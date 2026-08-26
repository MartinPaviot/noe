//! Le trait de capture et sa doublure (spec 002, design §1).
//!
//! `CaptureSource` est la frontière que D19 a rendue essentielle : `UiaSource`
//! répond des applications natives, `DomSource` des surfaces navigateur, et
//! `FakeSource` des tests. Les trois produisent le MÊME [`RawEvent`], si bien
//! que tout ce qui est en aval — redaction, writer, assemblage — ignore d'où
//! l'événement vient.
//!
//! La doublure n'ouvre aucun thread et ne dort jamais. Un banc qui s'appuierait
//! sur l'ordonnanceur pour livrer ses événements « à peu près au bon moment »
//! rendrait les scénarios non reproductibles, et c'est la reproductibilité qui
//! fait toute la valeur d'un rejeu.

//! ## Code en attente de consommateur
//!
//! `allow(dead_code)` a l'echelle du module, et c'est deliberé : ce module est
//! le SOCLE que la tache 2 livre, son consommateur de production est la tache 6a
//! (`UiaSource`). Rien ici n'est mort — tout est exerce par les quatre scenarios
//! rejouables — mais rien n'est encore construit depuis le chemin du binaire.
//!
//! **A retirer en tache 6a**, ou l'adaptateur reel construira ces types. La
//! consigne est inscrite dans `tasks.md` pour que l'oubli se voie.
#![allow(dead_code)]

#[cfg(test)]
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
#[cfg(test)]
use std::time::Duration;

/// Qui a produit l'événement (`[amendé D19]`, design §1bis).
///
/// Descriptif, jamais normatif : aucune règle de grade ne s'y adosse. Il sert à
/// diagnostiquer un épisode mixte, où l'opérateur passe d'une application native
/// au navigateur au milieu d'une tâche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Uia,
    Dom,
    Fake,
}

/// L'élément visé, décrit sémantiquement (R2.2).
///
/// Ni sélecteur CSS, ni XPath, ni coordonnée écran : le schéma ne les accepte
/// pas et le type ne les offre pas.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Cible {
    pub role: String,
    pub nom: String,
    pub region: Option<String>,
}

impl Cible {
    pub fn new(role: &str, nom: &str) -> Self {
        Self {
            role: role.to_string(),
            nom: nom.to_string(),
            region: None,
        }
    }

    pub fn dans(mut self, region: &str) -> Self {
        self.region = Some(region.to_string());
        self
    }

    /// R2.4 : un rôle ou un nom manquant rend la cible non résolue.
    ///
    /// L'événement est enregistré quand même, marqué, et compté. Le jeter
    /// donnerait un épisode plus propre et faux : la statistique de santé ne
    /// verrait jamais que le capteur perd du terrain.
    pub fn resolue(&self) -> bool {
        !self.role.trim().is_empty() && !self.nom.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GenreEvenement {
    /// Le focus arrive sur un élément.
    Focus(Cible),
    /// Un bouton, un lien, un élément de menu est actionné.
    Invocation(Cible),
    /// La valeur d'un champ change.
    ChangementValeur(Cible),
    /// Le conteneur se réorganise (ouverture de panneau, rendu différé).
    ChangementStructure(Cible),
    /// Une frappe. Le contenu n'est jamais porté ici : R4.5.
    Saisie(Cible),
    /// Un formulaire part.
    Soumission(Cible),
    /// L'application au premier plan change.
    BasculeApplication {
        vers: String,
    },
    /// La machine s'endort, puis se réveille (R3.3).
    Veille,
    Reveil,
}

impl GenreEvenement {
    pub fn cible(&self) -> Option<&Cible> {
        match self {
            Self::Focus(c)
            | Self::Invocation(c)
            | Self::ChangementValeur(c)
            | Self::ChangementStructure(c)
            | Self::Saisie(c)
            | Self::Soumission(c) => Some(c),
            Self::BasculeApplication { .. } | Self::Veille | Self::Reveil => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEvent {
    pub source: Source,
    pub monotone_ms: u64,
    pub genre: GenreEvenement,
}

#[derive(Debug)]
pub enum ErreurSource {
    DejaAbonne,
}

impl std::fmt::Display for ErreurSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DejaAbonne => write!(f, "cette source a deja un abonnement actif"),
        }
    }
}

impl std::error::Error for ErreurSource {}

/// L'abonnement. Le relâcher coupe le flux.
///
/// Le type existe pour que la désinscription soit portée par la durée de vie :
/// une source qu'on oublie de désabonner continuerait de pousser des événements
/// dans un épisode clos, ce que R1.2 interdit.
#[must_use = "relacher l abonnement coupe immediatement la capture"]
pub struct Abonnement {
    _prive: (),
}

impl Abonnement {
    fn nouveau() -> Self {
        Self { _prive: () }
    }
}

pub trait CaptureSource {
    fn abonner(&mut self, puits: Sender<RawEvent>) -> Result<Abonnement, ErreurSource>;
}

/// Une étape de scénario.
#[cfg(test)]
#[derive(Debug, Clone)]
pub enum Etape {
    /// Laisser le temps passer — en temps SIMULÉ.
    Attendre(Duration),
    /// Émettre un événement à l'instant courant.
    Evenement(GenreEvenement),
}

/// Raccourci de lecture pour écrire des scénarios sans bruit.
#[cfg(test)]
pub fn attendre_s(s: u64) -> Etape {
    Etape::Attendre(Duration::from_secs(s))
}

#[cfg(test)]
pub fn attendre_ms(ms: u64) -> Etape {
    Etape::Attendre(Duration::from_millis(ms))
}

/// La source des tests : elle n'émet que ce qu'on lui dit, quand on le lui dit.
#[cfg(test)]
#[derive(Default)]
pub struct FakeSource {
    puits: Option<Sender<RawEvent>>,
}

#[cfg(test)]
impl FakeSource {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pousse un événement daté par l'horloge fournie.
    ///
    /// Rend `false` si personne n'écoute — un scénario qui émet dans le vide est
    /// un scénario qui ne teste rien, et le test doit pouvoir le voir.
    pub fn emettre(&self, genre: GenreEvenement, monotone_ms: u64) -> bool {
        match &self.puits {
            Some(p) => p
                .send(RawEvent {
                    source: Source::Fake,
                    monotone_ms,
                    genre,
                })
                .is_ok(),
            None => false,
        }
    }
}

#[cfg(test)]
impl CaptureSource for FakeSource {
    fn abonner(&mut self, puits: Sender<RawEvent>) -> Result<Abonnement, ErreurSource> {
        if self.puits.is_some() {
            return Err(ErreurSource::DejaAbonne);
        }
        self.puits = Some(puits);
        Ok(Abonnement::nouveau())
    }
}

/// Draine tout ce qui attend, sans jamais bloquer.
#[cfg(test)]
pub fn drainer(recepteur: &Receiver<RawEvent>) -> Vec<RawEvent> {
    recepteur.try_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn une_cible_sans_nom_n_est_pas_resolue() {
        assert!(Cible::new("button", "Enregistrer").resolue());
        assert!(!Cible::new("button", "").resolue());
        assert!(!Cible::new("", "Enregistrer").resolue());
        assert!(
            !Cible::new("button", "   ").resolue(),
            "R2.4 : espaces = vide"
        );
    }

    #[test]
    fn la_region_est_optionnelle_et_se_pose_a_la_demande() {
        let c = Cible::new("textbox", "Description").dans("Details");
        assert_eq!(c.region.as_deref(), Some("Details"));
    }

    #[test]
    fn emettre_sans_abonne_echoue_franchement() {
        let f = FakeSource::new();
        assert!(
            !f.emettre(GenreEvenement::Focus(Cible::new("button", "X")), 0),
            "un scenario qui emet dans le vide doit etre visible"
        );
    }

    #[test]
    fn un_second_abonnement_est_refuse() {
        let mut f = FakeSource::new();
        let (tx, _rx) = channel();
        let _a = f.abonner(tx).unwrap();
        let (tx2, _rx2) = channel();
        assert!(matches!(f.abonner(tx2), Err(ErreurSource::DejaAbonne)));
    }

    #[test]
    fn les_evenements_portent_leur_source_et_leur_instant() {
        let mut f = FakeSource::new();
        let (tx, rx) = channel();
        let _a = f.abonner(tx).unwrap();

        assert!(f.emettre(
            GenreEvenement::Soumission(Cible::new("button", "Enregistrer")),
            1234
        ));
        let recus = drainer(&rx);

        assert_eq!(recus.len(), 1);
        assert_eq!(recus[0].source, Source::Fake);
        assert_eq!(recus[0].monotone_ms, 1234);
    }

    #[test]
    fn les_genres_sans_cible_le_disent() {
        assert!(GenreEvenement::Veille.cible().is_none());
        assert!(GenreEvenement::BasculeApplication {
            vers: "chrome".into()
        }
        .cible()
        .is_none());
        assert!(GenreEvenement::Focus(Cible::new("tab", "Details"))
            .cible()
            .is_some());
    }
}
