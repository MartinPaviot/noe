//! Les candidates : d'où viennent les clés fortes, et à qui elles s'adressent
//! (spec 003, R1.1, R2.1, et le hook de première vue de la tâche 6).
//!
//! ## Ce que la capture voit vraiment
//!
//! La spec 002 ne capture ni sélecteur, ni coordonnée, ni corps de page : elle
//! capture des **noms accessibles** — le libellé de l'élément visé, les nœuds du
//! conteneur photographié. C'est peu, et c'est assez : une barre d'adresse est un
//! nœud accessible dont le nom est l'URL, et un en-tête de fiche porte le nom du
//! dossier ouvert.
//!
//! ## Ce module ne nomme aucun système
//!
//! R1.1 : **le code ne doit jamais encoder le CRM hors de son adaptateur.** Les
//! formes d'URL, les algorithmes de contrôle d'identifiant, le fait qu'une
//! adresse désigne une personne « ici » et pas « là » — ce sont des faits sur un
//! système donné, et ils vivent dans l'adaptateur de ce système. Ce module ne
//! fait que deux choses : les outils de lecture partagés, et l'aiguillage vers
//! les connecteurs **que `terrain.json` déclare**.
//!
//! Changer de CRM, c'est changer une ligne de `terrain.json`.
//!
//! ## Les valeurs sortent EN CLAIR, et c'est voulu
//!
//! R6.2 : les valeurs d'identification sont **tokenisées à la volée** et
//! comparées en jetons ; la valeur claire ne vit qu'en mémoire. Ce module est en
//! amont de ce point : il rend ce qu'il a lu, tel quel, parce qu'on ne peut pas
//! interroger un CRM avec un HMAC. **L'appelant tokenise avant toute
//! persistance**, et rien d'ici ne doit atteindre un fichier.
//!
//! ## Pourquoi si peu de motifs
//!
//! Un faux positif de résolution est pire qu'une absence de résolution : il
//! attribue le travail d'un opérateur au dossier de quelqu'un d'autre, et rien en
//! aval ne le rattrape — le graphe est faux et il a l'air juste. Les adaptateurs
//! n'extraient donc que des formes **non ambiguës**. Pas de « nom qui ressemble
//! à », pas de numéro à sept chiffres qui pourrait être un identifiant.

#![allow(dead_code)] // retiré quand la tâche 0 permet de brancher le worker

use crate::motifs;
use crate::terrain::Terrain;

/// Une entité candidate, telle que la capture la voit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Candidate {
    /// À quel système elle s'adresse. Une adresse de courriel désigne une
    /// personne dans le CRM ; un identifiant de fil désigne un fil dans la
    /// messagerie. Ce ne sont pas les mêmes entités, et elles ne se résolvent pas
    /// au même endroit.
    pub connecteur: String,
    /// Ses clés fortes, **en clair**.
    pub cles: Vec<(String, String)>,
}

impl Candidate {
    /// La clé qui sert de référence stable pour cette candidate.
    ///
    /// C'est elle que l'appelant tokenise pour fabriquer l'identifiant de
    /// candidate. **La construction de cet identifiant n'est pas ici** : elle
    /// demande la clé HMAC, qui vit dans le coffre.
    pub fn cle_de_reference(&self) -> Option<&(String, String)> {
        self.cles.first()
    }

    /// Les clés telles que le worker les reçoit : les fortes, **plus la
    /// destination**.
    ///
    /// Le worker ne connaît qu'une fédération et n'a pas à savoir combien de
    /// systèmes sont branchés. C'est donc la candidate qui porte sa destination,
    /// et le routeur qui la lit puis la retire.
    pub fn cles_routees(&self) -> Vec<(String, String)> {
        let mut sortie = vec![(
            crate::federation::CLE_CONNECTEUR.to_owned(),
            self.connecteur.clone(),
        )];
        sortie.extend(self.cles.iter().cloned());
        sortie
    }
}

/// Ce qu'un connecteur sait reconnaître de lui-même dans un texte.
pub type Extracteur = fn(&str) -> Vec<Vec<(String, String)>>;

/// L'aiguillage : un nom de connecteur, l'extracteur de son adaptateur.
///
/// C'est le **seul** endroit qui associe un nom à du code, et il ne décide de
/// rien : le nom vient de `terrain.json`. Un connecteur que ce binaire ne connaît
/// pas rend `None`, et l'appelant le dit — plutôt que de tomber en silence sur
/// celui d'à côté.
pub fn extracteur(connecteur: &str) -> Option<Extracteur> {
    match connecteur {
        crate::salesforce::CONNECTEUR => Some(crate::salesforce::cles_du_texte),
        crate::gmail::CONNECTEUR => Some(crate::gmail::cles_du_texte),
        _ => None,
    }
}

/// Les candidates qu'un texte vu à l'écran fait apparaître, pour ce terrain.
///
/// L'ordre est celui des connecteurs déclarés : le CRM d'abord, la messagerie
/// ensuite. Deux lectures du même texte rendent la même liste — une candidate
/// dont l'ordre varie deviendrait deux candidates au fil des épisodes.
pub fn candidates_du_texte(texte: &str, terrain: &Terrain) -> Vec<Candidate> {
    let mut sorties = Vec::new();
    for connecteur in terrain.connecteurs() {
        let Some(extraire) = extracteur(connecteur) else {
            continue;
        };
        for cles in extraire(texte) {
            sorties.push(Candidate {
                connecteur: connecteur.to_owned(),
                cles,
            });
        }
    }
    sorties
}

/// Coupe un segment d'URL à ce qui ne peut plus en faire partie.
///
/// **Une URL lue à l'écran n'est presque jamais seule.** Elle est suivie d'une
/// espace, d'un guillemet, d'un point d'interrogation. Sans cette coupe, un
/// identifiant suivi d'un mot avait la mauvaise longueur et l'entité disparaissait
/// en silence — le pire des échecs, parce qu'il ressemble à « rien à voir ici ».
pub fn segment(brut: &str) -> &str {
    let fin = brut
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(brut.len());
    &brut[..fin]
}

/// Toutes les positions d'une aiguille dans une botte de foin.
pub fn indices_de(texte: &str, aiguille: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut depuis = 0;
    while let Some(i) = texte[depuis..].find(aiguille) {
        positions.push(depuis + i);
        depuis += i + 1;
    }
    positions
}

/// Les adresses de courriel lues dans un texte, normalisées.
///
/// La détection est celle de la bibliothèque de motifs — **la même** que celle
/// qui redacte. Deux détecteurs différents laisseraient passer d'un côté ce que
/// l'autre attrape, et la valeur qui échappe à la résolution est justement celle
/// qui a échappé à la redaction.
///
/// La normalisation est celle de `normaliser_identifiant`, la seule du dépôt —
/// et non une copie de ses règles, qui finirait par en différer.
pub fn courriels(texte: &str) -> Vec<String> {
    let normalise = motifs::normaliser_blancs(texte);
    let mut trouves = Vec::new();
    for occurrence in motifs::chercher(texte) {
        if occurrence.type_pii != "EMAIL" {
            continue;
        }
        let Some(brut) = normalise.get(occurrence.debut..occurrence.fin) else {
            continue;
        };
        let adresse = crate::federation::normaliser_identifiant("email_token", brut);
        if !adresse.is_empty() && !trouves.contains(&adresse) {
            trouves.push(adresse);
        }
    }
    trouves
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terrain(json: serde_json::Value) -> Terrain {
        Terrain::analyser(&json.to_string()).expect("terrain")
    }

    fn terrain_complet() -> Terrain {
        terrain(serde_json::json!({"crm": "salesforce", "mail": "gmail"}))
    }

    const URL_FICHE: &str = "/lightning/r/Contact/0035g00000LmT4EAAV/view";
    const URL_FIL: &str = "https://mail.google.com/mail/u/0/#inbox/18f0c1a2b3c4d5e6";

    #[test]
    fn une_fiche_va_au_crm_que_le_terrain_declare() {
        let c = candidates_du_texte(URL_FICHE, &terrain_complet());
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0].connecteur, "salesforce");
        assert_eq!(c[0].cles[0].0, "system_id");
    }

    #[test]
    fn un_fil_va_a_la_messagerie_que_le_terrain_declare() {
        let c = candidates_du_texte(URL_FIL, &terrain_complet());
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0].connecteur, "gmail");
    }

    #[test]
    fn un_terrain_sans_messagerie_ne_produit_pas_de_candidate_de_fil() {
        // R1.1 : le choix vient du fichier. Retirer la messagerie du terrain
        // suffit a ce que plus rien n'aille la chercher.
        let sans_mail = terrain(serde_json::json!({"crm": "salesforce"}));
        assert!(candidates_du_texte(URL_FIL, &sans_mail).is_empty());
        assert_eq!(candidates_du_texte(URL_FICHE, &sans_mail).len(), 1);
    }

    #[test]
    fn un_connecteur_inconnu_de_ce_binaire_ne_tombe_pas_sur_le_voisin() {
        // Un terrain peut nommer un CRM que cette version ne sait pas parler.
        // Tomber en silence sur celui d'a cote donnerait des candidates fausses.
        let hubspot = terrain(serde_json::json!({"crm": "hubspot", "mail": "gmail"}));
        let c = candidates_du_texte(URL_FICHE, &hubspot);
        assert!(c.is_empty(), "{c:?}");
        assert!(extracteur("hubspot").is_none());
    }

    #[test]
    fn le_crm_passe_avant_la_messagerie() {
        // Deux lectures du meme texte doivent rendre la meme liste : une
        // candidate dont l'ordre varie deviendrait deux candidates au fil des
        // episodes.
        let texte = format!("{URL_FIL} et {URL_FICHE}");
        let c = candidates_du_texte(&texte, &terrain_complet());
        assert_eq!(c.len(), 2, "{c:?}");
        assert_eq!(c[0].connecteur, "salesforce");
        assert_eq!(c[1].connecteur, "gmail");
        assert_eq!(c, candidates_du_texte(&texte, &terrain_complet()));
    }

    #[test]
    fn une_adresse_lue_a_l_ecran_ne_va_qu_au_crm() {
        // Elle designe une personne. « Le fil de jean@ex.com » designerait tous
        // ses fils, ce qui est une ambiguite par construction.
        let c = candidates_du_texte(
            "De : Jean Dupont <Jean.Dupont@Exemple.FR>",
            &terrain_complet(),
        );
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0].connecteur, "salesforce");
        assert_eq!(c[0].cles[0].1, "jean.dupont@exemple.fr");
    }

    #[test]
    fn la_cle_de_reference_est_la_premiere() {
        let c = candidates_du_texte(URL_FICHE, &terrain_complet());
        assert_eq!(
            c[0].cle_de_reference().map(|(g, _)| g.as_str()),
            Some("system_id")
        );
    }

    #[test]
    fn les_cles_routees_portent_la_destination_en_tete() {
        let c = candidates_du_texte(URL_FICHE, &terrain_complet());
        let routees = c[0].cles_routees();
        assert_eq!(routees[0].0, crate::federation::CLE_CONNECTEUR);
        assert_eq!(routees[0].1, "salesforce");
        assert_eq!(routees.len(), 2);
    }

    #[test]
    fn un_texte_ordinaire_ne_produit_aucune_candidate() {
        let t = terrain_complet();
        assert!(candidates_du_texte("Enregistrer", &t).is_empty());
        assert!(candidates_du_texte("Devis 2026-014 pour Dupont SARL", &t).is_empty());
        assert!(candidates_du_texte("", &t).is_empty());
    }

    // -- Les outils partages ------------------------------------------------

    #[test]
    fn un_segment_s_arrete_a_ce_qui_ne_peut_pas_en_faire_partie() {
        assert_eq!(
            segment("0035g00000LmT4EAAV maintenant"),
            "0035g00000LmT4EAAV"
        );
        assert_eq!(segment("18f0c1?x=1"), "18f0c1");
        assert_eq!(segment("abc"), "abc");
        assert_eq!(segment(" abc"), "");
    }

    #[test]
    fn deux_graphies_d_une_adresse_ne_font_qu_une() {
        assert_eq!(
            courriels("Jean.Dupont@Exemple.FR et jean.dupont@exemple.fr"),
            vec!["jean.dupont@exemple.fr"]
        );
    }

    #[test]
    fn la_detection_de_courriel_est_celle_qui_redacte() {
        // Deux detecteurs differents laisseraient passer d'un cote ce que
        // l'autre attrape — et la valeur qui echappe a la resolution est
        // justement celle qui a echappe a la redaction.
        let texte = "jean\u{00a0}.\u{200b}dupont@exemple.fr";
        let vues = motifs::chercher(texte)
            .into_iter()
            .filter(|o| o.type_pii == "EMAIL")
            .count();
        assert_eq!(vues, courriels(texte).len());
    }
}
