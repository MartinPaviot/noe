//! Configuration persistée : la liste des tâches et celle qui est active.
//!
//! Le `task_slug` d'un épisode vient d'ici (spec 002, R1.1). C'est donc le seul
//! endroit qui décide si le hotkey de début a le droit d'ouvrir quoi que ce
//! soit — et la raison pour laquelle ce module ne connaît ni Tauri ni le disque
//! au-delà d'un chemin : il se teste entièrement, sans écran ni process.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Ce qui empêche de définir une tâche active.
#[derive(Debug, PartialEq, Eq)]
pub enum ErreurConfig {
    /// Le slug ne figure pas dans la liste connue.
    ///
    /// Sans ce refus, un slug effacé du sous-menu resterait actif en
    /// configuration, et les épisodes suivants porteraient une étiquette qui ne
    /// correspond plus à rien — un corpus qu'on ne peut plus regrouper.
    TacheInconnue(String),
    /// Le slug est vide ou n'est pas un identifiant utilisable.
    SlugInvalide(String),
}

impl std::fmt::Display for ErreurConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TacheInconnue(s) => write!(f, "tache inconnue : {s}"),
            Self::SlugInvalide(s) => write!(f, "slug invalide : {s}"),
        }
    }
}

impl std::error::Error for ErreurConfig {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    /// Les tâches que l'opérateur a déclarées, dans l'ordre d'affichage.
    pub taches: Vec<String>,
    /// Celle qui sera étiquetée sur le prochain épisode. `None` = aucune.
    pub tache_active: Option<String>,
    /// R5.4 : les applications sur lesquelles la capture a le droit d'avoir
    /// lieu. Vide à l'installation, et vide aussi pour une configuration écrite
    /// par une version qui ne connaissait pas ce champ — `serde(default)` fait
    /// pencher la mise à jour du côté qui n'observe pas.
    #[serde(default)]
    pub surfaces: crate::surfaces::ListeBlanche,
}

impl Default for Config {
    /// Aucune tâche active au premier lancement, volontairement.
    ///
    /// En choisir une par défaut ferait démarrer une capture sur une étiquette
    /// que l'opérateur n'a pas choisie. Le refus au premier hotkey est le
    /// comportement voulu, pas une friction à contourner.
    fn default() -> Self {
        Self {
            taches: vec!["maj-crm-post-echange".to_string()],
            tache_active: None,
            // R5.4 : aucune surface activée au premier lancement. Une liste
            // pré-remplie « pour rendre service » ferait exactement ce que le
            // produit promet de ne pas faire.
            surfaces: crate::surfaces::ListeBlanche::vide(),
        }
    }
}

/// Un slug utilisable : minuscules, chiffres et tirets, ni vide ni démesuré.
fn slug_valide(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl Config {
    pub fn definir_active(&mut self, slug: &str) -> Result<(), ErreurConfig> {
        if !slug_valide(slug) {
            return Err(ErreurConfig::SlugInvalide(slug.to_string()));
        }
        if !self.taches.iter().any(|t| t == slug) {
            return Err(ErreurConfig::TacheInconnue(slug.to_string()));
        }
        self.tache_active = Some(slug.to_string());
        Ok(())
    }

    // Aucune interface ne declare encore de tache : le sous-menu se contente de
    // choisir parmi la liste existante. La saisie arrive avec l onboarding de la
    // spec 008 ; la regle de validation, elle, doit exister et etre testee des
    // maintenant, sinon elle s ecrira dans l urgence d un ecran a livrer.
    #[allow(dead_code)]
    pub fn ajouter_tache(&mut self, slug: &str) -> Result<(), ErreurConfig> {
        if !slug_valide(slug) {
            return Err(ErreurConfig::SlugInvalide(slug.to_string()));
        }
        if !self.taches.iter().any(|t| t == slug) {
            self.taches.push(slug.to_string());
        }
        Ok(())
    }

    /// Charge, ou rend la configuration par défaut.
    ///
    /// Un fichier illisible ou corrompu ne fait PAS échouer le démarrage : on
    /// repart du défaut. Refuser de se lancer parce qu'un JSON est abîmé
    /// priverait l'opérateur de son bouton panique, qui est justement ce dont on
    /// a besoin quand quelque chose va mal. La tâche active retombe alors à
    /// `None`, donc rien ne se capture tant qu'elle n'est pas rechoisie.
    pub fn charger(chemin: &Path) -> Self {
        std::fs::read_to_string(chemin)
            .ok()
            .and_then(|t| serde_json::from_str::<Self>(&t).ok())
            .map(|mut c| {
                // Une tâche active qui ne figure plus dans la liste est écartée.
                if let Some(a) = &c.tache_active {
                    if !c.taches.iter().any(|t| t == a) {
                        c.tache_active = None;
                    }
                }
                c
            })
            .unwrap_or_default()
    }

    pub fn enregistrer(&self, chemin: &Path) -> std::io::Result<()> {
        if let Some(parent) = chemin.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(chemin, serde_json::to_string_pretty(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporaire(nom: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("noe-test-{nom}-{}.json", std::process::id()));
        p
    }

    #[test]
    fn aucune_surface_activee_au_premier_lancement() {
        // R5.4 : le moteur tourne, la capture non. C'est l'etat voulu.
        assert!(Config::default().surfaces.est_vide());
    }

    #[test]
    fn une_config_ecrite_avant_la_liste_blanche_se_relit_sans_surface() {
        // Une mise a jour ne doit pas activer une capture que l'operateur n'a
        // pas demandee. Le champ absent vaut « aucune surface », jamais
        // « toutes ».
        let p = temporaire("ancienne");
        std::fs::write(&p, r#"{"taches":["a-faire"],"tache_active":null}"#).unwrap();
        let c = Config::charger(&p);
        assert!(c.surfaces.est_vide());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn les_surfaces_survivent_a_un_aller_retour_disque() {
        let p = temporaire("surfaces");
        let mut c = Config::default();
        c.surfaces.autoriser("chrome.exe");
        c.enregistrer(&p).unwrap();
        assert!(Config::charger(&p).surfaces.autorise(Some("chrome.exe")));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn aucune_tache_active_au_premier_lancement() {
        assert_eq!(Config::default().tache_active, None);
    }

    #[test]
    fn definir_active_refuse_une_tache_inconnue() {
        let mut c = Config::default();
        let r = c.definir_active("tache-jamais-declaree");
        assert_eq!(
            r,
            Err(ErreurConfig::TacheInconnue("tache-jamais-declaree".into()))
        );
        assert_eq!(c.tache_active, None, "l etat ne doit pas avoir bouge");
    }

    #[test]
    fn definir_active_refuse_un_slug_malforme() {
        let mut c = Config::default();
        for mauvais in ["", "Avec Majuscules", "espace ici", "accentué"] {
            assert!(
                matches!(
                    c.definir_active(mauvais),
                    Err(ErreurConfig::SlugInvalide(_))
                ),
                "« {mauvais} » aurait du etre refuse"
            );
        }
    }

    #[test]
    fn definir_active_accepte_une_tache_declaree() {
        let mut c = Config::default();
        c.definir_active("maj-crm-post-echange").unwrap();
        assert_eq!(c.tache_active.as_deref(), Some("maj-crm-post-echange"));
    }

    #[test]
    fn ajouter_est_idempotent() {
        let mut c = Config::default();
        c.ajouter_tache("relance-devis").unwrap();
        c.ajouter_tache("relance-devis").unwrap();
        assert_eq!(c.taches.iter().filter(|t| *t == "relance-devis").count(), 1);
    }

    #[test]
    fn aller_retour_disque() {
        let p = temporaire("aller-retour");
        let mut c = Config::default();
        c.ajouter_tache("relance-devis").unwrap();
        c.definir_active("relance-devis").unwrap();
        c.enregistrer(&p).unwrap();

        assert_eq!(Config::charger(&p), c);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn un_fichier_corrompu_ne_bloque_pas_le_demarrage() {
        let p = temporaire("corrompu");
        std::fs::write(&p, "{ ceci n est pas du json").unwrap();
        let c = Config::charger(&p);
        assert_eq!(c, Config::default());
        assert_eq!(c.tache_active, None, "rien ne doit se capturer par defaut");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn une_tache_active_disparue_de_la_liste_est_ecartee() {
        let p = temporaire("fantome");
        std::fs::write(
            &p,
            r#"{"taches":["a-faire"],"tache_active":"tache-supprimee"}"#,
        )
        .unwrap();
        assert_eq!(
            Config::charger(&p).tache_active,
            None,
            "un slug absent de la liste ne doit pas rester actif"
        );
        let _ = std::fs::remove_file(&p);
    }
}
