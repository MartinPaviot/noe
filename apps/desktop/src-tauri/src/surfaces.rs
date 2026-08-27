//! La liste blanche des surfaces observables (spec 002, R5.4).
//!
//! **Vide par défaut, et c'est le point.** Rien n'est capturé tant que
//! l'opérateur n'a pas explicitement autorisé une application. Une liste blanche
//! pré-remplie « pour rendre service » ferait exactement ce que le produit
//! promet de ne pas faire : observer sans qu'on l'ait demandé.
//!
//! Une surface est identifiée par le **nom de l'exécutable** — `chrome.exe`,
//! `outlook.exe` — et non par le titre de la fenêtre. Un titre change à chaque
//! fiche ouverte et porte souvent le nom d'un client ; il ne peut être ni une clé
//! stable, ni une chose qu'on écrit dans une configuration en clair.

use std::collections::BTreeSet;

/// Normalise un identifiant de surface.
///
/// Windows ne distingue pas la casse des noms de fichiers : `Chrome.exe` et
/// `chrome.exe` sont la même application, et les traiter comme deux surfaces
/// laisserait passer la capture d'une application qu'on croyait avoir refusée.
pub fn normaliser(surface: &str) -> String {
    surface.trim().to_lowercase()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ListeBlanche {
    autorisees: BTreeSet<String>,
}

impl ListeBlanche {
    pub fn vide() -> Self {
        Self::default()
    }

    // La production construit la liste au fil des clics de l'operateur, jamais
    // d'un bloc. Ce constructeur sert aux bancs — et il porte la normalisation,
    // qui doit etre testee la ou elle s'ecrit.
    #[allow(dead_code)]
    pub fn depuis<I: IntoIterator<Item = S>, S: AsRef<str>>(surfaces: I) -> Self {
        Self {
            autorisees: surfaces
                .into_iter()
                .map(|s| normaliser(s.as_ref()))
                .filter(|s| !s.is_empty())
                .collect(),
        }
    }

    /// R5.4 : hors liste, la capture est refusée.
    ///
    /// Une surface **inconnue** est refusée, jamais tolérée : le défaut penche du
    /// côté qui n'observe pas. Un événement sans surface identifiée l'est aussi —
    /// on ne peut pas autoriser ce qu'on n'a pas su nommer.
    pub fn autorise(&self, surface: Option<&str>) -> bool {
        match surface {
            Some(s) => self.autorisees.contains(&normaliser(s)),
            None => false,
        }
    }

    pub fn autoriser(&mut self, surface: &str) -> bool {
        let s = normaliser(surface);
        if s.is_empty() {
            return false;
        }
        self.autorisees.insert(s)
    }

    pub fn retirer(&mut self, surface: &str) -> bool {
        self.autorisees.remove(&normaliser(surface))
    }

    pub fn basculer(&mut self, surface: &str) -> bool {
        if self.autorise(Some(surface)) {
            self.retirer(surface);
            false
        } else {
            self.autoriser(surface);
            true
        }
    }

    pub fn est_vide(&self) -> bool {
        self.autorisees.is_empty()
    }

    pub fn liste(&self) -> Vec<String> {
        self.autorisees.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_liste_vide_n_autorise_rien() {
        // R5.4, dans sa forme la plus forte : au premier lancement, rien n'est
        // observable. Un produit qui capturerait « juste un peu » par defaut
        // trahirait sa promesse avant meme d'avoir servi.
        let l = ListeBlanche::vide();
        assert!(l.est_vide());
        for s in ["chrome.exe", "outlook.exe", "explorer.exe", ""] {
            assert!(!l.autorise(Some(s)), "{s} ne doit pas passer");
        }
    }

    #[test]
    fn une_surface_inconnue_est_refusee_pas_toleree() {
        let l = ListeBlanche::depuis(["chrome.exe"]);
        assert!(l.autorise(Some("chrome.exe")));
        assert!(!l.autorise(Some("outlook.exe")));
    }

    #[test]
    fn un_evenement_sans_surface_est_refuse() {
        // On ne peut pas autoriser ce qu'on n'a pas su nommer. Le doute ne
        // profite pas a la capture.
        let l = ListeBlanche::depuis(["chrome.exe"]);
        assert!(!l.autorise(None));
    }

    #[test]
    fn la_casse_ne_cree_pas_deux_surfaces() {
        // Windows ne la distingue pas : les traiter separement laisserait passer
        // une application qu'on croyait avoir refusee.
        let l = ListeBlanche::depuis(["Chrome.exe"]);
        for graphie in ["chrome.exe", "CHROME.EXE", "Chrome.Exe", "  chrome.exe  "] {
            assert!(l.autorise(Some(graphie)), "{graphie}");
        }
    }

    #[test]
    fn autoriser_puis_retirer_revient_au_refus() {
        let mut l = ListeBlanche::vide();
        l.autoriser("chrome.exe");
        assert!(l.autorise(Some("chrome.exe")));
        l.retirer("chrome.exe");
        assert!(!l.autorise(Some("chrome.exe")));
        assert!(l.est_vide());
    }

    #[test]
    fn basculer_alterne_et_rend_le_nouvel_etat() {
        let mut l = ListeBlanche::vide();
        assert!(l.basculer("chrome.exe"), "premier appel : autorise");
        assert!(l.autorise(Some("chrome.exe")));
        assert!(!l.basculer("chrome.exe"), "second appel : retire");
        assert!(!l.autorise(Some("chrome.exe")));
    }

    #[test]
    fn une_surface_vide_n_entre_pas_dans_la_liste() {
        // Sinon une chaine vide autoriserait tout evenement dont la surface n'a
        // pas ete resolue.
        let mut l = ListeBlanche::vide();
        assert!(!l.autoriser("   "));
        assert!(l.est_vide());
        assert!(!l.autorise(Some("")));
    }

    #[test]
    fn autoriser_deux_fois_ne_duplique_pas() {
        let mut l = ListeBlanche::vide();
        l.autoriser("chrome.exe");
        l.autoriser("CHROME.EXE");
        assert_eq!(l.liste(), vec!["chrome.exe".to_string()]);
    }

    #[test]
    fn la_liste_sort_triee_donc_stable() {
        // L'ordre alimente un menu et une configuration ecrite sur disque : un
        // ordre variable produirait des diffs sans changement.
        let l = ListeBlanche::depuis(["outlook.exe", "chrome.exe", "explorer.exe"]);
        assert_eq!(
            l.liste(),
            vec![
                "chrome.exe".to_string(),
                "explorer.exe".to_string(),
                "outlook.exe".to_string()
            ]
        );
    }

    #[test]
    fn l_aller_retour_json_conserve_la_liste() {
        let l = ListeBlanche::depuis(["chrome.exe", "outlook.exe"]);
        let json = serde_json::to_string(&l).unwrap();
        assert_eq!(serde_json::from_str::<ListeBlanche>(&json).unwrap(), l);
    }
}
