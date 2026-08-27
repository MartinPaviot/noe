//! L'adaptateur Gmail, minimal et en lecture seule (spec 003, tâche 11, design §5).
//!
//! ## Ce qu'il sert, et rien de plus
//!
//! Trois usages, nommés par le design §5 : **la résolution par courriel** (en
//! jetons, R6.2), **les bornes de fils** — un fil qui gagne un message pendant
//! l'épisode est un changement observable — et, plus tard, le signal d'envoi.
//! Rien d'autre. En particulier, **aucun corps de message** (R6.3) : le contexte
//! pour les brouillons arrive avec la spec Assisté, avec ses propres règles.
//!
//! ## La liste blanche, et pourquoi ce n'est pas une liste noire
//!
//! `format=metadata` **a l'air** de suffire à respecter R6.3. Il ne suffit pas :
//! la réponse porte toujours `snippet`, c'est-à-dire les premiers caractères du
//! **corps** du message. Un adaptateur qui aurait retiré `snippet` par une liste
//! noire aurait été correct ce jour-là et faux le jour où Google ajoute un champ.
//! Ici l'état plat est construit en **nommant** ce qu'on prend, comme
//! `DATA_ADMIS` côté extension : ce qui n'est pas nommé n'entre pas, y compris ce
//! qui n'existe pas encore.
//!
//! ## Le filigrane d'historique
//!
//! `users.history.list` est un delta relatif à un `historyId`. Google ne garde
//! cet historique que quelques jours : un filigrane trop vieux rend **404**, et
//! ce n'est pas une panne — c'est une demande de resynchronisation. Les deux
//! mènent à des gestes opposés, donc ils portent des noms différents.
//!
//! Et le filigrane **n'avance qu'à la dernière page**. L'avancer au milieu d'une
//! pagination perdrait en silence tout ce que les pages suivantes portaient — un
//! trou qui ne se déclare pas, ce que la règle 4 interdit.
//!
//! ## Pas encore branché
//!
//! Comme l'adaptateur CRM : tout ce qui transforme et tout ce qui construit une
//! requête est ici et se vérifie sans réseau. L'échange OAuth demande une
//! application Google en mode test, que la tâche 0 crée.
#![allow(dead_code)] // retiré quand la tâche 0 crée l'application Google

use crate::federation::{EtatPlat, Issue, RefApi, Resolution};
use std::collections::{BTreeMap, BTreeSet};

/// La racine de l'API. Gmail n'a qu'une version, et elle est stable depuis 2014.
pub const BASE_API: &str = "/gmail/v1";

/// Le seul format que cet adaptateur sait demander.
///
/// `full` et `raw` rendent le corps ; les nommer ici, même dans une constante
/// inutilisée, laisserait un appelant les choisir. Ils n'existent pas dans ce
/// module.
pub const FORMAT: &str = "metadata";

/// Les en-têtes qu'on accepte de recevoir.
///
/// Demander tous les en-têtes ferait entrer `Received` — donc des adresses IP et
/// des noms de serveurs internes — et `Authentication-Results`, qui n'apprend
/// rien sur le travail de l'opérateur. On nomme les six qui servent.
///
/// `Subject` est un en-tête et R6.3 les autorise, mais il porte souvent une
/// identité : il traverse le **même** pipeline de rédaction que la capture, comme
/// tout le reste (R6.1).
pub const EN_TETES_ADMIS: &[&str] = &["From", "To", "Cc", "Date", "Subject", "Message-ID"];

/// Les types d'historique demandés.
///
/// `messageDeleted` est là parce qu'un message supprimé pendant l'épisode est un
/// changement, pas un non-événement.
pub const TYPES_HISTORIQUE: &[&str] = &[
    "messageAdded",
    "messageDeleted",
    "labelAdded",
    "labelRemoved",
];

/// Ce que l'adaptateur sait demander au réseau — le même que le CRM.
pub use crate::transport::Transport;

fn encoder(s: &str) -> String {
    crate::oauth::encoder_composant(s)
}

/// Le chemin d'un fil, en métadonnées seulement.
///
/// `format` et `metadataHeaders` sont posés ici et nulle part ailleurs :
/// l'appelant ne choisit pas le format, donc il ne peut pas se tromper.
pub fn chemin_fil(id: &str) -> String {
    let mut chemin = format!(
        "{BASE_API}/users/me/threads/{}?format={FORMAT}",
        encoder(id)
    );
    for en_tete in EN_TETES_ADMIS {
        chemin.push_str("&metadataHeaders=");
        chemin.push_str(&encoder(en_tete));
    }
    chemin
}

/// Le chemin d'une page d'historique.
pub fn chemin_historique(depuis: &str, page: Option<&str>) -> String {
    let mut chemin = format!(
        "{BASE_API}/users/me/history?startHistoryId={}&maxResults=100",
        encoder(depuis)
    );
    for t in TYPES_HISTORIQUE {
        chemin.push_str("&historyTypes=");
        chemin.push_str(&encoder(t));
    }
    if let Some(jeton) = page {
        chemin.push_str("&pageToken=");
        chemin.push_str(&encoder(jeton));
    }
    chemin
}

/// Ce qu'une page d'historique apprend.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageHistorique {
    /// Les fils touchés, dédupliqués. Le même fil revient dans plusieurs
    /// enregistrements — un message ajouté puis étiqueté en produit deux.
    pub fils_touches: BTreeSet<String>,
    /// Les messages disparus, qu'un fil relu ne montrera plus.
    pub messages_supprimes: BTreeSet<String>,
    /// La page suivante, s'il y en a une.
    pub page_suivante: Option<String>,
    /// Le nouveau filigrane — **présent seulement à la dernière page**.
    pub filigrane: Option<String>,
}

/// Ce qu'il faut faire ensuite (R5.2 : jamais une exception, toujours un verdict).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suite {
    /// Le delta est lisible.
    Delta(Box<PageHistorique>),
    /// Le filigrane est périmé : Google ne garde l'historique que quelques
    /// jours. Il faut repartir d'une lecture complète, et le **dire** — c'est un
    /// trou de couverture, avec sa cause, pas une panne à réessayer.
    ResynchronisationRequise,
    /// Le reste, classé comme partout ailleurs.
    Echec(Issue),
}

/// Lit une page d'historique.
///
/// Le filigrane sort du **corps de la réponse** (`historyId`) et jamais du plus
/// grand identifiant des enregistrements : la réponse en porte un même quand
/// `history` est absent, et s'arrêter d'avancer parce que rien n'a bougé ferait
/// redemander éternellement la même fenêtre.
pub fn lire_page_historique(corps: &serde_json::Value) -> PageHistorique {
    let vide = Vec::new();
    let page_suivante = corps
        .get("nextPageToken")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    // Le filigrane n'avance qu'une fois la pagination épuisée.
    let filigrane = if page_suivante.is_none() {
        corps
            .get("historyId")
            .and_then(valeur_en_texte)
            .filter(|s| !s.is_empty())
    } else {
        None
    };

    let mut page = PageHistorique {
        page_suivante,
        filigrane,
        ..PageHistorique::default()
    };

    let Some(enregistrements) = corps.get("history").and_then(serde_json::Value::as_array) else {
        return page;
    };
    for e in enregistrements {
        for cle in ["messagesAdded", "labelsAdded", "labelsRemoved"] {
            for entree in e
                .get(cle)
                .and_then(serde_json::Value::as_array)
                .unwrap_or(&vide)
            {
                if let Some(fil) = entree
                    .get("message")
                    .and_then(|m| m.get("threadId"))
                    .and_then(serde_json::Value::as_str)
                {
                    page.fils_touches.insert(fil.to_owned());
                }
            }
        }
        for entree in e
            .get("messagesDeleted")
            .and_then(serde_json::Value::as_array)
            .unwrap_or(&vide)
        {
            let message = entree.get("message");
            if let Some(fil) = message
                .and_then(|m| m.get("threadId"))
                .and_then(serde_json::Value::as_str)
            {
                page.fils_touches.insert(fil.to_owned());
            }
            if let Some(id) = message
                .and_then(|m| m.get("id"))
                .and_then(serde_json::Value::as_str)
            {
                page.messages_supprimes.insert(id.to_owned());
            }
        }
    }
    page
}

/// Un `historyId` arrive tantôt en texte, tantôt en nombre selon le client.
fn valeur_en_texte(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Classe une réponse d'historique.
pub fn classer_historique(statut: u16, corps: &str) -> Suite {
    // 404 sur `history` ne veut pas dire « absent » : il veut dire « ton
    // filigrane est plus vieux que ce que je garde ».
    if statut == 404 {
        return Suite::ResynchronisationRequise;
    }
    if statut == 200 {
        return match serde_json::from_str::<serde_json::Value>(corps) {
            Ok(v) => Suite::Delta(Box::new(lire_page_historique(&v))),
            Err(e) => Suite::Echec(Issue::Trou(format!("reponse illisible : {e}"))),
        };
    }
    Suite::Echec(classer_erreur(statut, corps))
}

/// Classe une erreur Google.
///
/// **403 ne se lit pas au code seul.** Le même 403 veut dire « ralentis »
/// (`rateLimitExceeded`, `userRateLimitExceeded`) ou « tu n'as pas le droit »
/// (`insufficientPermissions`) — et ces deux-là appellent des gestes opposés :
/// réessayer plus tard, ou sortir du périmètre et le déclarer. La `reason` est
/// stable ; le message, lui, est traduit et reformulé.
pub fn classer_erreur(statut: u16, corps: &str) -> Issue {
    let raison = serde_json::from_str::<serde_json::Value>(corps)
        .ok()
        .and_then(|v| {
            v.get("error")?
                .get("errors")?
                .as_array()?
                .first()?
                .get("reason")?
                .as_str()
                .map(str::to_owned)
        })
        .unwrap_or_default();

    match (statut, raison.as_str()) {
        (403, "rateLimitExceeded" | "userRateLimitExceeded") | (429, _) => {
            Issue::Trou(format!("quota : {statut} {raison}"))
        }
        (401, _) => Issue::Trou("jeton refuse : reauth requise".into()),
        (403, _) => Issue::HorsPerimetre(format!("droits insuffisants : {raison}")),
        (404, _) => Issue::HorsPerimetre("introuvable".into()),
        _ => Issue::Trou(format!("api gmail : {statut} {raison}")),
    }
}

/// Aplatit un fil en état plat, **par liste blanche**.
///
/// Ce qui sort, et rien d'autre :
/// - `thread.id`, `thread.message_count` — les bornes du fil ;
/// - `thread.labels` — ce que l'opérateur change quand il archive ou lit ;
/// - `thread.last_internal_date_ms` — l'horodatage **serveur** ;
/// - `thread.subject` — l'en-tête du premier message ;
/// - `thread.participants` — les adresses **normalisées, en clair**.
///
/// L'horodatage est `internalDate` et **pas l'en-tête `Date`** : le second est
/// posé par le client de l'expéditeur, il peut être faux de plusieurs heures et
/// se falsifie sans effort. C'est la même leçon que le mural et le monotone de la
/// spec 002 : deux horloges, et une seule fait foi pour ordonner.
///
/// Gmail rend `internalDate` **en texte**. Le laisser en texte ferait comparer
/// « 999 » et « 1000 » comme des mots, où le premier est le plus grand.
///
/// **`thread.participants` sort en clair et doit être tokenisé par l'appelant**
/// avant toute persistance (R6.2). La valeur claire ne vit qu'en mémoire.
pub fn aplatir_fil(fil: &serde_json::Value) -> EtatPlat {
    let mut plat = EtatPlat::new();
    let Some(id) = fil.get("id").and_then(serde_json::Value::as_str) else {
        return plat;
    };
    plat.insert("thread.id".into(), id.into());

    let vide = Vec::new();
    let messages = fil
        .get("messages")
        .and_then(serde_json::Value::as_array)
        .unwrap_or(&vide);
    plat.insert("thread.message_count".into(), messages.len().into());

    let mut etiquettes = BTreeSet::new();
    let mut participants: Vec<String> = Vec::new();
    let mut derniere = 0_i64;
    let mut sujet: Option<String> = None;

    for (rang, message) in messages.iter().enumerate() {
        for e in message
            .get("labelIds")
            .and_then(serde_json::Value::as_array)
            .unwrap_or(&vide)
        {
            if let Some(s) = e.as_str() {
                etiquettes.insert(s.to_owned());
            }
        }
        if let Some(ms) = message
            .get("internalDate")
            .and_then(valeur_en_texte)
            .and_then(|s| s.parse::<i64>().ok())
        {
            derniere = derniere.max(ms);
        }
        let en_tetes = lire_en_tetes(message);
        for champ in ["From", "To", "Cc"] {
            if let Some(v) = en_tetes.get(champ) {
                for a in adresses_de_liste(v) {
                    if !participants.contains(&a) {
                        participants.push(a);
                    }
                }
            }
        }
        if rang == 0 {
            sujet = en_tetes.get("Subject").cloned();
        }
    }

    // Ordonnés : deux lectures du même fil doivent donner le même état, sinon le
    // juge verrait un changement là où l'ordre de parcours a seulement varié.
    participants.sort_unstable();

    plat.insert(
        "thread.labels".into(),
        etiquettes.into_iter().collect::<Vec<_>>().into(),
    );
    if derniere > 0 {
        plat.insert("thread.last_internal_date_ms".into(), derniere.into());
    }
    if let Some(s) = sujet {
        plat.insert("thread.subject".into(), s.into());
    }
    plat.insert("thread.participants".into(), participants.into());
    plat
}

/// Les en-têtes d'un message, **filtrés par la liste blanche**.
///
/// Le filtre est appliqué ici aussi, et pas seulement à la requête : Google peut
/// rendre plus que ce qu'on a demandé, et un adaptateur qui ferait confiance à sa
/// propre requête n'aurait aucune défense ce jour-là.
pub fn lire_en_tetes(message: &serde_json::Value) -> BTreeMap<String, String> {
    let mut sortie = BTreeMap::new();
    let vide = Vec::new();
    let en_tetes = message
        .get("payload")
        .and_then(|p| p.get("headers"))
        .and_then(serde_json::Value::as_array)
        .unwrap_or(&vide);
    for h in en_tetes {
        let (Some(nom), Some(valeur)) = (
            h.get("name").and_then(serde_json::Value::as_str),
            h.get("value").and_then(serde_json::Value::as_str),
        ) else {
            continue;
        };
        // Les noms d'en-tête sont insensibles à la casse (RFC 5322) et Gmail rend
        // la casse d'origine : « from » et « From » arrivent tous les deux.
        if let Some(admis) = EN_TETES_ADMIS.iter().find(|a| a.eq_ignore_ascii_case(nom)) {
            sortie.insert((*admis).to_owned(), valeur.to_owned());
        }
    }
    sortie
}

/// Extrait les adresses d'une liste d'en-tête, normalisées.
///
/// **La virgule ne suffit pas à séparer.** Un nom affiché entre guillemets en
/// contient : `"Dupont, Jean" <j@ex.com>` est UNE adresse, et découper bêtement
/// en produirait deux, dont une sans arobase. On suit donc l'état — dans les
/// guillemets, dans les chevrons, ou dehors.
///
/// La normalisation est celle de `normaliserIdentifiant('email_token')` côté
/// TypeScript : `trim` puis minuscules. **Les mêmes règles des deux côtés**,
/// sinon deux graphies d'une adresse donneraient deux jetons et la jointure
/// serait perdue sans que personne ne le voie.
pub fn adresses_de_liste(valeur: &str) -> Vec<String> {
    let mut sorties = Vec::new();
    let mut courant = String::new();
    let mut dans_guillemets = false;
    let mut dans_chevrons = false;

    for c in valeur.chars() {
        match c {
            '"' if !dans_chevrons => dans_guillemets = !dans_guillemets,
            '<' if !dans_guillemets => {
                dans_chevrons = true;
                // Le nom affiché est jeté : l'adresse fait foi.
                courant.clear();
            }
            '>' if dans_chevrons => dans_chevrons = false,
            ',' if !dans_guillemets && !dans_chevrons => {
                pousser_adresse(&mut sorties, &courant);
                courant.clear();
            }
            _ => courant.push(c),
        }
    }
    pousser_adresse(&mut sorties, &courant);
    sorties
}

fn pousser_adresse(sorties: &mut Vec<String>, brut: &str) {
    let a = brut.trim().to_lowercase();
    // Une adresse sans arobase n'est pas une adresse : c'est un reste de nom
    // affiché, ou un groupe RFC (`undisclosed-recipients:;`).
    if a.contains('@') && !a.starts_with('@') && !a.ends_with('@') && !sorties.contains(&a) {
        sorties.push(a);
    }
}

/// Les clés fortes qu'un fil apporte à la résolution (R2.1).
///
/// **En clair** : c'est l'appelant qui tokenise, parce que la clé HMAC vit dans
/// le coffre et pas ici.
pub fn cles_du_fil(plat: &EtatPlat) -> Vec<(String, String)> {
    plat.get("thread.participants")
        .and_then(serde_json::Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(serde_json::Value::as_str)
                .map(|a| ("email_token".to_owned(), a.to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un fil tel que Gmail le rend en `format=metadata` — **avec** son
    /// `snippet`, parce que c'est exactement ce que fait l'API.
    fn fil_enregistre() -> serde_json::Value {
        serde_json::json!({
            "id": "18f0c1",
            "historyId": "993344",
            "snippet": "Bonjour Jean, ci-joint le devis de 12 400 EUR pour",
            "messages": [
                {
                    "id": "m1",
                    "threadId": "18f0c1",
                    "labelIds": ["INBOX", "UNREAD"],
                    "snippet": "Bonjour Jean, ci-joint le devis",
                    "internalDate": "1717171717000",
                    "sizeEstimate": 41233,
                    "payload": {
                        "mimeType": "multipart/mixed",
                        "filename": "",
                        "headers": [
                            {"name": "From", "value": "Marie Martin <Marie.Martin@Exemple.FR>"},
                            {"name": "To", "value": "jean@client.com"},
                            {"name": "Subject", "value": "Devis Dupont"},
                            {"name": "Date", "value": "Mon, 1 Jan 2019 00:00:00 +0000"},
                            {"name": "Received", "value": "from mx.exemple.fr (10.4.2.19)"}
                        ]
                    }
                },
                {
                    "id": "m2",
                    "threadId": "18f0c1",
                    "labelIds": ["INBOX", "SENT"],
                    "internalDate": "1717175555000",
                    "payload": {
                        "headers": [
                            {"name": "from", "value": "jean@client.com"},
                            {"name": "To", "value": "marie.martin@exemple.fr"},
                            {"name": "Subject", "value": "Re: Devis Dupont"}
                        ]
                    }
                }
            ]
        })
    }

    // -- R6.3 : jamais de corps --------------------------------------------

    #[test]
    fn le_format_demande_est_toujours_metadata() {
        let c = chemin_fil("18f0c1");
        assert!(c.contains("format=metadata"), "{c}");
    }

    #[test]
    fn aucun_chemin_ne_sait_demander_le_corps() {
        // Le module entier est relu : ce n'est pas une propriété d'une fonction,
        // c'est une propriété de l'adaptateur. Les motifs sont assemblés pour que
        // CE test ne se trouve pas lui-même.
        let source = include_str!("gmail.rs");
        for interdit in [format!("format={}", "full"), format!("format={}", "raw")] {
            assert!(
                !source.contains(&interdit),
                "le module sait demander {interdit}"
            );
        }
    }

    #[test]
    fn le_snippet_n_entre_pas_dans_l_etat() {
        // `format=metadata` a l'air de suffire à R6.3. Il ne suffit pas : la
        // réponse porte `snippet`, qui est le début du CORPS.
        let plat = aplatir_fil(&fil_enregistre());
        let entier = serde_json::to_string(&plat).unwrap();
        assert!(!entier.contains("snippet"), "{entier}");
        assert!(!entier.contains("ci-joint"), "le corps a fuite :\n{entier}");
        assert!(!entier.contains("12 400"), "le montant a fuite :\n{entier}");
    }

    #[test]
    fn un_champ_inconnu_de_google_n_entre_pas() {
        // La différence entre une liste blanche et une liste noire tient dans ce
        // test : personne n'a écrit de règle contre `bodyPreview`, et il n'entre
        // pas quand même.
        let mut fil = fil_enregistre();
        fil["messages"][0]["bodyPreview"] = "le corps entier, en clair".into();
        fil["messages"][0]["textPlain"] = "encore le corps".into();
        let entier = serde_json::to_string(&aplatir_fil(&fil)).unwrap();
        assert!(!entier.contains("corps"), "{entier}");
        assert!(!entier.contains("bodyPreview"), "{entier}");
    }

    #[test]
    fn l_etat_ne_porte_que_les_cles_nommees() {
        let plat = aplatir_fil(&fil_enregistre());
        let attendues = [
            "thread.id",
            "thread.labels",
            "thread.last_internal_date_ms",
            "thread.message_count",
            "thread.participants",
            "thread.subject",
        ];
        let vues: Vec<&str> = plat.keys().map(String::as_str).collect();
        assert_eq!(vues, attendues, "l etat a gagne ou perdu une cle");
    }

    #[test]
    fn les_en_tetes_hors_liste_blanche_sont_jetes() {
        // `Received` porte les adresses IP et les noms de serveurs internes. Il
        // n'apprend rien sur le travail, et il en dit long sur l'infrastructure.
        let fil = fil_enregistre();
        let en_tetes = lire_en_tetes(&fil["messages"][0]);
        assert!(!en_tetes.contains_key("Received"));
        let entier = serde_json::to_string(&aplatir_fil(&fil)).unwrap();
        assert!(!entier.contains("10.4.2.19"), "{entier}");
    }

    #[test]
    fn les_noms_d_en_tete_sont_insensibles_a_la_casse() {
        // RFC 5322, et Gmail rend la casse d'origine : `from` arrive tel quel.
        let fil = fil_enregistre();
        assert_eq!(
            lire_en_tetes(&fil["messages"][1])
                .get("From")
                .map(String::as_str),
            Some("jean@client.com")
        );
    }

    // -- Les adresses -------------------------------------------------------

    #[test]
    fn une_virgule_dans_un_nom_affiche_ne_coupe_pas_l_adresse() {
        // Le piège classique : découper sur la virgule produirait deux morceaux,
        // dont un sans arobase — donc une adresse perdue et une invention.
        let liste = concat!("\"Dupont, Jean\" <j@ex.com>, ", "Marie <m@ex.com>");
        assert_eq!(adresses_de_liste(liste), vec!["j@ex.com", "m@ex.com"]);
    }

    #[test]
    fn le_nom_affiche_est_jete() {
        assert_eq!(
            adresses_de_liste("Jean Dupont <jean@ex.com>"),
            vec!["jean@ex.com"]
        );
    }

    #[test]
    fn deux_graphies_convergent_avant_le_jeton() {
        // Sinon « Jean.Dupont@Exemple.FR » et « jean.dupont@exemple.fr » seraient
        // deux personnes, et la jointure serait perdue sans que ça se voie.
        let a = adresses_de_liste("Jean.Dupont@Exemple.FR");
        let b = adresses_de_liste("  jean.dupont@exemple.fr ");
        assert_eq!(a, b);
        assert_eq!(a, vec!["jean.dupont@exemple.fr"]);
    }

    #[test]
    fn un_destinataire_masque_ne_produit_pas_d_adresse() {
        assert!(adresses_de_liste("undisclosed-recipients:;").is_empty());
        assert!(adresses_de_liste("").is_empty());
        assert!(adresses_de_liste("Jean Dupont").is_empty());
    }

    #[test]
    fn une_adresse_repetee_ne_compte_qu_une_fois() {
        assert_eq!(
            adresses_de_liste("a@ex.com, A@EX.COM, b@ex.com"),
            vec!["a@ex.com", "b@ex.com"]
        );
    }

    // -- L'horodatage -------------------------------------------------------

    #[test]
    fn l_horodatage_vient_du_serveur_et_pas_de_l_en_tete() {
        // L'en-tête `Date` dit 2019 ; il est posé par le client de l'expéditeur,
        // il peut être faux de plusieurs heures et se falsifie sans effort.
        let plat = aplatir_fil(&fil_enregistre());
        assert_eq!(
            plat.get("thread.last_internal_date_ms"),
            Some(&serde_json::json!(1_717_175_555_000_i64))
        );
        let entier = serde_json::to_string(&plat).unwrap();
        assert!(
            !entier.contains("2019"),
            "l en-tete Date a servi :\n{entier}"
        );
    }

    #[test]
    fn l_horodatage_se_compare_en_nombre_et_pas_en_texte() {
        // Gmail rend `internalDate` en TEXTE. Comparé comme un mot, « 999 » est
        // plus grand que « 1000 ».
        let fil = serde_json::json!({
            "id": "t",
            "messages": [
                {"internalDate": "1000", "payload": {"headers": []}},
                {"internalDate": "999", "payload": {"headers": []}}
            ]
        });
        assert_eq!(
            aplatir_fil(&fil).get("thread.last_internal_date_ms"),
            Some(&serde_json::json!(1000))
        );
    }

    // -- L'état plat --------------------------------------------------------

    #[test]
    fn les_bornes_du_fil_sont_lues() {
        let plat = aplatir_fil(&fil_enregistre());
        assert_eq!(plat["thread.id"], serde_json::json!("18f0c1"));
        assert_eq!(plat["thread.message_count"], serde_json::json!(2));
        assert_eq!(
            plat["thread.labels"],
            serde_json::json!(["INBOX", "SENT", "UNREAD"])
        );
        assert_eq!(plat["thread.subject"], serde_json::json!("Devis Dupont"));
    }

    #[test]
    fn les_participants_sont_ordonnes_et_normalises() {
        // Deux lectures du même fil doivent donner le MÊME état : un ordre qui
        // varie ferait voir au juge un changement là où rien n'a bougé.
        let plat = aplatir_fil(&fil_enregistre());
        assert_eq!(
            plat["thread.participants"],
            serde_json::json!(["jean@client.com", "marie.martin@exemple.fr"])
        );
    }

    #[test]
    fn un_fil_sans_identifiant_ne_produit_rien() {
        // Un état vide se lirait comme « tous les champs sont nuls » ; ici c'est
        // l'absence complète qui est juste, et l'appelant déclare son trou.
        assert!(aplatir_fil(&serde_json::json!({"messages": []})).is_empty());
    }

    #[test]
    fn les_cles_du_fil_sont_des_courriels() {
        let cles = cles_du_fil(&aplatir_fil(&fil_enregistre()));
        assert_eq!(
            cles,
            vec![
                ("email_token".to_owned(), "jean@client.com".to_owned()),
                (
                    "email_token".to_owned(),
                    "marie.martin@exemple.fr".to_owned()
                ),
            ]
        );
    }

    // -- Le filigrane -------------------------------------------------------

    #[test]
    fn le_filigrane_n_avance_pas_au_milieu_d_une_pagination() {
        // L'avancer ici perdrait en silence tout ce que la page suivante porte.
        let v = serde_json::json!({
            "historyId": "500",
            "nextPageToken": "PAGE2",
            "history": [{"messagesAdded": [{"message": {"id": "m", "threadId": "t1"}}]}]
        });
        let page = lire_page_historique(&v);
        assert_eq!(page.page_suivante.as_deref(), Some("PAGE2"));
        assert_eq!(page.filigrane, None, "le filigrane a avance trop tot");
    }

    #[test]
    fn le_filigrane_avance_meme_quand_rien_n_a_bouge() {
        // La réponse porte un `historyId` même sans `history`. Ne pas l'avancer
        // ferait redemander éternellement la même fenêtre.
        let page = lire_page_historique(&serde_json::json!({"historyId": "777"}));
        assert_eq!(page.filigrane.as_deref(), Some("777"));
        assert!(page.fils_touches.is_empty());
    }

    #[test]
    fn le_filigrane_se_lit_en_texte_comme_en_nombre() {
        let a = lire_page_historique(&serde_json::json!({"historyId": 777}));
        let b = lire_page_historique(&serde_json::json!({"historyId": "777"}));
        assert_eq!(a.filigrane, b.filigrane);
    }

    #[test]
    fn le_meme_fil_touche_deux_fois_ne_compte_qu_une_fois() {
        // Un message ajouté PUIS étiqueté produit deux enregistrements sur le
        // même fil. Le relire deux fois brûlerait le budget d'appels pour rien.
        let v = serde_json::json!({
            "historyId": "9",
            "history": [
                {"messagesAdded": [{"message": {"id": "m1", "threadId": "t1"}}]},
                {"labelsAdded": [{"message": {"id": "m1", "threadId": "t1"}}]},
                {"labelsRemoved": [{"message": {"id": "m2", "threadId": "t2"}}]}
            ]
        });
        let page = lire_page_historique(&v);
        assert_eq!(page.fils_touches.len(), 2);
        assert!(page.fils_touches.contains("t1") && page.fils_touches.contains("t2"));
    }

    #[test]
    fn un_message_supprime_touche_son_fil_et_se_nomme() {
        // Une suppression est un changement, pas un non-événement : le fil doit
        // être relu, sinon `state_after` garderait un message qui n'existe plus.
        let v = serde_json::json!({
            "historyId": "9",
            "history": [{"messagesDeleted": [{"message": {"id": "m9", "threadId": "t9"}}]}]
        });
        let page = lire_page_historique(&v);
        assert!(page.fils_touches.contains("t9"));
        assert!(page.messages_supprimes.contains("m9"));
    }

    #[test]
    fn un_filigrane_perime_demande_une_resynchronisation() {
        // 404 sur l'historique ne veut pas dire « absent » : Google ne garde le
        // journal que quelques jours. Le traiter comme une panne ferait réessayer
        // sans fin une fenêtre qui ne reviendra jamais.
        let corps = serde_json::json!({"error": {"code": 404}}).to_string();
        assert_eq!(
            classer_historique(404, &corps),
            Suite::ResynchronisationRequise
        );
    }

    #[test]
    fn une_page_lisible_devient_un_delta() {
        let corps = serde_json::json!({"historyId": "42"}).to_string();
        match classer_historique(200, &corps) {
            Suite::Delta(p) => assert_eq!(p.filigrane.as_deref(), Some("42")),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn une_page_illisible_est_un_trou_nomme() {
        match classer_historique(200, "{ pas du json") {
            Suite::Echec(Issue::Trou(c)) => assert!(c.contains("illisible"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn le_chemin_d_historique_pagine_et_borne() {
        let sans = chemin_historique("500", None);
        assert!(sans.contains("startHistoryId=500"), "{sans}");
        assert!(sans.contains("maxResults=100"), "{sans}");
        assert!(!sans.contains("pageToken"), "{sans}");
        let avec = chemin_historique("500", Some("P A/2"));
        assert!(avec.contains("pageToken=P%20A%2F2"), "{avec}");
    }

    // -- La classification des erreurs --------------------------------------

    fn erreur(reason: &str, message: &str) -> String {
        serde_json::json!({
            "error": {"errors": [{"reason": reason, "message": message}]}
        })
        .to_string()
    }

    #[test]
    fn deux_403_ne_veulent_pas_dire_la_meme_chose() {
        // Le même code, deux gestes opposés : attendre, ou sortir du périmètre.
        let quota = erreur("rateLimitExceeded", "Quota exceeded");
        let droits = erreur("insufficientPermissions", "Insufficient Permission");
        assert!(matches!(classer_erreur(403, &quota), Issue::Trou(_)));
        assert!(matches!(
            classer_erreur(403, &droits),
            Issue::HorsPerimetre(_)
        ));
    }

    #[test]
    fn la_classification_ignore_le_message_traduit() {
        // Le message est traduit et reformulé sans prévenir ; la `reason` est
        // stable. Classer sur le message casserait le jour d'un changement de
        // langue de compte.
        let fr = erreur("rateLimitExceeded", "Quota depasse pour cet utilisateur");
        let en = erreur("rateLimitExceeded", "Rate Limit Exceeded");
        assert_eq!(classer_erreur(403, &fr), classer_erreur(403, &en));
    }

    #[test]
    fn un_429_est_un_trou_et_pas_un_hors_perimetre() {
        assert!(matches!(classer_erreur(429, "{}"), Issue::Trou(_)));
    }

    #[test]
    fn un_401_demande_une_reauth() {
        match classer_erreur(401, "{}") {
            Issue::Trou(c) => assert!(c.contains("reauth"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn un_corps_illisible_ne_fait_pas_paniquer_la_classification() {
        // Un proxy d'entreprise rend volontiers du HTML sur un 502.
        assert!(matches!(
            classer_erreur(502, "<html>Bad Gateway</html>"),
            Issue::Trou(_)
        ));
    }
}

/// Le nom du connecteur, tel qu'il apparaît dans les `api_refs`.
pub const CONNECTEUR: &str = "gmail";

/// Le seul objet que ce connecteur manipule.
pub const OBJET: &str = "thread";

/// L'adaptateur, une fois le transport branché.
pub struct Adaptateur<T: Transport> {
    transport: T,
}

impl<T: Transport> Adaptateur<T> {
    pub fn nouveau(transport: T) -> Self {
        Self { transport }
    }

    /// Va chercher un fil. Rend le corps si le fil existe.
    fn fil(&self, id: &str) -> Result<Option<serde_json::Value>, String> {
        let (statut, corps) = self
            .transport
            .get(&chemin_fil(id))
            .map_err(|e| format!("transport : {e}"))?;
        match statut {
            200 => serde_json::from_str(&corps)
                .map(Some)
                .map_err(|e| format!("reponse illisible : {e}")),
            // On a regardé, et il n'y a rien. C'est une conclusion, contrairement
            // à tous les autres cas.
            404 => Ok(None),
            _ => Err(match classer_erreur(statut, &corps) {
                Issue::Trou(c) | Issue::HorsPerimetre(c) => c,
                Issue::Lu(_) => "reponse inattendue".into(),
            }),
        }
    }
}

impl<T: Transport> crate::federation::Federation for Adaptateur<T> {
    /// R2.1 — la résolution d'un fil.
    ///
    /// **Un fil ne se résout que par son identifiant.** La capture le lit dans
    /// l'URL ; c'est une clé forte, exacte, émise par Gmail lui-même.
    ///
    /// Résoudre un fil par un participant serait une ambiguïté par construction :
    /// « le fil de jean@ex.com » désigne tous les fils où cette personne est
    /// jamais apparue, et R2.2 interdit de trancher entre eux. Le rôle du
    /// courriel ici est **inverse** — c'est Gmail qui *fournit* la clé qui
    /// résoudra le contact dans le CRM (`cles_du_fil`), pas l'inverse.
    ///
    /// L'identifiant est **confirmé par un appel** avant d'être rendu. Un
    /// `api_ref` posé sans vérification pointerait peut-être sur rien, et
    /// l'INVARIANT 7 promeut au grade A les épisodes dont toutes les entités
    /// pointent vers de vrais enregistrements — une confirmation supposée n'en
    /// est pas une.
    fn resoudre(&self, cles: &[(String, String)]) -> Resolution {
        let Some((_, id)) = cles.iter().find(|(genre, _)| genre == "system_id") else {
            return Resolution::Empechee(
                "aucune cle exploitable : un fil ne se resout que par son identifiant".into(),
            );
        };
        match self.fil(id) {
            Ok(Some(_)) => Resolution::Resolue {
                reference: RefApi {
                    connector: CONNECTEUR.into(),
                    object: OBJET.into(),
                    id: id.clone(),
                },
                par: "system_id".into(),
                quand: crate::federation::maintenant_iso(),
            },
            Ok(None) => Resolution::Introuvable,
            Err(cause) => Resolution::Empechee(cause),
        }
    }

    /// R3.1 — l'état d'un fil, restreint au périmètre.
    ///
    /// Le périmètre filtre **par-dessus** la liste blanche, il ne l'élargit
    /// jamais : demander `thread.body` ne le fait pas apparaître. Et un périmètre
    /// qui ne parle d'aucun champ de fil rend `HorsPerimetre` plutôt qu'un état
    /// vide — un état vide se lirait comme « tous les champs sont nuls », et le
    /// diff inventerait des changements qui n'ont pas eu lieu.
    fn lire(&self, reference: &RefApi, champs: &[String]) -> Issue {
        if reference.connector != CONNECTEUR {
            return Issue::HorsPerimetre(format!("connecteur {}", reference.connector));
        }
        let fil = match self.fil(&reference.id) {
            Ok(Some(v)) => v,
            Ok(None) => return Issue::Trou("fil disparu entre la resolution et la lecture".into()),
            Err(cause) => return Issue::Trou(cause),
        };
        let plat = aplatir_fil(&fil);
        let voulus: std::collections::BTreeSet<&str> = champs.iter().map(String::as_str).collect();
        let retenu: EtatPlat = plat
            .into_iter()
            .filter(|(cle, _)| voulus.contains(cle.as_str()))
            .collect();
        if retenu.is_empty() {
            return Issue::HorsPerimetre("aucun champ du perimetre ne concerne un fil".into());
        }
        Issue::Lu(retenu)
    }
}

#[cfg(test)]
mod tests_adaptateur {
    use super::*;
    use crate::federation::Federation;

    struct Faux<F: Fn(&str) -> (u16, String) + Send + Sync>(F);
    impl<F: Fn(&str) -> (u16, String) + Send + Sync> Transport for Faux<F> {
        fn get(&self, chemin: &str) -> Result<(u16, String), String> {
            Ok((self.0)(chemin))
        }
    }

    fn fil_json() -> String {
        serde_json::json!({
            "id": "18f0c1",
            "snippet": "le corps du message, en clair",
            "messages": [{
                "id": "m1",
                "labelIds": ["INBOX", "UNREAD"],
                "internalDate": "1717171717000",
                "payload": {"headers": [
                    {"name": "From", "value": "marie@ex.com"},
                    {"name": "Subject", "value": "Devis"}
                ]}
            }]
        })
        .to_string()
    }

    fn adaptateur<F: Fn(&str) -> (u16, String) + Send + Sync>(f: F) -> Adaptateur<Faux<F>> {
        Adaptateur::nouveau(Faux(f))
    }

    fn reference() -> RefApi {
        RefApi {
            connector: CONNECTEUR.into(),
            object: OBJET.into(),
            id: "18f0c1".into(),
        }
    }

    #[test]
    fn un_identifiant_de_fil_confirme_resout() {
        let a = adaptateur(|_| (200, fil_json()));
        match a.resoudre(&[("system_id".into(), "18f0c1".into())]) {
            Resolution::Resolue { reference, par, .. } => {
                assert_eq!(par, "system_id");
                assert_eq!(reference.connector, "gmail");
                assert_eq!(reference.object, "thread");
                assert_eq!(reference.id, "18f0c1");
            }
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn l_identifiant_est_confirme_par_un_appel_et_pas_suppose() {
        // L'INVARIANT 7 promeut au grade A les episodes dont toutes les entites
        // pointent vers de VRAIS enregistrements. Une confirmation supposee n'en
        // est pas une — et un `api_ref` pose sans verification pointerait
        // peut-etre sur rien.
        //
        // La preuve qu'un appel part vraiment : la reponse DEPEND du statut.
        // Sans appel, un identifiant absent resoudrait quand meme.
        let absent = adaptateur(|_| (404, "{}".into()));
        assert_eq!(
            absent.resoudre(&[("system_id".into(), "18f0c1".into())]),
            Resolution::Introuvable
        );
    }

    #[test]
    fn un_quota_empeche_au_lieu_de_conclure() {
        // 404 dit « il n'existe pas » ; 429 dit « je n'ai pas pu regarder ». Les
        // confondre ferait affirmer une absence qu'on n'a pas constatee.
        let a = adaptateur(|_| (429, "{}".into()));
        match a.resoudre(&[("system_id".into(), "18f0c1".into())]) {
            Resolution::Empechee(c) => assert!(c.contains("quota"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn un_courriel_seul_ne_resout_pas_un_fil() {
        // « Le fil de jean@ex.com » designe tous les fils ou cette personne est
        // jamais apparue. C'est une ambiguite par construction, et R2.2 interdit
        // de trancher entre eux.
        let a = adaptateur(|_| panic!("aucun appel ne devrait partir"));
        match a.resoudre(&[("email_token".into(), "jean@ex.com".into())]) {
            Resolution::Empechee(c) => assert!(c.contains("identifiant"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn une_lecture_rend_les_bornes_du_fil_restreintes_au_perimetre() {
        let a = adaptateur(|_| (200, fil_json()));
        let champs = vec![
            "thread.labels".to_owned(),
            "thread.message_count".to_owned(),
        ];
        match a.lire(&reference(), &champs) {
            Issue::Lu(plat) => {
                assert_eq!(plat.len(), 2, "{plat:?}");
                assert_eq!(plat["thread.message_count"], serde_json::json!(1));
                assert!(!plat.contains_key("thread.subject"), "hors perimetre");
            }
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn le_perimetre_filtre_par_dessus_la_liste_blanche_et_ne_l_elargit_pas() {
        // Demander `thread.body` ne le fait pas apparaitre : la liste blanche
        // decide de ce qui existe, le perimetre de ce qu'on en garde.
        let a = adaptateur(|_| (200, fil_json()));
        let champs = vec!["thread.body".to_owned(), "thread.id".to_owned()];
        match a.lire(&reference(), &champs) {
            Issue::Lu(plat) => {
                let entier = serde_json::to_string(&plat).unwrap();
                assert!(!entier.contains("body"), "{entier}");
                assert!(!entier.contains("corps"), "le corps a fuite : {entier}");
                assert_eq!(plat.len(), 1);
            }
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn un_perimetre_qui_ne_parle_pas_de_fil_est_hors_perimetre() {
        // Plutot qu'un etat vide, qui se lirait comme « tous les champs sont
        // nuls » et ferait inventer au diff des changements qui n'ont pas eu lieu.
        let a = adaptateur(|_| (200, fil_json()));
        match a.lire(&reference(), &["Statut__c".to_owned()]) {
            Issue::HorsPerimetre(c) => assert!(c.contains("fil"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn une_reference_d_un_autre_connecteur_est_hors_perimetre() {
        let a = adaptateur(|_| panic!("aucun appel ne devrait partir"));
        let mut r = reference();
        r.connector = "salesforce".into();
        match a.lire(&r, &["thread.id".to_owned()]) {
            Issue::HorsPerimetre(c) => assert!(c.contains("salesforce"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn un_fil_disparu_entre_la_resolution_et_la_lecture_est_un_trou() {
        // Ce n'est pas « hors perimetre » : le fil etait la, on l'avait resolu.
        // Le perdre est un trou de capture, et la regle 4 dit qu'il s'enregistre.
        let a = adaptateur(|_| (404, "{}".into()));
        match a.lire(&reference(), &["thread.id".to_owned()]) {
            Issue::Trou(c) => assert!(c.contains("disparu"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }
}
