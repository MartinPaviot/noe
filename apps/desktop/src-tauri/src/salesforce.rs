//! L'adaptateur Salesforce, en lecture seule (spec 003, tâche 4 et design §5).
//!
//! ## Ce qui est ici, et ce qui n'y est pas
//!
//! Tout ce qui **transforme** et tout ce qui **construit une requête** est ici, et
//! se teste sans réseau. Le seul morceau qui exige une org est l'appel HTTP
//! lui-même, isolé derrière un trait — c'est ce qui permet d'écrire cet
//! adaptateur alors que l'org de démo est inaccessible, et de le vérifier sur des
//! réponses enregistrées plutôt que sur des suppositions.
//!
//! ## Les trois pièges de Salesforce, nommés par le design §5
//!
//! 1. **Vingt champs suivis par objet.** Au-delà, l'historique ne dit rien. Un
//!    historique vide et un champ non suivi se ressemblent, et mènent à des
//!    conclusions opposées : le premier autorise un `state_before` reconstitué,
//!    le second impose `unknown_before`.
//! 2. **Les longs textes ne stockent pas leurs valeurs.** `LeadHistory` rend bien
//!    une ligne, mais `OldValue` et `NewValue` y sont nuls. On sait donc *qu'il* a
//!    changé, jamais *ce qu'il* valait — encore `unknown_before`.
//! 3. **L'horodatage n'est pas garanti** au niveau de précision qu'on voudrait.
//!    D'où le rôle assigné à l'historique : **corroboration seulement**, jamais
//!    source primaire. La lecture directe fait foi.
//!
//! `unknown_before` sera donc fréquent. C'est prévu, ce n'est pas un défaut, et
//! le juge sait retirer ces champs de son verdict avec trace.
//!
//! ## Pas encore branché
//!
//! Tout ici est écrit et vérifié sur des réponses enregistrées. Ce qui manque est
//! une **org accessible** : l'échange OAuth demande une application connectée,
//! qui se crée dans l'org de démo, dont les identifiants ont été perdus
//! (incident du 2026-08-27). L'annotation porte le numéro de la tâche qui devra
//! la retirer.
#![allow(dead_code)] // retiré quand la tâche 0 rend l'org accessible

use crate::federation::{EtatPlat, Issue, RefApi, Resolution};

/// La version d'API visée.
///
/// Épinglée, pas « la dernière » : une org peut être en retard d'une version, et
/// un adaptateur qui suivrait la pointe casserait sans prévenir le jour d'une
/// mise à jour côté Salesforce.
pub const VERSION_API: &str = "v62.0";

/// Ce que l'adaptateur sait demander au réseau.
///
/// Un trait, et non `fetch` en dur : c'est ce qui rend l'adaptateur vérifiable
/// sur des réponses enregistrées. Le client robuste de R5 — backoff, budget,
/// classification — s'intercale entre les deux.
///
/// **Un seul trait pour les deux adaptateurs.** En avoir deux identiques
/// obligerait le transport à les implémenter tous les deux, et le jour où l'un
/// gagnerait une garantie que l'autre n'a pas, rien ne le dirait.
pub use crate::transport::Transport;

/// Échappe une valeur destinée à une clause SOQL.
///
/// **Une injection SOQL n'est pas une curiosité théorique ici.** Les valeurs qui
/// entrent dans ces requêtes viennent d'un nom accessible lu à l'écran : une
/// fiche client dont le nom contient une apostrophe suffit à casser la requête,
/// et un nom fabriqué exprès suffit à en changer le sens.
///
/// Salesforce échappe l'apostrophe et l'antislash par un antislash.
pub fn echapper_soql(valeur: &str) -> String {
    let mut sortie = String::with_capacity(valeur.len() + 2);
    for c in valeur.chars() {
        match c {
            '\\' => sortie.push_str("\\\\"),
            '\'' => sortie.push_str("\\'"),
            // Les caractères de contrôle n'ont rien à faire dans une requête et
            // certains la coupent en silence côté serveur.
            c if (c as u32) < 0x20 => sortie.push(' '),
            c => sortie.push(c),
        }
    }
    sortie
}

/// Encode un composant d'URL. Réutilise l'encodeur d'OAuth : une seule règle.
fn encoder(s: &str) -> String {
    crate::oauth::encoder_composant(s)
}

/// R3.1 — la lecture d'un enregistrement, restreinte aux champs demandés.
///
/// On **nomme** les champs plutôt que de tout prendre. Une lecture complète
/// ferait entrer dans l'épisode des dizaines de champs que personne n'a demandés,
/// et R3.1 restreint explicitement au périmètre de la tâche.
pub fn chemin_lecture(objet: &str, id: &str, champs: &[String]) -> String {
    format!(
        "/services/data/{VERSION_API}/sobjects/{}/{}?fields={}",
        encoder(objet),
        encoder(id),
        encoder(&champs.join(",")),
    )
}

/// R2.1 — la requête de résolution par courriel.
pub fn soql_par_courriel(objet: &str, courriel: &str) -> String {
    format!(
        "SELECT Id FROM {objet} WHERE Email = '{}' LIMIT 5",
        echapper_soql(courriel)
    )
}

/// R2.1 — la requête de résolution par domaine + nom exact.
pub fn soql_par_domaine_nom(objet: &str, domaine: &str, nom: &str) -> String {
    format!(
        "SELECT Id FROM {objet} WHERE Website LIKE '%{}' AND Name = '{}' LIMIT 5",
        echapper_soql(domaine),
        echapper_soql(nom)
    )
}

/// R4.1 — le delta : ce qui a bougé depuis un instant.
///
/// `LastModifiedById` sert à R4.2 : un changement fait par quelqu'un d'autre est
/// hors périmètre, pas un trou de capture.
pub fn soql_delta(objet: &str, depuis_iso: &str) -> String {
    format!(
        "SELECT Id, LastModifiedDate, LastModifiedById FROM {objet} \
         WHERE LastModifiedDate > {depuis_iso} ORDER BY LastModifiedDate LIMIT 200"
    )
}

/// R3.3 — l'historique d'un champ, pour corroborer.
///
/// L'objet d'historique se nomme `<Objet>History` pour les objets standard —
/// `LeadHistory`, `AccountHistory` — et `<Objet>__History` pour les objets
/// personnalisés, dont le nom finit déjà par `__c`.
pub fn soql_historique(objet: &str, champ_lien: &str, id: &str, depuis_iso: &str) -> String {
    format!(
        "SELECT Field, OldValue, NewValue, CreatedDate FROM {} \
         WHERE {champ_lien} = '{}' AND CreatedDate >= {depuis_iso} \
         ORDER BY CreatedDate LIMIT 200",
        objet_historique(objet),
        echapper_soql(id)
    )
}

/// Le nom de l'objet d'historique associé.
pub fn objet_historique(objet: &str) -> String {
    if let Some(base) = objet.strip_suffix("__c") {
        format!("{base}__History")
    } else {
        format!("{objet}History")
    }
}

/// Le chemin d'une requête SOQL.
pub fn chemin_query(soql: &str) -> String {
    format!("/services/data/{VERSION_API}/query?q={}", encoder(soql))
}

/// Aplatit un enregistrement Salesforce en `FlatState`.
///
/// Trois règles, et chacune vient d'une façon dont Salesforce diffère du modèle
/// plat que le juge attend :
///
/// - **`attributes` disparaît.** Salesforce y met le type et l'URL de
///   l'enregistrement ; ce sont des métadonnées de transport, pas un état, et les
///   garder ferait diverger deux lectures du même état selon l'URL servie.
/// - **Les objets imbriqués disparaissent aussi**, avec une exception : une
///   relation résolue (`Owner: {Name: ...}`) est aplatie en `Owner.Name`. Le juge
///   compare des scalaires ; un objet imbriqué n'a pas de diff trivial.
/// - **`null` reste `null`.** Ne pas le confondre avec l'absence : la spec 001
///   les traite comme équivalents au moment du diff, mais c'est SA décision, pas
///   celle de l'adaptateur.
pub fn aplatir(record: &serde_json::Value, champs: &[String]) -> EtatPlat {
    let mut plat = EtatPlat::new();
    let Some(objet) = record.as_object() else {
        return plat;
    };
    let voulus: std::collections::BTreeSet<&str> = champs.iter().map(String::as_str).collect();

    for (cle, valeur) in objet {
        if cle == "attributes" {
            continue;
        }
        match valeur {
            serde_json::Value::Object(imbrique) => {
                for (sous_cle, sous_valeur) in imbrique {
                    if sous_cle == "attributes" || sous_valeur.is_object() {
                        continue;
                    }
                    let compose = format!("{cle}.{sous_cle}");
                    if voulus.is_empty() || voulus.contains(compose.as_str()) {
                        plat.insert(compose, sous_valeur.clone());
                    }
                }
            }
            // Un tableau n'a pas de diff trivial non plus, et Salesforce n'en
            // rend que pour les sous-requêtes, qu'on ne demande pas.
            serde_json::Value::Array(_) => {}
            scalaire => {
                if voulus.is_empty() || voulus.contains(cle.as_str()) {
                    plat.insert(cle.clone(), scalaire.clone());
                }
            }
        }
    }
    plat
}

/// Un point d'historique, tel que Salesforce le rend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointHistorique {
    pub champ: String,
    pub quand: String,
    pub avant: Option<serde_json::Value>,
    pub apres: Option<serde_json::Value>,
    /// **Le champ a changé, mais on ne sait pas vers quoi.**
    ///
    /// C'est le cas des longs textes : Salesforce écrit la ligne d'historique et
    /// laisse les valeurs nulles. Savoir qu'il a changé sans savoir ce qu'il
    /// valait est précisément ce que `unknown_before` doit porter.
    pub valeurs_absentes: bool,
}

/// Analyse une réponse d'historique.
pub fn lire_historique(corps: &serde_json::Value) -> Vec<PointHistorique> {
    let Some(lignes) = corps.get("records").and_then(|r| r.as_array()) else {
        return Vec::new();
    };
    lignes
        .iter()
        .filter_map(|l| {
            let champ = l.get("Field")?.as_str()?.to_string();
            let quand = l.get("CreatedDate")?.as_str()?.to_string();
            let avant = l.get("OldValue").cloned().filter(|v| !v.is_null());
            let apres = l.get("NewValue").cloned().filter(|v| !v.is_null());
            Some(PointHistorique {
                valeurs_absentes: avant.is_none() && apres.is_none(),
                champ,
                quand,
                avant,
                apres,
            })
        })
        .collect()
}

/// Analyse une réponse de résolution : combien de candidats, et lesquels.
pub fn lire_resolution(
    corps: &serde_json::Value,
    objet: &str,
    par: &str,
    quand: &str,
) -> Resolution {
    let vide = Vec::new();
    let lignes = corps
        .get("records")
        .and_then(|r| r.as_array())
        .unwrap_or(&vide);

    match lignes.len() {
        0 => Resolution::Introuvable,
        1 => match lignes[0].get("Id").and_then(|i| i.as_str()) {
            Some(id) => Resolution::Resolue {
                reference: RefApi {
                    connector: "salesforce".into(),
                    object: objet.to_string(),
                    id: id.to_string(),
                },
                par: par.to_string(),
                quand: quand.to_string(),
            },
            // Un enregistrement sans `Id` n'est pas un enregistrement. On ne
            // devine pas : c'est un « introuvable » honnête.
            None => Resolution::Introuvable,
        },
        n => Resolution::Ambigue(n),
    }
}

/// R5.2 — ce que le statut et le corps d'erreur veulent dire.
///
/// Salesforce rend un tableau de `{message, errorCode}`. Le **code** compte, pas
/// le message : les messages sont traduits dans la langue de l'org, et une
/// classification qui les lirait se tromperait sur une org en français.
pub fn classer_erreur(statut: u16, corps: &str) -> Issue {
    let code = serde_json::from_str::<serde_json::Value>(corps)
        .ok()
        .and_then(|v| {
            v.as_array()
                .and_then(|a| a.first().cloned())
                .or(Some(v))?
                .get("errorCode")?
                .as_str()
                .map(str::to_string)
        })
        .unwrap_or_default();

    match statut {
        401 => Issue::Trou(format!("session invalide ({code})")),
        403 => Issue::HorsPerimetre(format!("droits insuffisants ({code})")),
        404 => Issue::HorsPerimetre(format!("enregistrement introuvable ({code})")),
        // R5.1 : 429 et 5xx méritent une reprise. Ici on ne retente pas — c'est
        // le rôle du client commun — mais on dit que c'était retentable.
        429 => Issue::Trou(format!("quota atteint ({code})")),
        s if s >= 500 => Issue::Trou(format!("erreur serveur {s} ({code})")),
        s => Issue::Trou(format!("reponse inattendue {s} ({code})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(t: &str) -> serde_json::Value {
        serde_json::from_str(t).expect("json de banc")
    }

    // -- L'echappement SOQL -------------------------------------------------

    #[test]
    fn une_apostrophe_dans_un_nom_ne_casse_pas_la_requete() {
        // Ce n'est pas theorique : les valeurs viennent d'un nom accessible lu a
        // l'ecran, et « O'Brien » est un nom.
        let s = soql_par_courriel("Lead", "o'brien@exemple.fr");
        assert!(s.contains("o\\'brien@exemple.fr"), "{s}");
        // Une seule apostrophe non echappee de chaque cote de la valeur.
        assert_eq!(s.matches('\'').count() - s.matches("\\'").count(), 2, "{s}");
    }

    #[test]
    fn un_nom_fabrique_ne_change_pas_le_sens_de_la_requete() {
        // La tentative classique : fermer la chaine et ajouter une clause.
        let mechant = "x' OR Id != '";
        let s = soql_par_courriel("Lead", mechant);
        assert!(!s.contains("' OR Id != ''"), "injection passee :\n{s}");
        assert!(s.contains("x\\' OR Id != \\'"), "{s}");
    }

    #[test]
    fn l_antislash_est_echappe_aussi() {
        // Sinon `\'` en entree devient `\\'` en sortie, ce qui reouvre la porte.
        assert_eq!(echapper_soql("a\\b"), "a\\\\b");
        assert_eq!(echapper_soql("a\\'b"), "a\\\\\\'b");
    }

    #[test]
    fn les_caracteres_de_controle_deviennent_des_espaces() {
        // Certains coupent la requete en silence cote serveur, et le diagnostic
        // est alors une erreur de syntaxe sur une requete qui a l'air correcte.
        assert_eq!(echapper_soql("a\nb\tc"), "a b c");
    }

    // -- Les chemins --------------------------------------------------------

    #[test]
    fn la_lecture_nomme_ses_champs() {
        // R3.1 : restreinte au perimetre de la tache. Une lecture complete ferait
        // entrer des dizaines de champs que personne n'a demandes.
        let c = chemin_lecture("Lead", "00Q1", &["Status".into(), "Rating".into()]);
        assert!(c.contains("/sobjects/Lead/00Q1"), "{c}");
        assert!(c.contains("fields=Status%2CRating"), "{c}");
    }

    #[test]
    fn la_version_d_api_est_epinglee() {
        // Pas « la derniere » : une org peut etre en retard d'une version, et un
        // adaptateur qui suivrait la pointe casserait sans prevenir.
        assert!(VERSION_API.starts_with('v'), "{VERSION_API}");
        assert!(chemin_lecture("Lead", "1", &[]).contains(VERSION_API));
    }

    #[test]
    fn l_objet_d_historique_suit_la_convention_de_salesforce() {
        assert_eq!(objet_historique("Lead"), "LeadHistory");
        assert_eq!(objet_historique("Account"), "AccountHistory");
        // Les objets personnalises finissent deja par `__c`.
        assert_eq!(objet_historique("Dossier__c"), "Dossier__History");
    }

    #[test]
    fn le_delta_demande_l_acteur() {
        // R4.2 : un changement fait par quelqu'un d'autre est hors perimetre, pas
        // un trou de capture. Sans `LastModifiedById`, on ne peut pas trancher.
        let s = soql_delta("Lead", "2026-01-14T09:00:00Z");
        assert!(s.contains("LastModifiedById"), "{s}");
        assert!(s.contains("ORDER BY LastModifiedDate"), "{s}");
    }

    // -- L'aplatissement ----------------------------------------------------

    #[test]
    fn attributes_ne_rentre_jamais_dans_l_etat() {
        // Ce sont des metadonnees de transport. Les garder ferait diverger deux
        // lectures du meme etat selon l'URL servie.
        let r = json(r#"{"attributes":{"type":"Lead","url":"/x"},"Status":"Open"}"#);
        let plat = aplatir(&r, &["Status".into()]);
        assert_eq!(plat.keys().collect::<Vec<_>>(), vec!["Status"]);
    }

    #[test]
    fn une_relation_resolue_est_aplatie_avec_un_point() {
        // Le juge compare des scalaires ; un objet imbrique n'a pas de diff
        // trivial, et c'est sa trivialite qui rend le juge verifiable.
        let r = json(
            r#"{"Status":"Open","Owner":{"attributes":{"type":"User"},"Name":"Alice","Id":"005"}}"#,
        );
        let plat = aplatir(&r, &["Status".into(), "Owner.Name".into()]);
        assert_eq!(plat.get("Owner.Name"), Some(&json(r#""Alice""#)));
        assert!(!plat.contains_key("Owner.Id"), "non demande");
    }

    #[test]
    fn un_champ_non_demande_n_entre_pas() {
        let r = json(r#"{"Status":"Open","Description":"secret"}"#);
        let plat = aplatir(&r, &["Status".into()]);
        assert!(!plat.contains_key("Description"));
    }

    #[test]
    fn null_reste_null() {
        // Ne pas le confondre avec l'absence : la spec 001 les traite comme
        // equivalents au moment du diff, mais c'est SA decision, pas celle de
        // l'adaptateur.
        let r = json(r#"{"Rating":null}"#);
        let plat = aplatir(&r, &["Rating".into()]);
        assert_eq!(plat.get("Rating"), Some(&serde_json::Value::Null));
    }

    #[test]
    fn les_tableaux_ne_rentrent_pas() {
        let r = json(r#"{"Status":"Open","Tasks":[{"Id":"1"}]}"#);
        let plat = aplatir(&r, &[]);
        assert!(!plat.contains_key("Tasks"));
    }

    // -- La resolution ------------------------------------------------------

    #[test]
    fn un_seul_enregistrement_resout() {
        let r = lire_resolution(
            &json(r#"{"totalSize":1,"records":[{"Id":"00Q123"}]}"#),
            "Lead",
            "email_token",
            "2026-01-14T09:12:00.000Z",
        );
        match r {
            Resolution::Resolue { reference, par, .. } => {
                assert_eq!(reference.id, "00Q123");
                assert_eq!(reference.connector, "salesforce");
                assert_eq!(par, "email_token");
            }
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn zero_enregistrement_rend_introuvable() {
        let r = lire_resolution(&json(r#"{"totalSize":0,"records":[]}"#), "Lead", "x", "t");
        assert_eq!(r, Resolution::Introuvable);
    }

    #[test]
    fn deux_enregistrements_rendent_ambigu_avec_leur_nombre() {
        let r = lire_resolution(
            &json(r#"{"records":[{"Id":"a"},{"Id":"b"},{"Id":"c"}]}"#),
            "Lead",
            "x",
            "t",
        );
        assert_eq!(r, Resolution::Ambigue(3));
    }

    #[test]
    fn un_enregistrement_sans_id_n_est_pas_un_enregistrement() {
        // On ne devine pas : c'est un « introuvable » honnete.
        let r = lire_resolution(&json(r#"{"records":[{"Name":"x"}]}"#), "Lead", "x", "t");
        assert_eq!(r, Resolution::Introuvable);
    }

    // -- L'historique et ses pieges (design §5) ----------------------------

    #[test]
    fn un_point_d_historique_ordinaire_porte_ses_deux_valeurs() {
        let h = lire_historique(&json(
            r#"{"records":[{"Field":"Status","OldValue":"Open","NewValue":"Qualified",
                "CreatedDate":"2026-01-14T09:00:00.000+0000"}]}"#,
        ));
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].champ, "Status");
        assert!(!h[0].valeurs_absentes);
    }

    #[test]
    fn un_long_texte_dit_qu_il_a_change_sans_dire_vers_quoi() {
        // Le deuxieme piege du design §5. Salesforce ecrit la ligne et laisse les
        // valeurs nulles : on sait QU'il a change, jamais CE QU'il valait.
        let h = lire_historique(&json(
            r#"{"records":[{"Field":"Description","OldValue":null,"NewValue":null,
                "CreatedDate":"2026-01-14T09:00:00.000+0000"}]}"#,
        ));
        assert_eq!(h.len(), 1);
        assert!(
            h[0].valeurs_absentes,
            "c est exactement le cas unknown_before"
        );
    }

    #[test]
    fn un_historique_vide_rend_une_liste_vide_pas_une_erreur() {
        // Le premier piege : au-dela de vingt champs suivis, l'historique ne dit
        // rien — et « rien » ressemble a « aucun changement ».
        assert_eq!(lire_historique(&json(r#"{"records":[]}"#)), Vec::new());
        assert_eq!(lire_historique(&json(r#"{}"#)), Vec::new());
    }

    #[test]
    fn une_ligne_d_historique_incomplete_est_ecartee_pas_devinee() {
        let h = lire_historique(&json(
            r#"{"records":[{"OldValue":"a","NewValue":"b"},{"Field":"Status","CreatedDate":"2026-01-14T09:00:00.000+0000"}]}"#,
        ));
        assert_eq!(h.len(), 1, "seule la ligne datee et nommee compte");
    }

    // -- La classification des erreurs (R5.2) ------------------------------

    #[test]
    fn le_code_compte_pas_le_message() {
        // Les messages de Salesforce sont traduits dans la langue de l'org. Une
        // classification qui les lirait se tromperait sur une org en francais.
        let corps = r#"[{"message":"Session expiree","errorCode":"INVALID_SESSION_ID"}]"#;
        match classer_erreur(401, corps) {
            Issue::Trou(c) => assert!(c.contains("INVALID_SESSION_ID"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn chaque_statut_a_sa_consequence() {
        // R5.2 : `permission` -> hors_perimetre, le reste -> trou avec cause.
        // Les confondre accuserait la capture d'avoir rate ce qu'elle n'avait pas
        // le droit de voir.
        assert!(matches!(classer_erreur(403, "[]"), Issue::HorsPerimetre(_)));
        assert!(matches!(classer_erreur(404, "[]"), Issue::HorsPerimetre(_)));
        assert!(matches!(classer_erreur(429, "[]"), Issue::Trou(_)));
        assert!(matches!(classer_erreur(503, "[]"), Issue::Trou(_)));
        assert!(matches!(classer_erreur(418, "[]"), Issue::Trou(_)));
    }

    #[test]
    fn un_corps_illisible_ne_fait_pas_perdre_le_statut() {
        // Une passerelle en panne rend du HTML, pas du JSON. Le statut reste
        // l'information, et la classification ne doit pas s'effondrer avec le
        // corps.
        match classer_erreur(503, "<html>Service Unavailable</html>") {
            Issue::Trou(c) => assert!(c.contains("503"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn aucune_classification_ne_rend_une_lecture() {
        // Une erreur ne doit JAMAIS ressembler a un etat lu : un etat vide se
        // lirait comme « tous les champs sont nuls ».
        for statut in [400u16, 401, 403, 404, 429, 500, 503] {
            assert!(
                !matches!(classer_erreur(statut, "[]"), Issue::Lu(_)),
                "statut {statut}"
            );
        }
    }
}
