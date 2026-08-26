//! Le moteur de capture : ce qui transforme un flux de [`RawEvent`] en journal.
//!
//! Tout le temporel de la spec 002 vit ici — 2 s d'inactivité après saisie,
//! retour d'application en moins de 60 s, reprise après plus de 10 s de pause,
//! clôture automatique à 60 minutes (R1.3) — et rien de tout cela ne touche à
//! l'horloge du système : le moteur reçoit une [`Horloge`], donc les quatre
//! scénarios se rejouent en quelques millisecondes de temps réel.
//!
//! Le moteur ne connaît ni UIA, ni le DOM, ni le disque. Il ne sait pas non plus
//! écrire : il produit un journal, et c'est le writer de la tâche 4 qui décidera
//! quoi en faire.

//! ## Code en attente de consommateur
//!
//! `allow(dead_code)` a l'echelle du module : le moteur est instancie par les
//! deux hotkeys, mais tant qu'aucune source reelle n'existe (tache 6a) il ne
//! recoit aucun evenement, si bien que la moitie de ses branches n'est atteinte
//! que par les tests. **A retirer en tache 6a.**
#![allow(dead_code)]

use crate::horloge::Horloge;
use crate::redaction::Redacteur;
use crate::source::{Cible, GenreEvenement, RawEvent, Source};

/// Les instants où la spec exige un snapshot (R2.3).
///
/// Le moteur les DÉTECTE ; la capture du snapshot elle-même est la tâche 7. La
/// détection est ce qui a besoin d'une horloge injectable, donc c'est elle qui
/// se teste maintenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Declencheur {
    Soumission,
    SaisiePuisInactivite,
    BasculeAvecRetour,
    PausePuisAction,
}

/// Les causes de trou. Miroir de `Gap.cause` d'`episode-spec` après l'extension
/// déclarée par cette spec (design §1bis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CauseGap {
    Crash,
    Kill,
    Sleep,
    SeqBreak,
    Manual,
    Pause,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntreeJournal {
    UiAction {
        seq: u64,
        monotone_ms: u64,
        source: Source,
        genre: GenreEvenement,
        /// R2.4 : jamais d'événement muet, mais jamais de faux résolu non plus.
        unresolved: bool,
    },
    Declencheur {
        seq: u64,
        monotone_ms: u64,
        quoi: Declencheur,
    },
    Gap {
        seq: u64,
        monotone_ms: u64,
        cause: CauseGap,
        debut_ms: u64,
        fin_ms: u64,
    },
    /// R1.3 : la borne oubliée.
    ClotureAuto { seq: u64, monotone_ms: u64 },
}

impl EntreeJournal {
    pub fn seq(&self) -> u64 {
        match self {
            Self::UiAction { seq, .. }
            | Self::Declencheur { seq, .. }
            | Self::Gap { seq, .. }
            | Self::ClotureAuto { seq, .. } => *seq,
        }
    }

    pub fn monotone_ms(&self) -> u64 {
        match self {
            Self::UiAction { monotone_ms, .. }
            | Self::Declencheur { monotone_ms, .. }
            | Self::Gap { monotone_ms, .. }
            | Self::ClotureAuto { monotone_ms, .. } => *monotone_ms,
        }
    }
}

/// Saisie suivie de ce délai d'inactivité (R2.3).
pub const INACTIVITE_MS: u64 = 2_000;
/// Retour d'application en deçà de ce délai (R2.3).
pub const RETOUR_MAX_MS: u64 = 60_000;
/// Pause au-delà de ce délai, puis action (R2.3).
pub const PAUSE_MIN_MS: u64 = 10_000;
/// Durée maximale d'un épisode (R1.3).
pub const TIMEOUT_MS: u64 = 3_600_000;

pub struct Moteur {
    horloge: std::sync::Arc<dyn Horloge>,
    /// R4.1 : rien n'entre au journal sans etre passe par la.
    redacteur: std::sync::Arc<Redacteur>,
    journal: Vec<EntreeJournal>,
    seq: u64,
    t0: u64,
    clos: bool,
    unresolved: u64,

    derniere_saisie: Option<u64>,
    derniere_action: u64,
    app_courante: String,
    /// L'application qu'on vient de quitter, et l'instant du départ.
    quittee: Option<(String, u64)>,
    veille_depuis: Option<u64>,
}

impl Moteur {
    /// L'horloge est PARTAGEE, pas empruntee : le moteur vit dans l'etat de
    /// l'application, ou une duree de vie liee a un emprunt ne passerait pas.
    pub fn ouvrir(
        horloge: std::sync::Arc<dyn Horloge>,
        redacteur: std::sync::Arc<Redacteur>,
        application: &str,
    ) -> Self {
        let t0 = horloge.monotone_ms();
        // Le nom de l'application est deja du texte du monde reel : un titre de
        // fenetre porte souvent le nom d'un client.
        let application = redacteur.redacter(application);
        Self {
            horloge,
            redacteur,
            journal: Vec::new(),
            seq: 0,
            t0,
            clos: false,
            unresolved: 0,
            derniere_saisie: None,
            derniere_action: t0,
            app_courante: application,
            quittee: None,
            veille_depuis: None,
        }
    }

    /// R3.1 : strictement croissant, par épisode.
    fn prochain_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    fn pousser(&mut self, entree: EntreeJournal) {
        self.journal.push(entree);
    }

    fn declencher(&mut self, quoi: Declencheur, monotone_ms: u64) {
        let seq = self.prochain_seq();
        self.pousser(EntreeJournal::Declencheur {
            seq,
            monotone_ms,
            quoi,
        });
    }

    fn trou(&mut self, cause: CauseGap, debut_ms: u64, fin_ms: u64) {
        let seq = self.prochain_seq();
        self.pousser(EntreeJournal::Gap {
            seq,
            monotone_ms: fin_ms,
            cause,
            debut_ms,
            fin_ms,
        });
    }

    /// R1.3 — la clôture automatique.
    ///
    /// Vérifiée à chaque battement ET à chaque événement : une machine qui sort
    /// de veille peut avoir franchi l'heure sans qu'aucun battement n'ait eu
    /// lieu entre-temps. Le calcul part donc de `t0`, jamais d'un compteur
    /// incrémenté — un compteur ne survit pas à un saut de temps.
    fn verifier_timeout(&mut self, maintenant: u64) -> bool {
        if self.clos || maintenant.saturating_sub(self.t0) < TIMEOUT_MS {
            return self.clos;
        }
        let borne = self.t0 + TIMEOUT_MS;
        self.trou(CauseGap::Timeout, borne, borne);
        let seq = self.prochain_seq();
        self.pousser(EntreeJournal::ClotureAuto {
            seq,
            monotone_ms: borne,
        });
        self.clos = true;
        true
    }

    /// Le déclencheur « saisie puis 2 s ».
    ///
    /// L'instant consigné est celui où le délai a EXPIRÉ, pas celui où on s'en
    /// aperçoit : sinon la granularité des battements se lirait dans les
    /// données, et deux exécutions du même scénario ne donneraient pas le même
    /// journal.
    fn verifier_inactivite(&mut self, maintenant: u64) {
        let Some(depuis) = self.derniere_saisie else {
            return;
        };
        if maintenant.saturating_sub(depuis) >= INACTIVITE_MS {
            self.derniere_saisie = None;
            self.declencher(Declencheur::SaisiePuisInactivite, depuis + INACTIVITE_MS);
        }
    }

    /// Fait avancer le temporel sans qu'aucun événement n'arrive.
    pub fn battre(&mut self) {
        let maintenant = self.horloge.monotone_ms();
        if self.verifier_timeout(maintenant) {
            return;
        }
        self.verifier_inactivite(maintenant);
    }

    /// Consomme un événement de capture.
    pub fn traiter(&mut self, ev: RawEvent) {
        // R1.2 : après clôture, plus rien n'entre. Jamais.
        if self.clos {
            return;
        }
        let maintenant = ev.monotone_ms;
        if self.verifier_timeout(maintenant) {
            return;
        }
        // Le délai d'inactivité a pu expirer AVANT cet événement : il doit donc
        // se consigner avant lui, sinon le journal raconte les choses à
        // l'envers.
        self.verifier_inactivite(maintenant);

        match &ev.genre {
            GenreEvenement::Veille => {
                self.veille_depuis = Some(maintenant);
                return;
            }
            GenreEvenement::Reveil => {
                // R3.3 : la veille est un trou, avec ses deux bornes.
                let debut = self.veille_depuis.take().unwrap_or(maintenant);
                self.trou(CauseGap::Sleep, debut, maintenant);
                // Au réveil, l'inactivité n'a plus de sens : la saisie date
                // d'avant la veille, et la « pause » n'est pas une hésitation de
                // l'opérateur mais une machine éteinte.
                self.derniere_saisie = None;
                self.derniere_action = maintenant;
                return;
            }
            GenreEvenement::BasculeApplication { vers } => {
                self.basculer(self.redacteur.redacter(vers), maintenant);
                return;
            }
            GenreEvenement::Saisie(_) => {
                self.derniere_saisie = Some(maintenant);
            }
            GenreEvenement::Soumission(_) => {
                self.declencher(Declencheur::Soumission, maintenant);
            }
            _ => {}
        }

        // « Pause > 10 s puis action » : c'est l'action qui déclenche, pas la
        // pause. Une pause qui ne serait jamais suivie d'un geste ne dit rien.
        if maintenant.saturating_sub(self.derniere_action) > PAUSE_MIN_MS {
            self.declencher(Declencheur::PausePuisAction, maintenant);
        }
        self.derniere_action = maintenant;

        if let Some(cible) = ev.genre.cible() {
            // « Resolu » se juge sur le nom BRUT : un nom vide le reste apres
            // redaction, et un nom redacte n'est pas un nom perdu.
            let unresolved = !cible.resolue();
            if unresolved {
                self.unresolved += 1;
            }
            // R4.1 : la redaction precede l'ecriture au journal, qui est la
            // premiere chose que le writer de la tache 4 persistera.
            let genre = self.redacteur.redacter_genre(&ev.genre);
            let seq = self.prochain_seq();
            self.pousser(EntreeJournal::UiAction {
                seq,
                monotone_ms: maintenant,
                source: ev.source,
                genre,
                unresolved,
            });
        }
    }

    fn basculer(&mut self, vers: String, maintenant: u64) {
        if let Some((partie, quitte_a)) = self.quittee.take() {
            if partie == vers && maintenant.saturating_sub(quitte_a) <= RETOUR_MAX_MS {
                self.declencher(Declencheur::BasculeAvecRetour, maintenant);
            }
        }
        let sortante = std::mem::replace(&mut self.app_courante, vers);
        self.quittee = Some((sortante, maintenant));
        self.derniere_action = maintenant;
    }

    /// Clôture demandée par l'opérateur (hotkey de fin).
    pub fn clore(&mut self) {
        if self.clos {
            return;
        }
        // Un délai d'inactivité déjà expiré appartient à l'épisode : le laisser
        // tomber à la clôture perdrait un snapshot que R2.3 exige.
        let maintenant = self.horloge.monotone_ms();
        self.verifier_inactivite(maintenant);
        self.clos = true;
    }

    pub fn journal(&self) -> &[EntreeJournal] {
        &self.journal
    }

    pub fn clos(&self) -> bool {
        self.clos
    }

    /// R2.4 : le compteur de santé.
    pub fn unresolved(&self) -> u64 {
        self.unresolved
    }

    pub fn declencheurs(&self) -> Vec<Declencheur> {
        self.journal
            .iter()
            .filter_map(|e| match e {
                EntreeJournal::Declencheur { quoi, .. } => Some(*quoi),
                _ => None,
            })
            .collect()
    }

    pub fn gaps(&self) -> Vec<CauseGap> {
        self.journal
            .iter()
            .filter_map(|e| match e {
                EntreeJournal::Gap { cause, .. } => Some(*cause),
                _ => None,
            })
            .collect()
    }
}

/// Fabrique une cible ordinaire, pour alléger les scénarios.
pub fn cible(role: &str, nom: &str) -> Cible {
    Cible::new(role, nom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::horloge::HorlogeSimulee;
    use crate::source::{attendre_ms, attendre_s, CaptureSource, Etape, FakeSource};
    use std::sync::mpsc::channel;
    use std::time::Duration;

    fn redacteur() -> std::sync::Arc<Redacteur> {
        std::sync::Arc::new(Redacteur::new(
            &crate::cle::CleHmac::generer().expect("alea"),
        ))
    }

    /// Granularité des battements pendant une attente simulée.
    ///
    /// Elle ne doit PAS se lire dans le journal : les instants consignés sont
    /// calculés depuis les bornes réelles. C'est ce qu'un test vérifie plus bas.
    const PAS_MS: u64 = 250;

    /// Rejoue un scénario complet en temps simulé et rend le moteur.
    fn rejouer(application: &str, etapes: Vec<Etape>) -> (Vec<EntreeJournal>, u64, bool) {
        let horloge = std::sync::Arc::new(HorlogeSimulee::new());
        let redacteur = redacteur();
        let mut source = FakeSource::new();
        let (tx, rx) = channel();
        let _abonnement = source.abonner(tx).expect("abonnement");
        let mut moteur = Moteur::ouvrir(horloge.clone(), redacteur, application);

        for etape in etapes {
            match etape {
                Etape::Attendre(duree) => {
                    let mut restant = duree.as_millis() as u64;
                    while restant > 0 {
                        let pas = restant.min(PAS_MS);
                        horloge.avancer(Duration::from_millis(pas));
                        restant -= pas;
                        moteur.battre();
                    }
                }
                Etape::Evenement(genre) => {
                    assert!(
                        source.emettre(genre, horloge.monotone_ms()),
                        "personne n ecoute la source : le scenario ne testerait rien"
                    );
                    for ev in crate::source::drainer(&rx) {
                        moteur.traiter(ev);
                    }
                }
            }
        }
        (
            moteur.journal().to_vec(),
            moteur.unresolved(),
            moteur.clos(),
        )
    }

    fn ev(genre: GenreEvenement) -> Etape {
        Etape::Evenement(genre)
    }

    fn declencheurs(journal: &[EntreeJournal]) -> Vec<Declencheur> {
        journal
            .iter()
            .filter_map(|e| match e {
                EntreeJournal::Declencheur { quoi, .. } => Some(*quoi),
                _ => None,
            })
            .collect()
    }

    fn gaps(journal: &[EntreeJournal]) -> Vec<CauseGap> {
        journal
            .iter()
            .filter_map(|e| match e {
                EntreeJournal::Gap { cause, .. } => Some(*cause),
                _ => None,
            })
            .collect()
    }

    // ---------------------------------------------------------------------
    // Les quatre scenarios rejouables de la tache 2.
    // ---------------------------------------------------------------------

    #[test]
    fn scenario_nominal() {
        let (journal, unresolved, clos) = rejouer(
            "chrome",
            vec![
                ev(GenreEvenement::Focus(cible("tab", "Details"))),
                attendre_ms(400),
                ev(GenreEvenement::Invocation(cible("button", "Modifier"))),
                attendre_ms(600),
                ev(GenreEvenement::ChangementValeur(cible(
                    "combobox",
                    "Statut de la piste",
                ))),
                attendre_ms(300),
                ev(GenreEvenement::Soumission(cible("button", "Enregistrer"))),
            ],
        );

        assert_eq!(declencheurs(&journal), vec![Declencheur::Soumission]);
        assert!(gaps(&journal).is_empty(), "aucun trou dans un nominal");
        assert_eq!(unresolved, 0);
        assert!(!clos, "seul le hotkey de fin clot un nominal");

        let actions = journal
            .iter()
            .filter(|e| matches!(e, EntreeJournal::UiAction { .. }))
            .count();
        assert_eq!(actions, 4, "les quatre gestes doivent etre au journal");
    }

    #[test]
    fn scenario_bascule_d_application() {
        let (journal, _, _) = rejouer(
            "chrome",
            vec![
                ev(GenreEvenement::Focus(cible("textbox", "Description"))),
                // On part vers Outlook, on revient 12 s plus tard : c est le
                // motif « je verifie un detail ailleurs » que R2.3 vise.
                ev(GenreEvenement::BasculeApplication {
                    vers: "outlook".into(),
                }),
                attendre_s(12),
                ev(GenreEvenement::BasculeApplication {
                    vers: "chrome".into(),
                }),
                attendre_ms(500),
                ev(GenreEvenement::Invocation(cible("button", "Enregistrer"))),
            ],
        );

        assert!(
            declencheurs(&journal).contains(&Declencheur::BasculeAvecRetour),
            "retour en 12 s : le declencheur doit partir"
        );
    }

    #[test]
    fn un_retour_trop_tardif_ne_declenche_pas() {
        let (journal, _, _) = rejouer(
            "chrome",
            vec![
                ev(GenreEvenement::BasculeApplication {
                    vers: "outlook".into(),
                }),
                // 61 s : au-dela de la fenetre, ce n est plus le meme geste.
                attendre_s(61),
                ev(GenreEvenement::BasculeApplication {
                    vers: "chrome".into(),
                }),
            ],
        );

        assert!(
            !declencheurs(&journal).contains(&Declencheur::BasculeAvecRetour),
            "au-dela de 60 s, ce n est pas un aller-retour"
        );
    }

    #[test]
    fn scenario_saisie_puis_pause() {
        let (journal, _, _) = rejouer(
            "chrome",
            vec![
                ev(GenreEvenement::Saisie(cible("textbox", "Description"))),
                attendre_ms(800),
                ev(GenreEvenement::Saisie(cible("textbox", "Description"))),
                // Deux secondes pleines sans frappe : le declencheur part.
                attendre_s(3),
                ev(GenreEvenement::Invocation(cible("button", "Enregistrer"))),
            ],
        );

        let d = declencheurs(&journal);
        assert!(
            d.contains(&Declencheur::SaisiePuisInactivite),
            "2 s d inactivite apres saisie : declencheur attendu, obtenu {d:?}"
        );
        assert_eq!(
            d.iter()
                .filter(|x| **x == Declencheur::SaisiePuisInactivite)
                .count(),
            1,
            "la seconde frappe reinitialise le delai : un seul declenchement"
        );
    }

    #[test]
    fn scenario_timeout_soixante_minutes_en_temps_simule() {
        let (journal, _, clos) = rejouer(
            "chrome",
            vec![
                ev(GenreEvenement::Focus(cible("tab", "Details"))),
                // Une heure et une minute, en quelques millisecondes reelles.
                attendre_s(3_660),
                ev(GenreEvenement::Invocation(cible("button", "Enregistrer"))),
            ],
        );

        assert!(clos, "R1.3 : l episode doit se clore tout seul");
        assert_eq!(gaps(&journal), vec![CauseGap::Timeout]);
        assert!(
            matches!(journal.last(), Some(EntreeJournal::ClotureAuto { .. })),
            "la cloture est la DERNIERE entree, journal : {journal:?}"
        );

        // R1.2 : le geste qui suit la cloture ne doit rien produire.
        let apres = journal
            .iter()
            .skip_while(|e| !matches!(e, EntreeJournal::ClotureAuto { .. }))
            .count();
        assert_eq!(apres, 1, "rien ne s ecrit apres la cloture");
    }

    // ---------------------------------------------------------------------
    // Proprietes transversales.
    // ---------------------------------------------------------------------

    #[test]
    fn la_borne_du_timeout_est_exacte_a_la_milliseconde() {
        let (journal, _, _) = rejouer(
            "chrome",
            vec![
                ev(GenreEvenement::Focus(cible("tab", "X"))),
                attendre_s(3_700),
            ],
        );
        let gap = journal
            .iter()
            .find(|e| matches!(e, EntreeJournal::Gap { .. }))
            .expect("un gap de timeout");
        assert_eq!(
            gap.monotone_ms(),
            TIMEOUT_MS,
            "la borne doit valoir t0+60 min, pas l instant du battement"
        );
    }

    #[test]
    fn l_instant_du_declencheur_d_inactivite_ne_depend_pas_du_pas_de_battement() {
        // La saisie tombe a 100 ms, donc le delai expire a 2100 ms — un instant
        // qu aucun battement multiple de 250 ms ne visite.
        let (journal, _, _) = rejouer(
            "chrome",
            vec![
                attendre_ms(100),
                ev(GenreEvenement::Saisie(cible("textbox", "Note"))),
                attendre_s(4),
            ],
        );
        let d = journal
            .iter()
            .find(|e| {
                matches!(
                    e,
                    EntreeJournal::Declencheur {
                        quoi: Declencheur::SaisiePuisInactivite,
                        ..
                    }
                )
            })
            .expect("le declencheur d inactivite");
        assert_eq!(
            d.monotone_ms(),
            100 + INACTIVITE_MS,
            "l instant consigne doit etre celui de l expiration"
        );
    }

    #[test]
    fn les_seq_sont_strictement_croissants() {
        let (journal, _, _) = rejouer(
            "chrome",
            vec![
                ev(GenreEvenement::Saisie(cible("textbox", "Note"))),
                attendre_s(3),
                ev(GenreEvenement::Soumission(cible("button", "Enregistrer"))),
                ev(GenreEvenement::Veille),
                attendre_s(30),
                ev(GenreEvenement::Reveil),
            ],
        );
        assert!(!journal.is_empty());
        for paire in journal.windows(2) {
            assert!(
                paire[1].seq() > paire[0].seq(),
                "R3.1 : {} n est pas > {}",
                paire[1].seq(),
                paire[0].seq()
            );
        }
    }

    #[test]
    fn la_veille_produit_un_trou_avec_ses_deux_bornes() {
        let (journal, _, _) = rejouer(
            "chrome",
            vec![
                ev(GenreEvenement::Focus(cible("tab", "X"))),
                ev(GenreEvenement::Veille),
                attendre_s(90),
                ev(GenreEvenement::Reveil),
            ],
        );
        let gap = journal
            .iter()
            .find_map(|e| match e {
                EntreeJournal::Gap {
                    cause: CauseGap::Sleep,
                    debut_ms,
                    fin_ms,
                    ..
                } => Some((*debut_ms, *fin_ms)),
                _ => None,
            })
            .expect("R3.3 : un gap de veille");
        assert_eq!(
            gap.1 - gap.0,
            90_000,
            "les bornes doivent encadrer la veille"
        );
    }

    #[test]
    fn une_cible_non_resolue_est_enregistree_et_comptee() {
        let (journal, unresolved, _) = rejouer(
            "chrome",
            vec![ev(GenreEvenement::Invocation(cible("", "")))],
        );
        assert_eq!(unresolved, 1, "R2.4 : le compteur de sante doit bouger");
        assert!(
            matches!(
                journal.first(),
                Some(EntreeJournal::UiAction {
                    unresolved: true,
                    ..
                })
            ),
            "R2.4 : jamais d evenement muet"
        );
    }

    #[test]
    fn une_reprise_apres_plus_de_dix_secondes_declenche() {
        let (journal, _, _) = rejouer(
            "chrome",
            vec![
                ev(GenreEvenement::Focus(cible("tab", "X"))),
                attendre_s(15),
                ev(GenreEvenement::Invocation(cible("button", "Enregistrer"))),
            ],
        );
        assert!(declencheurs(&journal).contains(&Declencheur::PausePuisAction));
    }

    #[test]
    fn le_reveil_n_est_pas_une_hesitation_de_l_operateur() {
        // Sans traitement particulier, une veille de 20 minutes serait lue comme
        // une « pause puis action » : ce serait attribuer a l operateur un temps
        // de reflexion qui n a pas eu lieu, et polluer les statistiques de la
        // spec 007.
        let (journal, _, _) = rejouer(
            "chrome",
            vec![
                ev(GenreEvenement::Focus(cible("tab", "X"))),
                ev(GenreEvenement::Veille),
                attendre_s(1_200),
                ev(GenreEvenement::Reveil),
                ev(GenreEvenement::Invocation(cible("button", "Enregistrer"))),
            ],
        );
        assert!(
            !declencheurs(&journal).contains(&Declencheur::PausePuisAction),
            "une veille n est pas une pause de travail"
        );
    }

    #[test]
    fn clore_ne_perd_pas_un_delai_deja_expire() {
        let horloge = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(horloge.clone(), redacteur(), "chrome");
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 0,
            genre: GenreEvenement::Saisie(cible("textbox", "Note")),
        });
        horloge.avancer(Duration::from_secs(5));
        m.clore();

        assert!(
            m.declencheurs()
                .contains(&Declencheur::SaisiePuisInactivite),
            "R2.3 : le snapshot du a l inactivite appartient a l episode"
        );
        assert!(m.clos());
    }

    #[test]
    fn apres_cloture_manuelle_plus_rien_n_entre() {
        let horloge = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(horloge.clone(), redacteur(), "chrome");
        m.clore();
        let avant = m.journal().len();
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 10,
            genre: GenreEvenement::Invocation(cible("button", "Enregistrer")),
        });
        assert_eq!(m.journal().len(), avant, "R1.2 : rien apres la cloture");
    }

    #[test]
    fn le_timeout_survit_a_un_saut_de_temps_sans_battement() {
        // La machine dort une heure et personne ne bat : au premier evenement,
        // la cloture doit avoir eu lieu quand meme. Un compteur incremente a
        // chaque battement raterait ce cas.
        let horloge = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(horloge.clone(), redacteur(), "chrome");
        horloge.avancer(Duration::from_secs(4_000));
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: horloge.monotone_ms(),
            genre: GenreEvenement::Invocation(cible("button", "Enregistrer")),
        });
        assert!(m.clos(), "R1.3 doit tenir sans battement intermediaire");
        assert_eq!(m.gaps(), vec![CauseGap::Timeout]);
    }
}
