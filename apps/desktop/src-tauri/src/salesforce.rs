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

/// Le code d'erreur Salesforce, où qu'il soit dans la réponse.
fn code_erreur(corps: &str) -> String {
    serde_json::from_str::<serde_json::Value>(corps)
        .ok()
        .and_then(|v| {
            v.as_array()
                .and_then(|a| a.first().cloned())
                .or(Some(v))?
                .get("errorCode")?
                .as_str()
                .map(str::to_string)
        })
        .unwrap_or_default()
}

/// R5.1 — ce qu'une réponse veut dire pour la reprise.
///
/// **Un `403` n'est pas toujours un refus de droits.** Salesforce renvoie
/// `REQUEST_LIMIT_EXCEEDED` avec ce statut quand l'org a brûlé son quota
/// quotidien d'appels — et là, réessayer plus tard est exactement ce qu'il faut
/// faire. Le confondre avec un refus de droits transformerait une limite
/// passagère en entité définitivement non résolue.
pub fn classe_reprise(statut: u16, corps: &str) -> crate::client::Classe {
    use crate::client::Classe;
    match (statut, code_erreur(corps).as_str()) {
        (401, _) => Classe::NonAutorise,
        (403, "REQUEST_LIMIT_EXCEEDED") | (408 | 429, _) => Classe::Reessayable,
        (s, _) if s >= 500 => Classe::Reessayable,
        _ => Classe::Finale,
    }
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
    let code = code_erreur(corps);

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

/// Le nom du connecteur, tel qu'il apparaît dans les `api_refs`.
pub const CONNECTEUR: &str = "salesforce";

/// L'ordre dans lequel les clés tranchent.
///
/// Miroir exact de `PRIORITE` côté TypeScript. L'identifiant système d'abord :
/// c'est le système lui-même qui l'a émis, il ne peut pas désigner deux
/// enregistrements. Le courriel ensuite. Le couple domaine + nom en dernier,
/// parce que deux personnes peuvent porter le même nom dans la même entreprise.
const PRIORITE: &[&str] = &["system_id", "email_token", "domain_name"];

/// Le séparateur d'une clé `domain_name`.
///
/// Le contrat TypeScript porte deux champs (`domain` et `name`) ; le canal Rust
/// ne transporte qu'un couple `(genre, valeur)`. Il faut donc un séparateur, et
/// c'est la tabulation : elle ne peut pas apparaître dans un domaine, et
/// `normaliser_blancs` la remplace par une espace dans un nom — donc une
/// tabulation restante ne peut être que celle-ci.
///
/// Une valeur sans séparateur est **refusée** et pas devinée : découper sur
/// autre chose reviendrait à choisir à la place de l'appelant, et R2.2 interdit
/// exactement ça.
pub const SEPARATEUR_DOMAINE_NOM: char = '\t';

/// L'adaptateur, une fois le transport branché.
pub struct Adaptateur<T: Transport> {
    transport: T,
    /// Les objets interrogés, dans l'ordre.
    ///
    /// Un courriel peut désigner un `Contact` **ou** un `Lead`, et les deux
    /// comptent : trouver un enregistrement dans chacun n'est pas une résolution,
    /// c'est une ambiguïté à deux candidats.
    objets: Vec<String>,
}

impl<T: Transport> Adaptateur<T> {
    pub fn nouveau(transport: T, objets: Vec<String>) -> Self {
        Self { transport, objets }
    }

    /// Exécute une requête de résolution et rend les identifiants trouvés.
    fn identifiants(&self, soql: &str) -> Result<Vec<String>, String> {
        let crate::transport::ReponseHttp { statut, corps, .. } = self
            .transport
            .get(&chemin_query(soql))
            .map_err(|e| format!("transport : {e}"))?;
        if statut != 200 {
            return Err(match classer_erreur(statut, &corps) {
                Issue::Trou(c) | Issue::HorsPerimetre(c) => c,
                Issue::Lu(_) => "reponse inattendue".into(),
            });
        }
        let v: serde_json::Value =
            serde_json::from_str(&corps).map_err(|e| format!("reponse illisible : {e}"))?;

        // `done: false` veut dire que la réponse est **partielle**. Une page
        // incomplète transformerait une ambiguïté en résolution : deux candidats
        // dont un seul est arrivé se lisent comme « exactement un ».
        if v.get("done") == Some(&serde_json::Value::Bool(false)) {
            return Err("page de resultats incomplete".into());
        }

        Ok(v.get("records")
            .and_then(serde_json::Value::as_array)
            .map(|r| {
                r.iter()
                    .filter_map(|e| e.get("Id").and_then(serde_json::Value::as_str))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default())
    }

    /// La requête d'une clé, pour un objet donné.
    fn requete(&self, genre: &str, valeur: &str, objet: &str) -> Result<String, String> {
        match genre {
            "system_id" => Ok(format!(
                "SELECT Id FROM {objet} WHERE Id = '{}' LIMIT 5",
                echapper_soql(valeur)
            )),
            "email_token" => Ok(soql_par_courriel(objet, valeur)),
            "domain_name" => match valeur.split_once(SEPARATEUR_DOMAINE_NOM) {
                Some((domaine, nom)) if !domaine.is_empty() && !nom.is_empty() => {
                    Ok(soql_par_domaine_nom(objet, domaine, nom))
                }
                _ => Err("cle domain_name malformee".into()),
            },
            autre => Err(format!("genre de cle inconnu : {autre}")),
        }
    }
}

impl<T: Transport> crate::federation::Federation for Adaptateur<T> {
    /// R2.1 et R2.2 — la résolution.
    ///
    /// Les clés sont essayées **dans l'ordre de force**, et la première qui donne
    /// exactement un candidat tranche. Une clé qui en donne plusieurs arrête
    /// tout : **une ambiguïté n'est jamais départagée par une clé plus faible**.
    /// Affiner avec `domain_name` ce que le courriel n'a pas tranché, c'est
    /// exactement deviner.
    ///
    /// Un appel qui échoue arrête tout aussi, en `Empechee`. Répondre
    /// `Introuvable` parce qu'on n'a pas pu regarder affirmerait que
    /// l'enregistrement n'existe pas — une conclusion qu'on n'a pas.
    fn resoudre(&self, cles: &[(String, String)]) -> Resolution {
        for genre in PRIORITE {
            let valeurs: Vec<&str> = cles
                .iter()
                .filter(|(g, _)| g == genre)
                .map(|(_, v)| v.as_str())
                .collect();
            if valeurs.is_empty() {
                continue;
            }

            let mut trouves: Vec<RefApi> = Vec::new();
            for objet in &self.objets {
                for valeur in &valeurs {
                    let soql = match self.requete(genre, valeur, objet) {
                        Ok(s) => s,
                        Err(cause) => return Resolution::Empechee(cause),
                    };
                    match self.identifiants(&soql) {
                        Ok(ids) => {
                            for id in ids {
                                let r = RefApi {
                                    connector: CONNECTEUR.into(),
                                    object: objet.clone(),
                                    id,
                                };
                                if !trouves.contains(&r) {
                                    trouves.push(r);
                                }
                            }
                        }
                        // Un objet qu'on n'a pas pu interroger laisse la question
                        // ouverte : « exactement un » n'est plus démontrable.
                        Err(cause) => return Resolution::Empechee(cause),
                    }
                }
            }

            match trouves.len() {
                0 => continue,
                1 => {
                    return Resolution::Resolue {
                        reference: trouves.remove(0),
                        par: (*genre).to_owned(),
                        quand: crate::federation::maintenant_iso(),
                    }
                }
                n => return Resolution::Ambigue(n),
            }
        }
        Resolution::Introuvable
    }

    /// R3.1 — la lecture d'un enregistrement, restreinte aux champs demandés.
    fn lire(&self, reference: &RefApi, champs: &[String]) -> Issue {
        if reference.connector != CONNECTEUR {
            return Issue::HorsPerimetre(format!("connecteur {}", reference.connector));
        }
        if champs.is_empty() {
            // Sans périmètre, une lecture complète ferait entrer dans l'épisode
            // des dizaines de champs que personne n'a demandés.
            return Issue::HorsPerimetre("aucun champ dans le perimetre".into());
        }
        let chemin = chemin_lecture(&reference.object, &reference.id, champs);
        match self.transport.get(&chemin) {
            Err(e) => Issue::Trou(format!("transport : {e}")),
            Ok(r) if r.statut == 200 => match serde_json::from_str::<serde_json::Value>(&r.corps) {
                Ok(v) => Issue::Lu(aplatir(&v, champs)),
                Err(e) => Issue::Trou(format!("reponse illisible : {e}")),
            },
            Ok(r) => classer_erreur(r.statut, &r.corps),
        }
    }
}

#[cfg(test)]
mod tests_adaptateur {
    use super::*;
    use crate::federation::Federation;

    /// Un transport enregistré : le test décide de la réponse à partir du chemin.
    struct Faux<F: Fn(&str) -> (u16, String) + Send + Sync>(F);
    impl<F: Fn(&str) -> (u16, String) + Send + Sync> Transport for Faux<F> {
        fn get(&self, chemin: &str) -> Result<crate::transport::ReponseHttp, String> {
            let (statut, corps) = (self.0)(chemin);
            Ok(crate::transport::ReponseHttp::simple(statut, corps))
        }
    }

    fn enregistrements(ids: &[&str]) -> (u16, String) {
        let records: Vec<serde_json::Value> = ids
            .iter()
            .map(|id| serde_json::json!({"attributes": {"type": "Contact"}, "Id": id}))
            .collect();
        (
            200,
            serde_json::json!({"totalSize": ids.len(), "done": true, "records": records})
                .to_string(),
        )
    }

    fn adaptateur<F: Fn(&str) -> (u16, String) + Send + Sync>(f: F) -> Adaptateur<Faux<F>> {
        Adaptateur::nouveau(Faux(f), vec!["Contact".into(), "Lead".into()])
    }

    fn cle(genre: &str, valeur: &str) -> Vec<(String, String)> {
        vec![(genre.to_owned(), valeur.to_owned())]
    }

    #[test]
    fn un_courriel_qui_donne_un_seul_enregistrement_resout() {
        let a = adaptateur(|c| {
            if c.contains("Contact") {
                enregistrements(&["003AAA"])
            } else {
                enregistrements(&[])
            }
        });
        match a.resoudre(&cle("email_token", "jean@ex.com")) {
            Resolution::Resolue { reference, par, .. } => {
                assert_eq!(par, "email_token");
                assert_eq!(reference.object, "Contact");
                assert_eq!(reference.id, "003AAA");
                assert_eq!(reference.connector, "salesforce");
            }
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn une_ambiguite_n_est_jamais_departagee_par_une_cle_plus_faible() {
        // Le courriel donne DEUX candidats ; le couple domaine + nom n'en
        // donnerait qu'un. Affiner avec la clé plus faible, c'est deviner.
        let a = adaptateur(|c| {
            if c.contains("Email") && c.contains("Contact") {
                enregistrements(&["003AAA", "003BBB"])
            } else if c.contains("Website") {
                enregistrements(&["003AAA"])
            } else {
                enregistrements(&[])
            }
        });
        let cles = vec![
            ("email_token".into(), "jean@ex.com".into()),
            (
                "domain_name".into(),
                format!("ex.com{SEPARATEUR_DOMAINE_NOM}Jean Dupont"),
            ),
        ];
        assert_eq!(a.resoudre(&cles), Resolution::Ambigue(2));
    }

    #[test]
    fn l_identifiant_systeme_passe_avant_le_courriel() {
        // C'est le système lui-même qui l'a émis : il ne peut pas désigner deux
        // enregistrements.
        let a = adaptateur(|c| {
            if c.contains("Id%20%3D") && c.contains("Contact") {
                enregistrements(&["003ZZZ"])
            } else if c.contains("Email") {
                enregistrements(&["003AAA", "003BBB"])
            } else {
                enregistrements(&[])
            }
        });
        let cles = vec![
            ("email_token".into(), "jean@ex.com".into()),
            ("system_id".into(), "003ZZZ".into()),
        ];
        match a.resoudre(&cles) {
            Resolution::Resolue { par, reference, .. } => {
                assert_eq!(par, "system_id");
                assert_eq!(reference.id, "003ZZZ");
            }
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn le_meme_courriel_dans_deux_objets_est_une_ambiguite() {
        // Un Contact ET un Lead : deux dossiers, et rien ne dit lequel.
        let a = adaptateur(|_| enregistrements(&["003AAA"]));
        assert_eq!(
            a.resoudre(&cle("email_token", "jean@ex.com")),
            Resolution::Ambigue(2)
        );
    }

    #[test]
    fn zero_candidat_est_introuvable() {
        let a = adaptateur(|_| enregistrements(&[]));
        assert_eq!(
            a.resoudre(&cle("email_token", "jean@ex.com")),
            Resolution::Introuvable
        );
    }

    #[test]
    fn une_lecture_empechee_ne_devient_pas_un_introuvable() {
        // `not_found` affirme que l'enregistrement n'existe pas. Un 403 ne dit
        // rien de tel : il dit qu'on n'a pas pu regarder.
        let a = adaptateur(|_| {
            (
                403,
                serde_json::json!([{"errorCode": "INSUFFICIENT_ACCESS"}]).to_string(),
            )
        });
        match a.resoudre(&cle("email_token", "jean@ex.com")) {
            Resolution::Empechee(cause) => assert!(!cause.is_empty(), "cause muette"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn une_page_incomplete_empeche_au_lieu_de_conclure() {
        // Deux candidats dont un seul est arrivé se liraient comme « exactement
        // un » — une ambiguïté transformée en résolution.
        let a = adaptateur(|_| {
            (
                200,
                serde_json::json!({
                    "done": false,
                    "nextRecordsUrl": "/services/data/v62.0/query/01g",
                    "records": [{"Id": "003AAA"}]
                })
                .to_string(),
            )
        });
        match a.resoudre(&cle("email_token", "jean@ex.com")) {
            Resolution::Empechee(c) => assert!(c.contains("incomplete"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn une_cle_domaine_nom_malformee_est_refusee_et_pas_devinee() {
        let a = adaptateur(|_| enregistrements(&[]));
        match a.resoudre(&cle("domain_name", "ex.com sans separateur")) {
            Resolution::Empechee(c) => assert!(c.contains("malformee"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn une_cle_de_genre_inconnu_ne_resout_rien_en_silence() {
        // Un genre inconnu ne fait pas partie de la priorité : il est ignoré, et
        // l'absence de clé utilisable rend `Introuvable`, pas une devinette.
        let a = adaptateur(|_| enregistrements(&["003AAA"]));
        assert_eq!(
            a.resoudre(&cle("nom_approchant", "Jean Dupont")),
            Resolution::Introuvable
        );
    }

    #[test]
    fn une_resolution_empechee_se_raconte_autrement_qu_un_introuvable() {
        assert_eq!(
            Resolution::Empechee("droits".into()).raison(),
            "blocked:droits"
        );
        assert_eq!(Resolution::Introuvable.raison(), "not_found");
    }

    // -- La lecture ---------------------------------------------------------

    fn reference() -> RefApi {
        RefApi {
            connector: CONNECTEUR.into(),
            object: "Contact".into(),
            id: "003AAA".into(),
        }
    }

    #[test]
    fn un_quota_d_org_est_reessayable_meme_en_403() {
        // Salesforce rend REQUEST_LIMIT_EXCEEDED avec un 403 quand l'org a brule
        // son quota quotidien. Le confondre avec un refus de droits
        // transformerait une limite passagere en entite definitivement non
        // resolue.
        use crate::client::Classe;
        let quota = serde_json::json!([{"errorCode": "REQUEST_LIMIT_EXCEEDED"}]).to_string();
        let droits = serde_json::json!([{"errorCode": "INSUFFICIENT_ACCESS"}]).to_string();
        assert_eq!(classe_reprise(403, &quota), Classe::Reessayable);
        assert_eq!(classe_reprise(403, &droits), Classe::Finale);
        assert_eq!(classe_reprise(401, "{}"), Classe::NonAutorise);
        assert_eq!(classe_reprise(503, "{}"), Classe::Reessayable);
        assert_eq!(classe_reprise(404, "{}"), Classe::Finale);
    }

    #[test]
    fn une_lecture_rend_un_etat_plat_restreint_au_perimetre() {
        let a = adaptateur(|_| {
            (
                200,
                serde_json::json!({
                    "attributes": {"type": "Contact", "url": "/x"},
                    "Id": "003AAA",
                    "Statut__c": "Nouveau",
                    "Description": "hors perimetre"
                })
                .to_string(),
            )
        });
        let champs = vec!["Id".to_owned(), "Statut__c".to_owned()];
        match a.lire(&reference(), &champs) {
            Issue::Lu(plat) => {
                assert_eq!(plat["Statut__c"], serde_json::json!("Nouveau"));
                assert!(!plat.contains_key("Description"), "hors perimetre");
                assert!(!plat.contains_key("attributes"), "metadonnee de transport");
            }
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn une_lecture_qui_echoue_ne_rend_pas_un_etat_vide() {
        // Un état vide se lirait comme « tous les champs sont nuls », et le diff
        // inventerait des changements qui n'ont pas eu lieu.
        let a = adaptateur(|_| (500, "<html>oops</html>".into()));
        match a.lire(&reference(), &["Id".to_owned()]) {
            Issue::Trou(c) => assert!(!c.is_empty()),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn une_reference_d_un_autre_connecteur_est_hors_perimetre() {
        let a = adaptateur(|_| enregistrements(&[]));
        let mut r = reference();
        r.connector = "gmail".into();
        match a.lire(&r, &["Id".to_owned()]) {
            Issue::HorsPerimetre(c) => assert!(c.contains("gmail"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn un_perimetre_vide_ne_declenche_pas_une_lecture_complete() {
        let a = adaptateur(|_| panic!("aucun appel ne devrait partir"));
        assert!(matches!(a.lire(&reference(), &[]), Issue::HorsPerimetre(_)));
    }

    #[test]
    fn l_horodatage_de_resolution_est_iso_8601() {
        let q = crate::federation::maintenant_iso();
        assert!(q.ends_with('Z'), "{q}");
        assert_eq!(q.len(), 24, "{q}");
        assert!(q.starts_with("20"), "{q}");
    }
}

// ---------------------------------------------------------------------------
// R1.1 — ce que cet adaptateur reconnaît de lui-même dans un texte.
//
// La forme d'une URL d'enregistrement et l'algorithme du suffixe de contrôle
// sont des faits sur Salesforce, pas sur Noe. Les laisser dans un module
// générique reviendrait à encoder le CRM hors de son adaptateur, ce que R1.1
// interdit — et le jour où le terrain change, il faudrait les retrouver.
// ---------------------------------------------------------------------------

/// L'alphabet du suffixe de contrôle d'un identifiant.
const ALPHABET_CONTROLE: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ012345";

/// Complète un identifiant de 15 caractères en 18.
///
/// **Le même enregistrement a deux écritures.** Les URL en portent une de
/// dix-huit caractères, les APIs acceptent les deux, et certaines pages en
/// affichent quinze. Sans cette conversion, le même dossier produirait deux
/// candidates, donc deux entités, donc un graphe qui compte double.
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

/// Normalise un identifiant vers sa forme de dix-huit caractères.
///
/// Rend `None` pour tout ce qui n'est pas un identifiant : c'est le bon sens de
/// l'erreur. Une candidate en moins est un trou qui se voit ; une candidate de
/// trop est un faux dossier qui ne se voit pas.
pub fn normaliser_identifiant(brut: &str) -> Option<String> {
    match brut.len() {
        15 => completer_identifiant(brut),
        18 if identifiant_coherent(brut) => Some(brut.to_owned()),
        _ => None,
    }
}

/// Les jeux de clés que ce connecteur reconnaît dans un texte vu à l'écran.
///
/// Chaque jeu est une candidate. Deux sources :
///
/// - une **URL d'enregistrement Lightning** (`/lightning/r/<Objet>/<Id>/view`),
///   qui donne un identifiant système. On ne cherche pas un identifiant isolé
///   dans un libellé : dix-huit caractères alphanumériques, ça se trouve dans
///   n'importe quelle interface, et le suffixe de contrôle ne suffirait pas à
///   écarter toutes les coïncidences ;
/// - une **adresse de courriel**, qui désigne une personne — donc un dossier
///   ici, et pas un fil de messagerie.
///
/// L'ordre suit la force des clés : les identifiants d'abord.
pub fn cles_du_texte(texte: &str) -> Vec<Vec<(String, String)>> {
    let mut sorties = Vec::new();
    let mut vus: Vec<String> = Vec::new();

    for depart in crate::candidates::indices_de(texte, "/lightning/r/") {
        let reste = &texte[depart + "/lightning/r/".len()..];
        let mut morceaux = reste.split('/');
        let (Some(_objet), Some(brut)) = (morceaux.next(), morceaux.next()) else {
            continue;
        };
        if let Some(id) = normaliser_identifiant(crate::candidates::segment(brut)) {
            if !vus.contains(&id) {
                vus.push(id.clone());
                sorties.push(vec![("system_id".to_owned(), id)]);
            }
        }
    }
    for adresse in crate::candidates::courriels(texte) {
        sorties.push(vec![("email_token".to_owned(), adresse)]);
    }
    sorties
}

#[cfg(test)]
mod tests_extraction {
    use super::*;

    #[test]
    fn un_identifiant_de_quinze_se_complete_en_dix_huit() {
        // Le meme enregistrement a deux ecritures. Sans cette conversion, le
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
            normaliser_identifiant("0035g00000LmT4E"),
            normaliser_identifiant("0035g00000LmT4EAAV")
        );
    }

    #[test]
    fn une_chaine_de_dix_huit_caracteres_n_est_pas_un_identifiant() {
        // Dix-huit caracteres alphanumeriques, ca se trouve partout dans une
        // interface. Le suffixe de controle est ce qui les distingue.
        assert!(!identifiant_coherent("ABCDEFGHIJKLMNOPQR"));
        assert_eq!(normaliser_identifiant("ABCDEFGHIJKLMNOPQR"), None);
        assert_eq!(normaliser_identifiant("trop-court"), None);
        assert_eq!(normaliser_identifiant(""), None);
    }

    #[test]
    fn la_casse_d_un_identifiant_est_significative() {
        // `normaliserIdentifiant('system_id')` ne touche pas a la casse cote
        // TypeScript : elle porte le suffixe de controle, donc de l'information.
        let a = normaliser_identifiant("0035g00000LmT4E").unwrap();
        let b = normaliser_identifiant("0035G00000LMT4E").unwrap();
        assert_ne!(a, b, "deux enregistrements differents");
    }

    #[test]
    fn une_url_lightning_donne_un_identifiant_systeme() {
        let c = cles_du_texte(
            "https://monorg.lightning.force.com/lightning/r/Contact/0035g00000LmT4EAAV/view",
        );
        assert_eq!(
            c,
            vec![vec![(
                "system_id".to_owned(),
                "0035g00000LmT4EAAV".to_owned()
            )]]
        );
    }

    #[test]
    fn une_url_en_quinze_caracteres_donne_la_meme_candidate() {
        assert_eq!(
            cles_du_texte("/lightning/r/Contact/0035g00000LmT4EAAV/view"),
            cles_du_texte("/lightning/r/Contact/0035g00000LmT4E/view")
        );
    }

    #[test]
    fn une_url_suivie_d_un_mot_donne_quand_meme_sa_candidate() {
        // Une URL lue a l'ecran n'est presque jamais seule. Sans la coupe,
        // l'identifiant avait la mauvaise longueur et l'entite disparaissait en
        // silence — le pire des echecs, parce qu'il ressemble a « rien a voir ».
        let c = cles_du_texte("Ouvrir /lightning/r/Contact/0035g00000LmT4EAAV maintenant");
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0][0].1, "0035g00000LmT4EAAV");
    }

    #[test]
    fn une_url_sans_identifiant_valable_ne_donne_rien() {
        assert!(cles_du_texte("/lightning/r/Contact/new").is_empty());
        assert!(cles_du_texte("/lightning/r/Contact/").is_empty());
        assert!(cles_du_texte("/lightning/o/Contact/list").is_empty());
    }

    #[test]
    fn une_adresse_designe_une_personne_donc_un_dossier_ici() {
        let c = cles_du_texte("De : Jean Dupont <Jean.Dupont@Exemple.FR>");
        assert_eq!(
            c,
            vec![vec![(
                "email_token".to_owned(),
                "jean.dupont@exemple.fr".to_owned()
            )]]
        );
    }

    #[test]
    fn les_identifiants_passent_avant_les_adresses() {
        // L'ordre suit la force des cles, et deux lectures du meme texte doivent
        // rendre la meme liste.
        let texte = "jean@ex.com — /lightning/r/Contact/0035g00000LmT4EAAV/view";
        let c = cles_du_texte(texte);
        assert_eq!(c.len(), 2, "{c:?}");
        assert_eq!(c[0][0].0, "system_id");
        assert_eq!(c[1][0].0, "email_token");
        assert_eq!(c, cles_du_texte(texte), "deux lectures divergent");
    }

    #[test]
    fn un_texte_ordinaire_ne_produit_rien() {
        assert!(cles_du_texte("Enregistrer").is_empty());
        assert!(cles_du_texte("Devis 2026-014 pour Dupont SARL").is_empty());
        assert!(cles_du_texte("").is_empty());
    }
}
