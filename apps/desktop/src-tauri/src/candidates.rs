//! Les candidates : d'où viennent les clés fortes, et à qui elles s'adressent
//! (spec 003, R2.1 et le hook de première vue de la tâche 6).
//!
//! ## Ce que la capture voit vraiment
//!
//! La spec 002 ne capture ni sélecteur, ni coordonnée, ni corps de page : elle
//! capture des **noms accessibles** — le libellé de l'élément visé, les nœuds du
//! conteneur photographié. C'est peu, et c'est assez : une barre d'adresse est un
//! nœud accessible dont le nom est l'URL, et un en-tête de fiche porte le nom du
//! dossier ouvert.
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
//! aval ne le rattrape — le graphe est faux et il a l'air juste. On n'extrait
//! donc que des formes **non ambiguës** : une URL d'enregistrement Lightning, une
//! URL de fil Gmail, une adresse de courriel. Pas de « nom qui ressemble à », pas
//! de numéro à sept chiffres qui pourrait être un identifiant.

#![allow(dead_code)] // retiré quand la tâche 0 permet de brancher le worker

use crate::motifs;

/// Le connecteur du système de vérité métier.
pub const CONNECTEUR_CRM: &str = "salesforce";
/// Le connecteur de la messagerie.
pub const CONNECTEUR_COURRIEL: &str = "gmail";

/// Une entité candidate, telle que la capture la voit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Candidate {
    /// À quel système elle s'adresse. Une adresse de courriel désigne une
    /// personne dans le CRM ; un identifiant de fil désigne un fil chez Gmail.
    /// Ce ne sont pas les mêmes entités, et elles ne se résolvent pas au même
    /// endroit.
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

/// L'alphabet du suffixe de contrôle d'un identifiant Salesforce.
const ALPHABET_CONTROLE: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ012345";

/// Complète un identifiant Salesforce de 15 caractères en 18.
///
/// **Le même enregistrement a deux écritures.** Les URL en portent une de dix-huit
/// caractères, les APIs acceptent les deux, et certaines pages en affichent quinze.
/// Sans cette conversion, le même dossier produirait deux candidates, donc deux
/// entités, donc un graphe qui compte double.
///
/// Le suffixe encode la casse : trois blocs de cinq caractères, un bit par
/// caractère majuscule, et chaque bloc de cinq bits donne une lettre.
pub fn completer_identifiant(id15: &str) -> Option<String> {
    if id15.len() != 15 || !id15.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    let octets = id15.as_bytes();
    let mut sortie = id15.to_owned();
    for bloc in 0..3 {
        let mut index = 0_usize;
        for position in 0..5 {
            if octets[bloc * 5 + position].is_ascii_uppercase() {
                index |= 1 << position;
            }
        }
        sortie.push(ALPHABET_CONTROLE[index] as char);
    }
    Some(sortie)
}

/// Un identifiant de dix-huit caractères est-il cohérent ?
///
/// Son suffixe se recalcule à partir des quinze premiers. C'est ce qui distingue
/// un vrai identifiant d'une chaîne de dix-huit caractères qui lui ressemble —
/// et il y en a partout dans une interface.
pub fn identifiant_coherent(id18: &str) -> bool {
    id18.len() == 18
        && completer_identifiant(&id18[..15])
            .is_some_and(|attendu| attendu.eq_ignore_ascii_case(id18))
}

/// Normalise un identifiant Salesforce vers sa forme de dix-huit caractères.
///
/// Rend `None` pour tout ce qui n'est pas un identifiant : c'est le bon sens de
/// l'erreur. Une candidate en moins est un trou qui se voit ; une candidate de
/// trop est un faux dossier qui ne se voit pas.
pub fn normaliser_identifiant_crm(brut: &str) -> Option<String> {
    match brut.len() {
        15 => completer_identifiant(brut),
        18 if identifiant_coherent(brut) => Some(brut.to_owned()),
        _ => None,
    }
}

/// Cherche un identifiant d'enregistrement dans une URL Lightning.
///
/// La forme visée est `/lightning/r/<Objet>/<Id>/view`. On ne cherche **pas** un
/// identifiant isolé dans un libellé : dix-huit caractères alphanumériques, ça se
/// trouve dans n'importe quelle interface, et le suffixe de contrôle ne suffirait
/// pas à écarter toutes les coïncidences.
fn identifiants_lightning(texte: &str) -> Vec<String> {
    let mut trouves = Vec::new();
    for depart in indices_de(texte, "/lightning/r/") {
        let reste = &texte[depart + "/lightning/r/".len()..];
        let mut morceaux = reste.split('/');
        let (Some(_objet), Some(brut)) = (morceaux.next(), morceaux.next()) else {
            continue;
        };
        if let Some(id) = normaliser_identifiant_crm(segment(brut)) {
            if !trouves.contains(&id) {
                trouves.push(id);
            }
        }
    }
    trouves
}

/// Cherche un identifiant de fil dans une URL Gmail.
///
/// La forme visée est `mail.google.com/mail/u/<n>/#<boite>/<fil>`, où le fil est
/// une suite hexadécimale d'au moins seize caractères.
fn identifiants_fils(texte: &str) -> Vec<String> {
    let mut trouves = Vec::new();
    for depart in indices_de(texte, "mail.google.com/") {
        // Borné à l'URL : sans ça, un `#` situé bien plus loin dans le texte
        // serait pris pour la partie fragment de CETTE adresse.
        let reste = &texte[depart..];
        let reste = &reste[..reste.find(char::is_whitespace).unwrap_or(reste.len())];
        let Some(diese) = reste.find('#') else {
            continue;
        };
        // Après le dièse : `<boite>/<fil>`, puis éventuellement autre chose.
        let apres = &reste[diese + 1..];
        let fin = apres
            .find(|c: char| c.is_whitespace() || c == '"' || c == '?')
            .unwrap_or(apres.len());
        let mut morceaux = apres[..fin].split('/');
        let (Some(_boite), Some(fil)) = (morceaux.next(), morceaux.next()) else {
            continue;
        };
        let fil = segment(fil);
        if fil.len() >= 16 && fil.bytes().all(|b| b.is_ascii_hexdigit()) {
            let fil = fil.to_owned();
            if !trouves.contains(&fil) {
                trouves.push(fil);
            }
        }
    }
    trouves
}

/// Coupe un segment d'URL à ce qui ne peut plus en faire partie.
///
/// **Une URL lue à l'écran n'est presque jamais seule.** Elle est suivie d'une
/// espace, d'un guillemet, d'un point d'interrogation. Sans cette coupe, un
/// identifiant suivi d'un mot avait la mauvaise longueur et l'entité disparaissait
/// en silence — le pire des échecs, parce qu'il ressemble à « rien à voir ici ».
fn segment(brut: &str) -> &str {
    let fin = brut
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(brut.len());
    &brut[..fin]
}

/// Toutes les positions d'une aiguille dans une botte de foin.
fn indices_de(texte: &str, aiguille: &str) -> Vec<usize> {
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
/// La normalisation est celle de `normaliserIdentifiant('email_token')` :
/// `trim` puis minuscules.
fn courriels(texte: &str) -> Vec<String> {
    let normalise = motifs::normaliser_blancs(texte);
    let mut trouves = Vec::new();
    for occurrence in motifs::chercher(texte) {
        if occurrence.type_pii != "EMAIL" {
            continue;
        }
        let Some(brut) = normalise.get(occurrence.debut..occurrence.fin) else {
            continue;
        };
        let adresse = brut.trim().to_lowercase();
        if !adresse.is_empty() && !trouves.contains(&adresse) {
            trouves.push(adresse);
        }
    }
    trouves
}

/// Les candidates qu'un texte vu à l'écran fait apparaître.
///
/// L'ordre est celui de la force des clés : d'abord les identifiants système,
/// ensuite les courriels. Deux lectures du même texte rendent la même liste —
/// une candidate dont l'ordre varie deviendrait deux candidates au fil des
/// épisodes.
pub fn candidates_du_texte(texte: &str) -> Vec<Candidate> {
    let mut sorties = Vec::new();

    for id in identifiants_lightning(texte) {
        sorties.push(Candidate {
            connecteur: CONNECTEUR_CRM.to_owned(),
            cles: vec![("system_id".to_owned(), id)],
        });
    }
    for fil in identifiants_fils(texte) {
        sorties.push(Candidate {
            connecteur: CONNECTEUR_COURRIEL.to_owned(),
            cles: vec![("system_id".to_owned(), fil)],
        });
    }
    for adresse in courriels(texte) {
        // Une adresse désigne une **personne**, donc un dossier dans le CRM.
        // Elle ne désigne pas un fil : « le fil de jean@ex.com » veut dire tous
        // ses fils, ce qui est une ambiguïté par construction.
        sorties.push(Candidate {
            connecteur: CONNECTEUR_CRM.to_owned(),
            cles: vec![("email_token".to_owned(), adresse)],
        });
    }
    sorties
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Les identifiants Salesforce ---------------------------------------

    #[test]
    fn un_identifiant_de_quinze_se_complete_en_dix_huit() {
        // Le même enregistrement a deux ecritures. Sans cette conversion, le
        // meme dossier produirait deux candidates, donc deux entites, donc un
        // graphe qui compte double.
        assert_eq!(
            completer_identifiant("0035g00000LmT4E").as_deref(),
            Some("0035g00000LmT4EAAV")
        );
        assert_eq!(
            completer_identifiant("00Q5g00000AbCdE").as_deref(),
            Some("00Q5g00000AbCdEEAV")
        );
    }

    #[test]
    fn les_deux_ecritures_du_meme_enregistrement_convergent() {
        assert_eq!(
            normaliser_identifiant_crm("0035g00000LmT4E"),
            normaliser_identifiant_crm("0035g00000LmT4EAAV")
        );
    }

    #[test]
    fn une_chaine_de_dix_huit_caracteres_n_est_pas_un_identifiant() {
        // Dix-huit caracteres alphanumeriques, ca se trouve partout dans une
        // interface. Le suffixe de controle est ce qui les distingue.
        assert!(!identifiant_coherent("ABCDEFGHIJKLMNOPQR"));
        assert_eq!(normaliser_identifiant_crm("ABCDEFGHIJKLMNOPQR"), None);
        assert_eq!(normaliser_identifiant_crm("trop-court"), None);
        assert_eq!(normaliser_identifiant_crm(""), None);
    }

    #[test]
    fn la_casse_d_un_identifiant_est_significative() {
        // `normaliserIdentifiant('system_id')` ne touche pas a la casse cote
        // TypeScript : elle porte le suffixe de controle, donc de l'information.
        let a = normaliser_identifiant_crm("0035g00000LmT4E").unwrap();
        let b = normaliser_identifiant_crm("0035G00000LMT4E").unwrap();
        assert_ne!(a, b, "deux enregistrements differents");
    }

    // -- L'extraction -------------------------------------------------------

    #[test]
    fn une_url_lightning_donne_une_candidate_crm() {
        let texte =
            "https://monorg.lightning.force.com/lightning/r/Contact/0035g00000LmT4EAAV/view";
        let c = candidates_du_texte(texte);
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0].connecteur, "salesforce");
        assert_eq!(
            c[0].cles,
            vec![("system_id".to_owned(), "0035g00000LmT4EAAV".to_owned())]
        );
    }

    #[test]
    fn une_url_lightning_en_quinze_caracteres_donne_la_meme_candidate() {
        let long = candidates_du_texte("/lightning/r/Contact/0035g00000LmT4EAAV/view");
        let court = candidates_du_texte("/lightning/r/Contact/0035g00000LmT4E/view");
        assert_eq!(long, court);
    }

    #[test]
    fn une_url_lightning_sans_identifiant_valable_ne_donne_rien() {
        // Le bon sens de l'erreur : une candidate en moins est un trou qui se
        // voit, une candidate de trop est un faux dossier qui ne se voit pas.
        assert!(candidates_du_texte("/lightning/r/Contact/new").is_empty());
        assert!(candidates_du_texte("/lightning/r/Contact/").is_empty());
        assert!(candidates_du_texte("/lightning/o/Contact/list").is_empty());
    }

    #[test]
    fn une_url_de_fil_gmail_donne_une_candidate_de_messagerie() {
        let texte = "https://mail.google.com/mail/u/0/#inbox/18f0c1a2b3c4d5e6";
        let c = candidates_du_texte(texte);
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0].connecteur, "gmail");
        assert_eq!(
            c[0].cles,
            vec![("system_id".to_owned(), "18f0c1a2b3c4d5e6".to_owned())]
        );
    }

    #[test]
    fn une_url_suivie_d_un_mot_donne_quand_meme_sa_candidate() {
        // Une URL lue a l'ecran n'est presque jamais seule. Sans la coupe,
        // l'identifiant avait la mauvaise longueur et l'entite disparaissait en
        // silence — le pire des echecs, parce qu'il ressemble a « rien a voir ».
        let c = candidates_du_texte("Ouvrir /lightning/r/Contact/0035g00000LmT4EAAV maintenant");
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0].cles[0].1, "0035g00000LmT4EAAV");

        let f = candidates_du_texte("https://mail.google.com/mail/u/0/#inbox/18f0c1a2b3c4d5e6 lu");
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].cles[0].1, "18f0c1a2b3c4d5e6");
    }

    #[test]
    fn un_diese_plus_loin_dans_le_texte_n_est_pas_le_fragment_de_l_url() {
        // Borner a l'URL : sinon le `#` d'une phrase suivante devient la partie
        // fragment de CETTE adresse, et l'entite est fausse.
        assert!(candidates_du_texte(
            "https://mail.google.com/mail/u/0/ puis #inbox/18f0c1a2b3c4d5e6"
        )
        .is_empty());
    }

    #[test]
    fn une_boite_gmail_sans_fil_ouvert_ne_donne_rien() {
        assert!(candidates_du_texte("https://mail.google.com/mail/u/0/#inbox").is_empty());
        // Un identifiant trop court n'est pas un fil.
        assert!(candidates_du_texte("https://mail.google.com/mail/u/0/#inbox/1234").is_empty());
        // Ni une suite non hexadecimale.
        assert!(
            candidates_du_texte("https://mail.google.com/mail/u/0/#inbox/zzzzzzzzzzzzzzzz")
                .is_empty()
        );
    }

    #[test]
    fn une_adresse_lue_a_l_ecran_designe_une_personne_dans_le_crm() {
        // Elle ne designe PAS un fil : « le fil de jean@ex.com » veut dire tous
        // ses fils, ce qui est une ambiguite par construction.
        let c = candidates_du_texte("De : Jean Dupont <Jean.Dupont@Exemple.FR>");
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0].connecteur, "salesforce");
        assert_eq!(
            c[0].cles,
            vec![(
                "email_token".to_owned(),
                "jean.dupont@exemple.fr".to_owned()
            )]
        );
    }

    #[test]
    fn deux_graphies_d_une_adresse_ne_font_qu_une_candidate() {
        let c = candidates_du_texte("Jean.Dupont@Exemple.FR et jean.dupont@exemple.fr");
        assert_eq!(c.len(), 1, "{c:?}");
    }

    #[test]
    fn les_candidates_sortent_dans_l_ordre_de_force_des_cles() {
        // Deux lectures du meme texte doivent rendre la meme liste : une
        // candidate dont l'ordre varie deviendrait deux candidates au fil des
        // episodes.
        let texte = "jean@ex.com — /lightning/r/Contact/0035g00000LmT4EAAV/view";
        let c = candidates_du_texte(texte);
        assert_eq!(c.len(), 2, "{c:?}");
        assert_eq!(c[0].cles[0].0, "system_id");
        assert_eq!(c[1].cles[0].0, "email_token");
        assert_eq!(c, candidates_du_texte(texte), "deux lectures divergent");
    }

    #[test]
    fn un_texte_ordinaire_ne_produit_aucune_candidate() {
        // Pas de « nom qui ressemble a », pas de numero a sept chiffres.
        assert!(candidates_du_texte("Enregistrer").is_empty());
        assert!(candidates_du_texte("Devis 2026-014 pour Dupont SARL").is_empty());
        assert!(candidates_du_texte("").is_empty());
    }

    #[test]
    fn la_cle_de_reference_est_la_premiere() {
        let c = candidates_du_texte("/lightning/r/Lead/00Q5g00000AbCdEEAV/view");
        assert_eq!(
            c[0].cle_de_reference().map(|(g, _)| g.as_str()),
            Some("system_id")
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
        assert_eq!(vues, candidates_du_texte(texte).len());
    }
}
