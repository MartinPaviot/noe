//! Le terrain : quel système de vérité, quels champs, quel budget (spec 003, R1.1).
//!
//! ## Pourquoi ce fichier existe
//!
//! R1.1 est catégorique : **le code ne doit jamais encoder le CRM hors de son
//! adaptateur.** `terrain.json` porte le choix, les scopes et les `scope_fields`
//! par tâche. Sans lui, le nom du CRM se retrouve en dur dans l'extraction des
//! candidates, dans le routeur, dans les tests — et le jour où le terrain change,
//! il faut le retrouver partout. C'est exactement ce qui était en train
//! d'arriver : `candidates.rs` nommait `"salesforce"` dans une constante.
//!
//! ## Le sens de l'erreur, cas par cas
//!
//! Un fichier de configuration a plusieurs façons de manquer, et elles n'appellent
//! pas la même réponse.
//!
//! - **Absent** : personne n'a fait la tâche 0. Ce n'est pas une panne — la
//!   capture tourne, la fédération ne tourne pas, et l'état le dit.
//! - **Illisible ou incohérent** : on **refuse**, on ne retombe pas sur des
//!   valeurs par défaut. Un défaut silencieux pointerait les lectures sur le
//!   mauvais périmètre, et les états produits auraient l'air justes.
//! - **Tâche inconnue du fichier** : aucun `scope_fields`, donc **aucune
//!   lecture**. Lire « tout » ferait entrer dans l'épisode des dizaines de champs
//!   que personne n'a demandés, ce que R3.1 interdit.
//!
//! ## Jamais de secret ici
//!
//! Ce fichier décrit un terrain, il ne l'ouvre pas. La règle 5 est appliquée par
//! une **mécanique** et pas par une intention : un fichier qui porterait une clé
//! dont le nom évoque un secret est refusé, avec sa raison. Les jetons vivent
//! sous DPAPI, et nulle part ailleurs.

#![allow(dead_code)] // retiré quand la tâche 0 écrit ce fichier

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// R5.3 : le budget d'appels par épisode, quand le fichier n'en fixe pas.
pub const BUDGET_PAR_DEFAUT: u32 = 30;

/// Les fragments de nom qui font refuser le fichier.
///
/// `client_id` n'en est pas : PKCE est fait pour des clients publics, et un
/// identifiant client n'est pas un secret. `client_secret` en est un.
const NOMS_DE_SECRET: &[&str] = &[
    "secret",
    "password",
    "passwd",
    "mot_de_passe",
    "token",
    "jeton",
    "private_key",
    "cle_privee",
];

/// Ce qui empêche de lire un terrain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurTerrain {
    /// Le fichier n'existe pas : la tâche 0 n'a pas eu lieu.
    Absent,
    /// Le fichier existe mais ne se lit pas.
    Illisible(String),
    /// Le fichier se lit mais ne tient pas debout.
    Incoherent(String),
    /// Le fichier porte une clé dont le nom évoque un secret (règle 5).
    SecretSuspect(String),
}

impl std::fmt::Display for ErreurTerrain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Absent => write!(f, "terrain.json absent : la tache 0 n a pas eu lieu"),
            Self::Illisible(c) => write!(f, "terrain.json illisible : {c}"),
            Self::Incoherent(c) => write!(f, "terrain.json incoherent : {c}"),
            Self::SecretSuspect(c) => write!(f, "terrain.json porte un secret presume : {c}"),
        }
    }
}

impl std::error::Error for ErreurTerrain {}

/// Le périmètre d'une tâche.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Perimetre {
    /// Les champs que le juge compare pour cette tâche (R3.1).
    #[serde(default)]
    pub scope_fields: Vec<String>,
    /// Les objets du CRM interrogés pour résoudre, dans l'ordre.
    #[serde(default)]
    pub objects: Vec<String>,
}

/// Les budgets, tels que le fichier les fixe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budgets {
    #[serde(default = "budget_par_defaut")]
    pub reads_per_episode: u32,
}

fn budget_par_defaut() -> u32 {
    BUDGET_PAR_DEFAUT
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            reads_per_episode: BUDGET_PAR_DEFAUT,
        }
    }
}

/// Le terrain, exactement dans la forme que le design §2 fixe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Terrain {
    /// Le connecteur du système de vérité métier. **Le seul endroit qui le
    /// nomme** hors de son adaptateur.
    pub crm: String,
    /// Le connecteur de messagerie, s'il y en a un.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mail: Option<String>,
    #[serde(default)]
    pub tasks: BTreeMap<String, Perimetre>,
    #[serde(default)]
    pub budgets: Budgets,
}

impl Terrain {
    /// Charge et valide.
    pub fn charger(chemin: &Path) -> Result<Self, ErreurTerrain> {
        let brut = match std::fs::read_to_string(chemin) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ErreurTerrain::Absent)
            }
            Err(e) => return Err(ErreurTerrain::Illisible(e.to_string())),
        };
        Self::analyser(&brut)
    }

    /// La partie qui se teste sans disque.
    pub fn analyser(brut: &str) -> Result<Self, ErreurTerrain> {
        let valeur: serde_json::Value =
            serde_json::from_str(brut).map_err(|e| ErreurTerrain::Illisible(e.to_string()))?;

        // Règle 5, appliquée par une mécanique. Le contrôle porte sur le fichier
        // ENTIER et pas sur les champs connus : un secret rangé dans un champ
        // qu'on ignore serait un secret quand même.
        if let Some(nom) = nom_de_secret(&valeur) {
            return Err(ErreurTerrain::SecretSuspect(nom));
        }

        let terrain: Self =
            serde_json::from_value(valeur).map_err(|e| ErreurTerrain::Incoherent(e.to_string()))?;

        if terrain.crm.trim().is_empty() {
            return Err(ErreurTerrain::Incoherent("crm vide".into()));
        }
        if terrain.budgets.reads_per_episode == 0 {
            // Un budget de zéro n'est pas « pas de limite » : c'est « aucune
            // lecture », et l'écrire par accident rendrait toute la fédération
            // muette sans qu'une seule erreur n'apparaisse.
            return Err(ErreurTerrain::Incoherent(
                "budget de lectures a zero : aucune lecture ne partirait".into(),
            ));
        }
        for (slug, perimetre) in &terrain.tasks {
            if perimetre.scope_fields.iter().any(|c| c.trim().is_empty()) {
                return Err(ErreurTerrain::Incoherent(format!(
                    "champ vide dans le perimetre de {slug}"
                )));
            }
        }
        Ok(terrain)
    }

    /// Le périmètre d'une tâche, s'il est déclaré.
    ///
    /// **Rend `None` plutôt qu'un périmètre vide** pour une tâche inconnue :
    /// l'appelant doit pouvoir distinguer « rien à lire ici » de « lis tout »,
    /// et un `Vec` vide se confond avec le second.
    pub fn perimetre(&self, task_slug: &str) -> Option<&Perimetre> {
        self.tasks.get(task_slug)
    }

    /// Les connecteurs que ce terrain déclare, dans l'ordre.
    pub fn connecteurs(&self) -> Vec<&str> {
        let mut sortie = vec![self.crm.as_str()];
        if let Some(m) = &self.mail {
            sortie.push(m.as_str());
        }
        sortie
    }
}

/// Le premier nom de clé qui évoque un secret, où qu'il soit dans l'arbre.
fn nom_de_secret(valeur: &serde_json::Value) -> Option<String> {
    match valeur {
        serde_json::Value::Object(o) => {
            for (cle, v) in o {
                let minuscule = cle.to_lowercase();
                if NOMS_DE_SECRET.iter().any(|n| minuscule.contains(n)) {
                    return Some(cle.clone());
                }
                if let Some(trouve) = nom_de_secret(v) {
                    return Some(trouve);
                }
            }
            None
        }
        serde_json::Value::Array(a) => a.iter().find_map(nom_de_secret),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json_minimal() -> String {
        serde_json::json!({
            "crm": "salesforce",
            "mail": "gmail",
            "tasks": {
                "maj-crm-post-echange": {
                    "scope_fields": ["Status", "Rating"],
                    "objects": ["Contact", "Lead"]
                }
            },
            "budgets": { "reads_per_episode": 30 }
        })
        .to_string()
    }

    #[test]
    fn un_terrain_complet_se_lit() {
        let t = Terrain::analyser(&json_minimal()).unwrap();
        assert_eq!(t.crm, "salesforce");
        assert_eq!(t.mail.as_deref(), Some("gmail"));
        assert_eq!(t.budgets.reads_per_episode, 30);
        let p = t.perimetre("maj-crm-post-echange").unwrap();
        assert_eq!(p.scope_fields, vec!["Status", "Rating"]);
        assert_eq!(p.objects, vec!["Contact", "Lead"]);
    }

    #[test]
    fn le_nom_du_crm_ne_vient_que_d_ici() {
        // R1.1 : le code ne doit jamais encoder le CRM hors de son adaptateur.
        // Changer le fichier suffit a changer de terrain.
        let autre = serde_json::json!({"crm": "hubspot"}).to_string();
        let t = Terrain::analyser(&autre).unwrap();
        assert_eq!(t.connecteurs(), vec!["hubspot"]);
        assert_eq!(t.mail, None, "une messagerie n est pas obligatoire");
    }

    #[test]
    fn un_terrain_absent_n_est_pas_une_panne() {
        // Personne n'a fait la tache 0 : la capture tourne, la federation non.
        let manquant = std::path::Path::new("C:/introuvable/terrain.json");
        assert_eq!(Terrain::charger(manquant), Err(ErreurTerrain::Absent));
    }

    #[test]
    fn un_terrain_illisible_refuse_au_lieu_de_deviner() {
        // Un defaut silencieux pointerait les lectures sur le mauvais perimetre,
        // et les etats produits auraient l'air justes.
        assert!(matches!(
            Terrain::analyser("{ pas du json"),
            Err(ErreurTerrain::Illisible(_))
        ));
    }

    #[test]
    fn un_crm_absent_ou_vide_est_incoherent() {
        assert!(matches!(
            Terrain::analyser("{}"),
            Err(ErreurTerrain::Incoherent(_))
        ));
        let vide = serde_json::json!({"crm": "   "}).to_string();
        match Terrain::analyser(&vide) {
            Err(ErreurTerrain::Incoherent(c)) => assert!(c.contains("crm"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn un_budget_a_zero_est_refuse() {
        // Zero n'est pas « pas de limite » : c'est « aucune lecture », et
        // l'ecrire par accident rendrait toute la federation muette sans qu'une
        // seule erreur n'apparaisse.
        let z = serde_json::json!({"crm": "salesforce", "budgets": {"reads_per_episode": 0}})
            .to_string();
        match Terrain::analyser(&z) {
            Err(ErreurTerrain::Incoherent(c)) => assert!(c.contains("aucune lecture"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn un_budget_absent_retombe_sur_celui_de_r5_3() {
        let t = Terrain::analyser(&serde_json::json!({"crm": "salesforce"}).to_string()).unwrap();
        assert_eq!(t.budgets.reads_per_episode, BUDGET_PAR_DEFAUT);
    }

    #[test]
    fn une_tache_inconnue_n_a_pas_de_perimetre_et_ne_vaut_pas_perimetre_vide() {
        // L'appelant doit distinguer « rien a lire ici » de « lis tout », et un
        // Vec vide se confond avec le second.
        let t = Terrain::analyser(&json_minimal()).unwrap();
        assert!(t.perimetre("une-tache-jamais-declaree").is_none());
    }

    #[test]
    fn un_champ_vide_dans_un_perimetre_est_refuse() {
        // Un champ vide partirait dans l'URL de lecture et ferait echouer
        // l'appel avec un message qui parlerait de syntaxe.
        let j = serde_json::json!({
            "crm": "salesforce",
            "tasks": {"t": {"scope_fields": ["Status", "  "]}}
        })
        .to_string();
        match Terrain::analyser(&j) {
            Err(ErreurTerrain::Incoherent(c)) => assert!(c.contains("champ vide"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    // -- Regle 5 : jamais un secret dans un fichier suivi -------------------

    #[test]
    fn un_secret_dans_le_terrain_fait_refuser_le_fichier() {
        // Applique par une MECANIQUE et pas par une intention.
        for cle in [
            "client_secret",
            "refresh_token",
            "mot_de_passe",
            "API_TOKEN",
        ] {
            let j = serde_json::json!({"crm": "salesforce", cle: "peu importe"}).to_string();
            match Terrain::analyser(&j) {
                Err(ErreurTerrain::SecretSuspect(nom)) => assert_eq!(nom, cle),
                autre => panic!("{cle} : {autre:?}"),
            }
        }
    }

    #[test]
    fn un_secret_range_dans_un_champ_inconnu_est_trouve_quand_meme() {
        // Le controle porte sur le fichier ENTIER : un secret range dans un
        // champ qu'on ignore serait un secret quand meme.
        let j = serde_json::json!({
            "crm": "salesforce",
            "notes": {"divers": [{"jeton_de_service": "x"}]}
        })
        .to_string();
        assert!(matches!(
            Terrain::analyser(&j),
            Err(ErreurTerrain::SecretSuspect(_))
        ));
    }

    #[test]
    fn un_identifiant_client_n_est_pas_un_secret() {
        // PKCE est fait pour des clients publics : `client_id` a sa place ici,
        // et le refuser interdirait d'ecrire le fichier que la tache 0 doit
        // produire.
        let j = serde_json::json!({"crm": "salesforce", "client_id": "3MVG9..."}).to_string();
        assert!(Terrain::analyser(&j).is_ok());
    }

    /// Le miroir : l'exemple du dépôt doit être accepté par CE validateur.
    ///
    /// `docs/terrain.example.json` est généré par `apps/terrain/plan.mjs` et
    /// vérifié côté TypeScript. Sans ce test-ci, rien ne dirait que le fichier
    /// que l'outil produit est celui que l'application sait lire — et on ne
    /// l'apprendrait que le jour de la tâche 0, au pire moment.
    #[test]
    fn l_exemple_du_depot_est_accepte_par_ce_validateur() {
        const EXEMPLE: &str = include_str!("../../../../docs/terrain.example.json");
        let t = Terrain::analyser(EXEMPLE).expect("l exemple du depot doit etre lisible");
        assert_eq!(t.crm, "salesforce");
        assert_eq!(t.budgets.reads_per_episode, BUDGET_PAR_DEFAUT);

        // Les deux taches de reference, avec leur perimetre.
        let propre = t
            .perimetre("maj-crm-post-echange")
            .expect("tache de reference");
        assert_eq!(propre.scope_fields, vec!["Status", "Rating"]);
        assert_eq!(propre.objects, vec!["Lead", "Contact"]);

        // Celle qui porte le texte long expres — deuxieme piege du design §5 :
        // l'historique d'un texte long ne stocke pas ses valeurs.
        let avec_note = t.perimetre("maj-crm-avec-note").expect("tache avec note");
        assert!(avec_note.scope_fields.contains(&"Description".to_owned()));
    }

    #[test]
    fn le_terrain_fait_l_aller_retour_sans_perdre_de_champ() {
        let t = Terrain::analyser(&json_minimal()).unwrap();
        let refait = Terrain::analyser(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(t, refait);
    }
}
