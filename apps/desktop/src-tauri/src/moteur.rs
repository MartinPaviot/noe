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

use crate::horloge::Horloge;
use crate::journal::Journal;
use crate::redaction::Redacteur;
use crate::snapshot::{self, Noeud, Snapshot};
use crate::source::{GenreEvenement, RawEvent, Source};

/// Les instants où la spec exige un snapshot (R2.3).
///
/// Le moteur les DÉTECTE et demande la photo à un [`Snapshotteur`] ; c'est la
/// source qui sait regarder l'écran. Cette séparation garde toute la logique
/// temporelle testable en temps simulé, sans bureau.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Declencheur {
    Soumission,
    SaisiePuisInactivite,
    BasculeAvecRetour,
    PausePuisAction,
    /// Le cinquième : un collage dont la copie a eu lieu pendant l'épisode.
    CopierColler,
}

/// Les causes de trou. Miroir de `Gap.cause` d'`episode-spec` après l'extension
/// déclarée par cette spec (design §1bis).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CauseGap {
    Crash,
    Kill,
    Sleep,
    SeqBreak,
    Manual,
    Pause,
    Timeout,
}

/// Une ligne de journal.
///
/// `tag = "kind"` : chaque ligne JSONL porte son genre en clair, si bien qu'un
/// journal se relit sans connaitre l'ordre des variantes — y compris par un
/// outil qui n'est pas ce programme, ce qui compte pour un format que
/// l'operateur doit pouvoir inspecter lui-meme.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
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
    /// R2.3 : la photo du conteneur actif, prise sur un déclencheur.
    ///
    /// `Box` : cette variante est bien plus grosse que les autres, et une
    /// énumération se dimensionne sur sa plus grande. Sans la boîte, chaque
    /// entrée de journal — y compris les milliers d'actions — paierait la taille
    /// d'un snapshot.
    Snapshot {
        seq: u64,
        monotone_ms: u64,
        photo: Box<Snapshot>,
    },
    /// R1.3 : la borne oubliée.
    ClotureAuto { seq: u64, monotone_ms: u64 },
    /// R5.4 : des actions ont eu lieu hors des surfaces activées.
    ///
    /// **Combien, et rien d'autre.** Ni le nom de l'application, ni la nature
    /// des actions : la liste blanche existe précisément pour que ce qui se
    /// passe ailleurs ne soit pas observé, et un journal qui nommerait ce qu'il
    /// refuse d'observer aurait observé quand même.
    ///
    /// Une entrée par *plage* contiguë, pas une par action refusée. Dix minutes
    /// passées dans une application non activée produiraient des milliers de
    /// lignes disant chacune la même chose ; elles en produisent une, avec son
    /// décompte.
    ///
    /// Elle est écrite au journal, pas tenue en mémoire : après un crash, un
    /// épisode réassemblé doit encore dire qu'il n'a pas tout vu.
    HorsPerimetre {
        seq: u64,
        monotone_ms: u64,
        combien: u64,
    },
}

impl EntreeJournal {
    pub fn seq(&self) -> u64 {
        match self {
            Self::UiAction { seq, .. }
            | Self::Declencheur { seq, .. }
            | Self::Gap { seq, .. }
            | Self::Snapshot { seq, .. }
            | Self::ClotureAuto { seq, .. }
            | Self::HorsPerimetre { seq, .. } => *seq,
        }
    }

    pub fn monotone_ms(&self) -> u64 {
        match self {
            Self::UiAction { monotone_ms, .. }
            | Self::Declencheur { monotone_ms, .. }
            | Self::Gap { monotone_ms, .. }
            | Self::Snapshot { monotone_ms, .. }
            | Self::ClotureAuto { monotone_ms, .. }
            | Self::HorsPerimetre { monotone_ms, .. } => *monotone_ms,
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

/// Ce qui sait photographier le conteneur actif.
///
/// Le moteur ne connaît ni UIA ni le DOM : il décide QUAND photographier, la
/// source sait COMMENT. Sans cette séparation, la logique des déclencheurs —
/// tout ce que la tâche 2 a rendu testable en temps simulé — redeviendrait
/// dépendante d'un bureau.
/// Ce que rend une demande de photo.
///
/// Trois issues et non deux : « pas de photo » ne dit pas pourquoi, et les deux
/// raisons n'ont pas le même sens. Un bureau qui ne répond pas est un incident
/// technique ; un focus hors périmètre est une **règle qui s'applique**, et la
/// confondre avec une panne empêcherait de savoir si R5.4 tient.
#[derive(Debug)]
pub enum Photo {
    Prise(Box<Noeud>),
    /// R5.4 : le focus n'était pas sur une surface activée.
    ///
    /// Le cas est loin d'être théorique. Le drainage passe jusqu'à une seconde
    /// après le geste, et l'opérateur bascule ; surtout, un `Focus` venu d'une
    /// application non activée est refusé AVANT le `match` de `traiter`, si bien
    /// que `derniere_saisie` n'est jamais remis à zéro : deux secondes plus tard
    /// le déclencheur d'inactivité part et photographie l'application où
    /// l'opérateur se trouve alors. Sans cette issue, jusqu'à 1500 nœuds d'une
    /// messagerie personnelle — rôles, noms accessibles ET valeurs de champs —
    /// entraient au journal.
    HorsPerimetre,
    /// Rien à montrer : bureau muet, délai dépassé, arbre illisible.
    Indisponible,
}

pub trait Snapshotteur: Send + Sync {
    /// La liste blanche voyage avec la demande : c'est le fil UIA, et lui seul,
    /// qui peut savoir sur quoi le focus se trouve à l'instant de la photo.
    fn photographier(&self, autorisees: &crate::surfaces::ListeBlanche) -> Photo;
}

/// Le nom que porte une application non activee, dans le journal.
///
/// Il n'y en a qu'un pour toutes : deux applications non observees ne doivent
/// pas etre distinguables, sinon le journal reconstitue par recoupement ce que
/// la liste blanche lui interdit de nommer.
pub const HORS_PERIMETRE: &str = "hors-perimetre";

pub struct Moteur {
    horloge: std::sync::Arc<dyn Horloge>,
    /// R4.1 : rien n'entre au journal sans etre passe par la.
    redacteur: std::sync::Arc<Redacteur>,
    entrees: Vec<EntreeJournal>,
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
    /// Instant d'entree en pause, tant qu'elle dure (R5.2).
    pause_depuis: Option<u64>,
    /// R5.4 : les seules surfaces sur lesquelles la capture a le droit d'avoir
    /// lieu. Vide par defaut, et c'est le point.
    liste_blanche: crate::surfaces::ListeBlanche,
    /// Combien d'actions refusees depuis la derniere action admise. Vide au
    /// journal des qu'une action admise arrive, ou a la cloture.
    hors_perimetre_en_cours: u64,

    /// Le journal est-il ferme ? Distinct de `clos`.
    ///
    /// `clos` dit que plus rien n'entre. Fermer le fichier — vider le tampon,
    /// `sync_all`, retirer le marqueur `.ouvert` — est un autre geste, et les
    /// confondre coutait un episode entier : la cloture automatique posait
    /// `clos`, puis `clore()` sortait aussitot sur `if self.clos { return; }` et
    /// n'atteignait jamais `Journal::clore()`.
    journal_clos: bool,
    /// Le writer, quand il y en a un. Les tests s'en passent : ils verifient la
    /// logique temporelle, et un disque dans la boucle la rendrait plus lente
    /// sans rien prouver de plus.
    journal: Option<Journal>,
    /// R3.4 : une ecriture qui echoue n'est PAS une perte silencieuse.
    echecs_ecriture: u64,
    /// Ce qui sait photographier, quand quelqu'un sait.
    snapshotteur: Option<std::sync::Arc<dyn Snapshotteur>>,
    /// Combien de photos ont réellement été prises (R2.3).
    snapshots_pris: u64,
    /// Combien ont été refusées parce que le focus avait quitté le périmètre.
    ///
    /// Compté et dit à la clôture. Un refus n'est pas un incident, mais un
    /// épisode dont la moitié des photos manquent ne se lit pas comme un
    /// épisode complet.
    photos_hors_perimetre: u64,
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
            entrees: Vec::new(),
            seq: 0,
            t0,
            clos: false,
            unresolved: 0,
            derniere_saisie: None,
            derniere_action: t0,
            app_courante: application,
            quittee: None,
            veille_depuis: None,
            pause_depuis: None,
            liste_blanche: crate::surfaces::ListeBlanche::vide(),
            hors_perimetre_en_cours: 0,
            journal: None,
            journal_clos: false,
            echecs_ecriture: 0,
            snapshotteur: None,
            snapshots_pris: 0,
            photos_hors_perimetre: 0,
        }
    }

    /// R5.4 — les surfaces sur lesquelles la capture a le droit d'avoir lieu.
    ///
    /// Reglable en cours d'episode : l'operateur qui autorise une application
    /// alors qu'il travaille dedans ne devrait pas avoir a rouvrir l'episode.
    /// Ce qui a ete refuse avant reste refuse — on ne recapture pas le passe.
    pub fn definir_liste_blanche(&mut self, liste: crate::surfaces::ListeBlanche) {
        self.liste_blanche = liste;
    }

    pub fn avec_liste_blanche(mut self, liste: crate::surfaces::ListeBlanche) -> Self {
        self.liste_blanche = liste;
        self
    }

    /// Branche ce qui sait photographier. Sans lui, les déclencheurs se
    /// consignent quand même — ils sont l'information ; le snapshot est le
    /// détail qu'on peut perdre sans perdre la chronologie.
    pub fn avec_snapshotteur(mut self, s: std::sync::Arc<dyn Snapshotteur>) -> Self {
        self.snapshotteur = Some(s);
        self
    }

    /// Branche un writer. Sans lui, le moteur ne fait que tenir un journal en
    /// memoire — ce qui suffit aux tests, jamais a une capture reelle.
    pub fn avec_journal(mut self, journal: Journal) -> Self {
        self.journal = Some(journal);
        self
    }

    /// R3.1 : strictement croissant, par épisode.
    fn prochain_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    /// Tout ce qui entre au journal est daté **depuis l'ouverture de l'épisode**.
    ///
    /// Le moteur raisonne sur l'horloge du processus — c'est elle qui survit à
    /// une veille et qui date les événements de toutes les sources. Mais
    /// l'assemblage reporte ces instants sur l'intervalle mural `[t0, t1]` de
    /// l'épisode, et il documente attendre « un instant monotone depuis
    /// l'ouverture ».
    ///
    /// Les deux ne coïncidaient que dans les tests, qui ouvrent le moteur à
    /// `t0 = 0`. En production, une application lancée depuis dix minutes
    /// donnait des instants supérieurs à la durée de l'épisode : tous les gaps,
    /// la clôture automatique et le hors-périmètre ressortaient horodatés à
    /// `t1`, écrasés par le `min` de l'assemblage.
    fn rebaser(&self, entree: EntreeJournal) -> EntreeJournal {
        let r = |ms: u64| ms.saturating_sub(self.t0);
        match entree {
            EntreeJournal::UiAction {
                seq,
                monotone_ms,
                source,
                genre,
                unresolved,
            } => EntreeJournal::UiAction {
                seq,
                monotone_ms: r(monotone_ms),
                source,
                genre,
                unresolved,
            },
            EntreeJournal::Declencheur {
                seq,
                monotone_ms,
                quoi,
            } => EntreeJournal::Declencheur {
                seq,
                monotone_ms: r(monotone_ms),
                quoi,
            },
            EntreeJournal::Gap {
                seq,
                monotone_ms,
                cause,
                debut_ms,
                fin_ms,
            } => EntreeJournal::Gap {
                seq,
                monotone_ms: r(monotone_ms),
                cause,
                debut_ms: r(debut_ms),
                fin_ms: r(fin_ms),
            },
            EntreeJournal::Snapshot {
                seq,
                monotone_ms,
                photo,
            } => EntreeJournal::Snapshot {
                seq,
                monotone_ms: r(monotone_ms),
                photo,
            },
            EntreeJournal::ClotureAuto { seq, monotone_ms } => EntreeJournal::ClotureAuto {
                seq,
                monotone_ms: r(monotone_ms),
            },
            EntreeJournal::HorsPerimetre {
                seq,
                monotone_ms,
                combien,
            } => EntreeJournal::HorsPerimetre {
                seq,
                monotone_ms: r(monotone_ms),
                combien,
            },
        }
    }

    fn pousser(&mut self, entree: EntreeJournal) {
        let entree = self.rebaser(entree);
        if let Some(j) = self.journal.as_mut() {
            // Un echec d'ecriture se COMPTE. Le ravaler ferait exactement ce que
            // R3.4 interdit : perdre un evenement sans que personne ne le sache.
            if let Err(e) = j.ecrire(&entree) {
                self.echecs_ecriture += 1;
                eprintln!("[noe] ecriture du journal refusee : {e}");
            }
        }
        self.entrees.push(entree);
    }

    fn declencher(&mut self, quoi: Declencheur, monotone_ms: u64) {
        let seq = self.prochain_seq();
        self.pousser(EntreeJournal::Declencheur {
            seq,
            monotone_ms,
            quoi,
        });
        self.photographier(quoi, monotone_ms);
    }

    /// R2.3 : chaque déclencheur persiste un snapshot canonisé.
    ///
    /// La photo est prise APRÈS l'entrée de déclenchement, et porte le même
    /// instant : à la relecture, on voit d'abord pourquoi on a photographié,
    /// puis ce qu'on a vu.
    fn photographier(&mut self, quoi: Declencheur, monotone_ms: u64) {
        let Some(s) = self.snapshotteur.clone() else {
            return;
        };
        let racine = match s.photographier(&self.liste_blanche) {
            Photo::Prise(n) => n,
            Photo::HorsPerimetre => {
                self.photos_hors_perimetre += 1;
                return;
            }
            Photo::Indisponible => return,
        };
        let photo = snapshot::construire(quoi, monotone_ms, &racine, &self.redacteur);
        self.snapshots_pris += 1;
        let seq = self.prochain_seq();
        self.pousser(EntreeJournal::Snapshot {
            seq,
            monotone_ms,
            photo: Box::new(photo),
        });
    }

    /// R2.3 — le cinquième déclencheur.
    ///
    /// Seul un collage APPARIÉ déclenche : un collage venu d'ailleurs est un
    /// événement de l'épisode, pas une preuve que quelque chose de l'épisode a
    /// été réutilisé.
    fn collage(&mut self, apparie: bool, monotone_ms: u64) {
        if apparie {
            self.declencher(Declencheur::CopierColler, monotone_ms);
        }
    }

    pub fn snapshots_pris(&self) -> u64 {
        self.snapshots_pris
    }

    /// R5.4 — combien de photos ont ete refusees hors perimetre.
    pub fn photos_hors_perimetre(&self) -> u64 {
        self.photos_hors_perimetre
    }

    /// R5.2 — l'episode est-il suspendu ?
    ///
    /// Le menu, lui, lit l'etat de la `Session` : c'est elle qui porte la pause
    /// hors episode. Cet accesseur sert au banc, qui verifie la garantie a la
    /// source plutot que sur son reflet.
    #[cfg(test)]
    pub fn en_pause(&self) -> bool {
        self.pause_depuis.is_some()
    }

    /// R5.4 — combien d'actions ont ete refusees, plage en cours comprise.
    pub fn hors_perimetre(&self) -> u64 {
        self.hors_perimetre_en_cours
            + self
                .entrees
                .iter()
                .filter_map(|e| match e {
                    EntreeJournal::HorsPerimetre { combien, .. } => Some(*combien),
                    _ => None,
                })
                .sum::<u64>()
    }

    /// R5.4 — cet événement a-t-il le droit d'entrer ?
    ///
    /// Trois cas, et ils ne se ressemblent pas.
    ///
    /// **La veille et le réveil** sont des faits de la machine, pas d'une
    /// application. Aucune liste blanche ne les gouverne, et les refuser ferait
    /// disparaître les trous de veille que R3.3 exige.
    ///
    /// **La bascule d'application** entre toujours — mais quand elle mène hors
    /// du périmètre, sa destination est remplacée par une constante. Ce que le
    /// journal a le droit de savoir, c'est que l'opérateur a quitté la surface
    /// observée ; ce qu'il n'a pas à savoir, c'est où il est allé. La refuser
    /// tout court coûterait le déclencheur « bascule avec retour » : sans
    /// l'aller, le moteur ne verrait jamais le retour.
    ///
    /// **Tout le reste** est une observation faite SUR une surface. Hors liste,
    /// elle n'a pas lieu — y compris quand la surface n'a pas pu être nommée :
    /// on n'autorise pas ce qu'on n'a pas su identifier.
    fn admissible(&self, ev: &mut RawEvent) -> bool {
        if matches!(ev.genre, GenreEvenement::Veille | GenreEvenement::Reveil) {
            return true;
        }
        if let GenreEvenement::BasculeApplication { vers } = &mut ev.genre {
            if !self.liste_blanche.autorise(Some(vers)) {
                *vers = HORS_PERIMETRE.to_string();
            }
            return true;
        }
        self.liste_blanche.autorise(ev.surface.as_deref())
    }

    /// Declare la plage hors perimetre qui vient de s'achever, s'il y en a une.
    fn vider_hors_perimetre(&mut self, maintenant: u64) {
        let combien = std::mem::take(&mut self.hors_perimetre_en_cours);
        if combien == 0 {
            return;
        }
        let seq = self.prochain_seq();
        self.pousser(EntreeJournal::HorsPerimetre {
            seq,
            monotone_ms: maintenant,
            combien,
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
        // Une pause encore ouverte se termine a la borne, AVANT le trou de
        // timeout : sinon l'episode se clot sur un trou jamais declare, ce que
        // R3.4 refuse au meme titre qu'un crash silencieux.
        if let Some(debut) = self.pause_depuis.take() {
            self.trou(CauseGap::Pause, debut, borne);
        }
        self.vider_hors_perimetre(borne);
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
        // R5.2 : en pause, le temps ne produit rien non plus. Un declencheur
        // d'inactivite pose ici daterait une hesitation qui n'a pas eu lieu —
        // l'operateur n'hesitait pas, il avait suspendu.
        if self.pause_depuis.is_some() {
            return;
        }
        self.verifier_inactivite(maintenant);
    }

    /// Consomme un événement de capture.
    pub fn traiter(&mut self, mut ev: RawEvent) {
        // R1.2 : après clôture, plus rien n'entre. Jamais.
        if self.clos {
            return;
        }
        // R5.2 : pendant la pause, ZERO ecriture. Pas de journal, pas de
        // temporel, pas d'etat interne — l'evenement n'est pas mis de cote pour
        // plus tard, il n'a jamais eu lieu pour cet episode. C'est le sens de
        // « suspendre » ; un moteur qui continuerait a compter en silence
        // rendrait la pause decorative.
        if self.pause_depuis.is_some() {
            return;
        }
        // R5.4 : hors des surfaces activees, rien n'entre. Le refus se compte,
        // il ne se decrit pas.
        if !self.admissible(&mut ev) {
            self.hors_perimetre_en_cours += 1;
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
        // La plage hors perimetre s'achevait a l'instant : elle se declare
        // avant l'action qui y met fin, pour que la chronologie tienne.
        self.vider_hors_perimetre(maintenant);

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
            GenreEvenement::Collage { apparie } => {
                let apparie = *apparie;
                self.collage(apparie, maintenant);
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

        // Une copie et un collage n'ont pas de cible : ce sont des gestes du
        // poste, pas des actions sur un element identifie. Les ecrire quand
        // meme est la lettre de R2.3 — « un collage dont la copie vient
        // d'ailleurs est enregistre `paste{paired:false}` » — et de R2.4 :
        // jamais d'evenement muet.
        //
        // Ils ne l'etaient pas. Le geste ne produisait un declencheur que s'il
        // etait apparie, et rien du tout sinon : on payait le cout vie privee
        // d'une lecture du presse-papiers pour un benefice nul, et le collage
        // non apparie — celui qui dit « cette valeur vient d'ailleurs », donc
        // le plus interessant — disparaissait sans laisser de trou.
        let geste_sans_cible = matches!(
            ev.genre,
            GenreEvenement::Copie | GenreEvenement::Collage { .. }
        );
        if ev.genre.cible().is_some() || geste_sans_cible {
            // « Resolu » se juge sur le nom BRUT : un nom vide le reste apres
            // redaction, et un nom redacte n'est pas un nom perdu. Un geste sans
            // cible n'a rien a resoudre — le compter comme non resolu ferait
            // croire a une capture defaillante.
            let unresolved = ev.genre.cible().is_some_and(|c| !c.resolue());
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
        // Sauter d'une application non observee vers une autre n'est pas un
        // depart : l'operateur etait deja ailleurs, il y reste. Ecraser
        // `quittee` ici perdrait le retour vers la surface observee — un detour
        // par deux applications au lieu d'une suffirait a effacer le
        // declencheur, et c'est un detour tres ordinaire.
        if self.app_courante == HORS_PERIMETRE && vers == HORS_PERIMETRE {
            self.derniere_action = maintenant;
            return;
        }
        if let Some((partie, quitte_a)) = self.quittee.take() {
            if partie == vers && maintenant.saturating_sub(quitte_a) <= RETOUR_MAX_MS {
                self.declencher(Declencheur::BasculeAvecRetour, maintenant);
            }
        }
        let sortante = std::mem::replace(&mut self.app_courante, vers);
        self.quittee = Some((sortante, maintenant));
        self.derniere_action = maintenant;
    }

    /// Clôture de l'épisode : plus rien n'entre, et le fichier se ferme.
    ///
    /// **Réentrante, à dessein.** La clôture automatique de R1.3 pose `clos`
    /// depuis `verifier_timeout`, au milieu d'un battement ; c'est ensuite le
    /// même chemin de clôture que le hotkey qui doit passer, et il doit pouvoir
    /// fermer le journal d'un moteur déjà clos. Sans ça, un épisode d'une heure
    /// laissait son tampon non vidé, `sync_all` jamais appelé et le marqueur
    /// `.ouvert` en place : il ressortait comme un orphelin de crash, et le
    /// travail d'une heure n'existait nulle part.
    pub fn clore(&mut self) {
        if !self.clos {
            // Un délai d'inactivité déjà expiré appartient à l'épisode : le
            // laisser tomber à la clôture perdrait un snapshot que R2.3 exige.
            let maintenant = self.horloge.monotone_ms();
            self.verifier_inactivite(maintenant);
            // Une pause encore ouverte se termine ici. Sans ca, l'episode se
            // clorait sur un trou jamais declare, ce que R3.4 refuse.
            if let Some(debut) = self.pause_depuis.take() {
                self.trou(CauseGap::Pause, debut, maintenant);
            }
            // R5.4 : ce que l'episode n'a pas vu se declare aussi a la fin.
            self.vider_hors_perimetre(maintenant);
            self.clos = true;
        }
        if !self.journal_clos {
            self.journal_clos = true;
            if let Some(j) = self.journal.as_mut() {
                if let Err(e) = j.clore() {
                    self.echecs_ecriture += 1;
                    eprintln!("[noe] cloture du journal refusee : {e}");
                }
            }
        }
    }

    /// R3.3 — la machine a dormi, le detecteur l'a mesure.
    ///
    /// Le trou est ecrit avec les bornes du detecteur, pas avec l'instant
    /// courant : c'est la seule facon de situer la veille entre les deux
    /// evenements qu'elle separe.
    pub fn signaler_veille(&mut self, veille: &crate::veille::Veille) {
        if self.clos {
            return;
        }
        self.trou(CauseGap::Sleep, veille.debut_ms, veille.fin_ms);
        // Au reveil, l'inactivite et la « pause » n'ont plus de sens : la
        // machine etait eteinte, l'operateur n'hesitait pas.
        self.derniere_saisie = None;
        self.derniere_action = veille.fin_ms;
    }

    /// R5.2 — l'operateur suspend la capture.
    ///
    /// Rien n'est ecrit ici : c'est la REPRISE qui produit le trou, parce
    /// qu'avant elle on ne connait pas encore sa borne de fin. Une pause jamais
    /// reprise se termine a la cloture, et `clore` s'en charge.
    pub fn mettre_en_pause(&mut self) {
        if self.clos || self.pause_depuis.is_some() {
            return;
        }
        self.pause_depuis = Some(self.horloge.monotone_ms());
    }

    /// R5.2 — la capture repart : le trou de pause est ecrit maintenant.
    pub fn reprendre(&mut self) {
        let Some(debut) = self.pause_depuis.take() else {
            return;
        };
        if self.clos {
            return;
        }
        let fin = self.horloge.monotone_ms();
        self.trou(CauseGap::Pause, debut, fin);
        // Comme au reveil : le temps de pause n'est pas du temps de travail.
        self.derniere_saisie = None;
        self.derniere_action = fin;
    }

    /// Fait respirer le writer : c'est ce qui honore le vidage a 5 s (R3.1).
    pub fn battre_journal(&mut self) {
        if let Some(j) = self.journal.as_mut() {
            if let Err(e) = j.battre() {
                self.echecs_ecriture += 1;
                eprintln!("[noe] vidage du journal refuse : {e}");
            }
        }
    }

    pub fn journal(&self) -> &[EntreeJournal] {
        &self.entrees
    }

    /// R3.4 : combien d'entrees n'ont pas pu etre ecrites.
    pub fn echecs_ecriture(&self) -> u64 {
        self.echecs_ecriture
    }

    pub fn clos(&self) -> bool {
        self.clos
    }

    /// R2.4 : le compteur de santé.
    pub fn unresolved(&self) -> u64 {
        self.unresolved
    }

    pub fn declencheurs(&self) -> Vec<Declencheur> {
        self.entrees
            .iter()
            .filter_map(|e| match e {
                EntreeJournal::Declencheur { quoi, .. } => Some(*quoi),
                _ => None,
            })
            .collect()
    }

    pub fn gaps(&self) -> Vec<CauseGap> {
        self.entrees
            .iter()
            .filter_map(|e| match e {
                EntreeJournal::Gap { cause, .. } => Some(*cause),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fabrique une cible ordinaire, pour alléger les scénarios.
    fn cible(role: &str, nom: &str) -> crate::source::Cible {
        crate::source::Cible::new(role, nom)
    }
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
        // R5.4 : le banc active ses propres surfaces. `outlook` n'y est
        // volontairement pas — c'est le cas reel, et le scenario de bascule
        // verifie qu'on voit le depart sans savoir ou il mene.
        let mut moteur =
            Moteur::ouvrir(horloge.clone(), redacteur, application).avec_liste_blanche(
                crate::surfaces::ListeBlanche::depuis(["banc.exe", application]),
            );

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
        let mut m = Moteur::ouvrir(horloge.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]));
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 0,
            surface: Some("chrome".into()),
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
        let mut m = Moteur::ouvrir(horloge.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]));
        m.clore();
        let avant = m.journal().len();
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 10,
            surface: Some("chrome".into()),
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
        let mut m = Moteur::ouvrir(horloge.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]));
        horloge.avancer(Duration::from_secs(4_000));
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: horloge.monotone_ms(),
            surface: Some("chrome".into()),
            genre: GenreEvenement::Invocation(cible("button", "Enregistrer")),
        });
        assert!(m.clos(), "R1.3 doit tenir sans battement intermediaire");
        assert_eq!(m.gaps(), vec![CauseGap::Timeout]);
    }

    // ---------------------------------------------------------------------
    // Tache 5 : les gaps systeme (R3.3, R3.4, R5.2).
    // ---------------------------------------------------------------------

    /// Le capteur et le harness doivent nommer les trous pareil.
    ///
    /// S'ils divergent, le capteur ecrit une cause que le schema refuse, et
    /// l'episode part en quarantaine sans que personne comprenne pourquoi. Le
    /// miroir JSON est genere depuis `CAUSES_GAP` et compare ici — meme
    /// dispositif que pour les motifs PII, meme raison.
    #[test]
    fn les_causes_de_gap_sont_les_memes_qu_en_typescript() {
        #[derive(serde::Deserialize)]
        struct Miroir {
            causes: Vec<String>,
        }
        const MIROIR: &str = include_str!("../../../../packages/episode-spec/causes-gap.json");
        let attendu: Miroir = serde_json::from_str(MIROIR).expect("causes-gap.json");

        let toutes = [
            CauseGap::Crash,
            CauseGap::Kill,
            CauseGap::Sleep,
            CauseGap::SeqBreak,
            CauseGap::Manual,
            CauseGap::Pause,
            CauseGap::Timeout,
        ];
        let mut obtenu: Vec<String> = toutes
            .iter()
            .map(|c| {
                serde_json::to_value(c)
                    .expect("serialisable")
                    .as_str()
                    .expect("une chaine")
                    .to_string()
            })
            .collect();
        obtenu.sort();

        let mut attendues = attendu.causes.clone();
        attendues.sort();
        assert_eq!(
            obtenu, attendues,
            "les deux enums ont diverge : un episode capture deviendrait              illisible par le harness"
        );
    }

    #[test]
    fn une_veille_mesuree_produit_un_trou_avec_ses_bornes() {
        let horloge = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(horloge.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]));
        horloge.avancer(Duration::from_secs(300));

        m.signaler_veille(&crate::veille::Veille {
            debut_ms: 60_000,
            fin_ms: 300_000,
            duree_mesuree_ms: 240_000,
        });

        let gap = m
            .journal()
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
        assert_eq!(gap, (60_000, 300_000), "les bornes viennent du detecteur");
    }

    #[test]
    fn apres_une_veille_la_reprise_n_est_pas_une_hesitation() {
        let horloge = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(horloge.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]));
        horloge.avancer(Duration::from_secs(600));

        m.signaler_veille(&crate::veille::Veille {
            debut_ms: 0,
            fin_ms: 600_000,
            duree_mesuree_ms: 600_000,
        });
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 600_100,
            surface: Some("chrome".into()),
            genre: GenreEvenement::Invocation(cible("button", "Enregistrer")),
        });

        assert!(
            !m.declencheurs().contains(&Declencheur::PausePuisAction),
            "dix minutes de veille ne sont pas dix minutes de reflexion"
        );
    }

    #[test]
    fn la_pause_n_ecrit_rien_avant_la_reprise() {
        // Tant que la pause dure, sa borne de fin n'existe pas : ecrire le trou
        // tout de suite obligerait a le corriger apres coup.
        let horloge = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(horloge.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]));
        m.mettre_en_pause();
        horloge.avancer(Duration::from_secs(120));

        assert!(m.gaps().is_empty(), "rien avant la reprise");
    }

    #[test]
    fn la_reprise_ecrit_le_trou_de_pause_avec_ses_bornes() {
        let horloge = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(horloge.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]));
        horloge.avancer(Duration::from_secs(10));
        m.mettre_en_pause();
        horloge.avancer(Duration::from_secs(120));
        m.reprendre();

        let gap = m
            .journal()
            .iter()
            .find_map(|e| match e {
                EntreeJournal::Gap {
                    cause: CauseGap::Pause,
                    debut_ms,
                    fin_ms,
                    ..
                } => Some((*debut_ms, *fin_ms)),
                _ => None,
            })
            .expect("R5.2 : un gap de pause");
        assert_eq!(gap, (10_000, 130_000));
    }

    #[test]
    fn une_pause_jamais_reprise_se_termine_a_la_cloture() {
        // Sinon l'episode se clorait sur un trou jamais declare, ce que R3.4
        // interdit — et la statistique de completude serait fausse a la hausse.
        let horloge = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(horloge.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]));
        horloge.avancer(Duration::from_secs(5));
        m.mettre_en_pause();
        horloge.avancer(Duration::from_secs(60));
        m.clore();

        assert!(m.gaps().contains(&CauseGap::Pause), "obtenu {:?}", m.gaps());
    }

    #[test]
    fn deux_pauses_successives_donnent_deux_trous() {
        let horloge = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(horloge.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]));
        for _ in 0..2 {
            m.mettre_en_pause();
            horloge.avancer(Duration::from_secs(30));
            m.reprendre();
            horloge.avancer(Duration::from_secs(5));
        }
        assert_eq!(
            m.gaps().iter().filter(|c| **c == CauseGap::Pause).count(),
            2
        );
    }

    #[test]
    fn mettre_en_pause_deux_fois_ne_deplace_pas_la_borne() {
        // Un double appui sur « pause » ne doit pas raccourcir le trou : la
        // borne reste celle de la PREMIERE mise en pause.
        let horloge = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(horloge.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]));
        m.mettre_en_pause();
        horloge.avancer(Duration::from_secs(50));
        m.mettre_en_pause();
        horloge.avancer(Duration::from_secs(10));
        m.reprendre();

        let (debut, fin) = m
            .journal()
            .iter()
            .find_map(|e| match e {
                EntreeJournal::Gap {
                    cause: CauseGap::Pause,
                    debut_ms,
                    fin_ms,
                    ..
                } => Some((*debut_ms, *fin_ms)),
                _ => None,
            })
            .expect("un gap de pause");
        assert_eq!((debut, fin), (0, 60_000), "le trou couvre les 60 s");
    }

    #[test]
    fn reprendre_sans_pause_ne_fabrique_pas_de_trou() {
        let horloge = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(horloge.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]));
        m.reprendre();
        assert!(m.gaps().is_empty(), "un trou invente salirait le corpus");
    }

    // ---------------------------------------------------------------------
    // Tache 7 : les snapshots et le cinquieme declencheur (R2.3).
    // ---------------------------------------------------------------------

    /// Un photographe de test : il rend toujours le meme ecran, et compte.
    struct PhotographeFaux {
        prises: std::sync::atomic::AtomicUsize,
        contenu: String,
    }

    impl PhotographeFaux {
        fn new(contenu: &str) -> std::sync::Arc<Self> {
            std::sync::Arc::new(Self {
                prises: std::sync::atomic::AtomicUsize::new(0),
                contenu: contenu.to_string(),
            })
        }
        fn prises(&self) -> usize {
            self.prises.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl Snapshotteur for PhotographeFaux {
        fn photographier(&self, _autorisees: &crate::surfaces::ListeBlanche) -> Photo {
            self.prises
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Photo::Prise(Box::new(Noeud::feuille("document", "Fiche").avec(vec![
                Noeud::feuille("textbox", "Contact").valant(&self.contenu),
            ])))
        }
    }

    /// Un photographe qui refuse : le focus a quitte le perimetre.
    struct PhotographeHorsPerimetre;
    impl Snapshotteur for PhotographeHorsPerimetre {
        fn photographier(&self, _autorisees: &crate::surfaces::ListeBlanche) -> Photo {
            Photo::HorsPerimetre
        }
    }

    /// Un photographe qui n'a rien a montrer — ecran verrouille, fenetre partie.
    struct PhotographeAveugle;
    impl Snapshotteur for PhotographeAveugle {
        fn photographier(&self, _autorisees: &crate::surfaces::ListeBlanche) -> Photo {
            Photo::Indisponible
        }
    }

    fn moteur_avec(
        photo: std::sync::Arc<dyn Snapshotteur>,
    ) -> (Moteur, std::sync::Arc<HorlogeSimulee>) {
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let m = Moteur::ouvrir(h.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]))
            .avec_snapshotteur(photo);
        (m, h)
    }

    fn snapshots(m: &Moteur) -> Vec<&Snapshot> {
        m.journal()
            .iter()
            .filter_map(|e| match e {
                EntreeJournal::Snapshot { photo, .. } => Some(photo.as_ref()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn une_soumission_declenche_un_snapshot() {
        let photo = PhotographeFaux::new("rien de sensible");
        let (mut m, _h) = moteur_avec(photo.clone());
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 100,
            surface: Some("chrome".into()),
            genre: GenreEvenement::Soumission(cible("button", "Enregistrer")),
        });
        assert_eq!(photo.prises(), 1);
        assert_eq!(m.snapshots_pris(), 1);
        assert_eq!(snapshots(&m).len(), 1);
    }

    #[test]
    fn le_snapshot_suit_son_declencheur_dans_le_journal() {
        // A la relecture, on doit voir d'abord POURQUOI on a photographie,
        // puis ce qu'on a vu.
        let (mut m, _h) = moteur_avec(PhotographeFaux::new("x"));
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 10,
            surface: Some("chrome".into()),
            genre: GenreEvenement::Soumission(cible("button", "Enregistrer")),
        });
        let genres: Vec<&str> = m
            .journal()
            .iter()
            .map(|e| match e {
                EntreeJournal::Declencheur { .. } => "declencheur",
                EntreeJournal::Snapshot { .. } => "snapshot",
                EntreeJournal::UiAction { .. } => "action",
                _ => "autre",
            })
            .collect();
        let i = genres.iter().position(|g| *g == "declencheur").unwrap();
        assert_eq!(genres[i + 1], "snapshot", "{genres:?}");
    }

    #[test]
    fn les_cinq_declencheurs_photographient() {
        // R2.3 nomme cinq declencheurs. Aucun ne doit rester sans photo.
        let photo = PhotographeFaux::new("x");
        let (mut m, h) = moteur_avec(photo.clone());

        // 1. soumission
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 0,
            surface: Some("chrome".into()),
            genre: GenreEvenement::Soumission(cible("button", "Enregistrer")),
        });
        // 2. saisie puis 2 s d inactivite
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 1_000,
            surface: Some("chrome".into()),
            genre: GenreEvenement::Saisie(cible("textbox", "Note")),
        });
        h.avancer(Duration::from_secs(3));
        m.battre();
        // 3. bascule avec retour
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 5_000,
            surface: Some("chrome".into()),
            genre: GenreEvenement::BasculeApplication {
                vers: "outlook".into(),
            },
        });
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 10_000,
            surface: Some("chrome".into()),
            genre: GenreEvenement::BasculeApplication {
                vers: "chrome".into(),
            },
        });
        // 4. pause > 10 s puis action
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 30_000,
            surface: Some("chrome".into()),
            genre: GenreEvenement::Invocation(cible("button", "Ouvrir")),
        });
        // 5. copier-coller apparie
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 31_000,
            surface: Some("chrome".into()),
            genre: GenreEvenement::Collage { apparie: true },
        });

        let d = m.declencheurs();
        for attendu in [
            Declencheur::Soumission,
            Declencheur::SaisiePuisInactivite,
            Declencheur::BasculeAvecRetour,
            Declencheur::PausePuisAction,
            Declencheur::CopierColler,
        ] {
            assert!(d.contains(&attendu), "{attendu:?} manque dans {d:?}");
        }
        assert_eq!(
            photo.prises(),
            d.len(),
            "chaque declencheur photographie exactement une fois"
        );
    }

    #[test]
    fn un_collage_non_apparie_ne_declenche_pas() {
        // Un collage venu d'ailleurs est un evenement de l'episode, pas une
        // preuve que quelque chose de l'episode a ete reutilise.
        let photo = PhotographeFaux::new("x");
        let (mut m, _h) = moteur_avec(photo.clone());
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 100,
            surface: Some("chrome".into()),
            genre: GenreEvenement::Collage { apparie: false },
        });
        assert!(!m.declencheurs().contains(&Declencheur::CopierColler));
        assert_eq!(photo.prises(), 0);
    }

    #[test]
    fn sans_photographe_les_declencheurs_se_consignent_quand_meme() {
        // Le declencheur EST l'information ; le snapshot est le detail qu'on
        // peut perdre sans perdre la chronologie.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(h, redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]));
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 0,
            surface: Some("chrome".into()),
            genre: GenreEvenement::Soumission(cible("button", "Enregistrer")),
        });
        assert!(m.declencheurs().contains(&Declencheur::Soumission));
        assert_eq!(m.snapshots_pris(), 0);
    }

    #[test]
    fn un_photographe_aveugle_ne_produit_pas_de_snapshot_vide() {
        // Un snapshot vide ferait croire a un ecran vide, ce qui est faux et
        // pire que pas de snapshot du tout.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(h, redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]))
            .avec_snapshotteur(std::sync::Arc::new(PhotographeAveugle));
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 0,
            surface: Some("chrome".into()),
            genre: GenreEvenement::Soumission(cible("button", "Enregistrer")),
        });
        assert!(m.declencheurs().contains(&Declencheur::Soumission));
        assert_eq!(m.snapshots_pris(), 0);
        assert!(snapshots(&m).is_empty());
    }

    #[test]
    fn le_snapshot_est_redacte_avant_d_entrer_au_journal() {
        // Le report explicite de la tache 3 : R4.5 se prouve ICI, ou les
        // snapshots apparaissent.
        let (mut m, _h) = moteur_avec(PhotographeFaux::new("jean.dupont@exemple.fr"));
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 0,
            surface: Some("chrome".into()),
            genre: GenreEvenement::Soumission(cible("button", "Enregistrer")),
        });
        let serialise = serde_json::to_string(m.journal()).expect("serialisable");
        assert!(
            crate::motifs::chercher(&serialise).is_empty(),
            "R4.5 : PII dans le journal — {serialise}"
        );
        assert!(serialise.contains("EMAIL_"));
    }

    #[test]
    fn les_seq_restent_croissants_avec_les_snapshots() {
        let (mut m, h) = moteur_avec(PhotographeFaux::new("x"));
        for i in 0..5u64 {
            m.traiter(RawEvent {
                source: Source::Fake,
                monotone_ms: i * 100,
                surface: Some("chrome".into()),
                genre: GenreEvenement::Soumission(cible("button", "Enregistrer")),
            });
        }
        h.avancer(Duration::from_secs(1));
        for paire in m.journal().windows(2) {
            assert!(paire[1].seq() > paire[0].seq(), "R3.1");
        }
    }

    #[test]
    fn apres_cloture_aucun_snapshot_n_est_pris() {
        let photo = PhotographeFaux::new("x");
        let (mut m, _h) = moteur_avec(photo.clone());
        m.clore();
        let avant = photo.prises();
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 10,
            surface: Some("chrome".into()),
            genre: GenreEvenement::Soumission(cible("button", "Enregistrer")),
        });
        assert_eq!(photo.prises(), avant, "R1.2 : rien apres la cloture");
    }

    // ---------------------------------------------------------------------
    // Tache 9 : la pause etanche et la liste blanche (R5.2, R5.4).
    // ---------------------------------------------------------------------

    fn moteur_neuf(h: &std::sync::Arc<HorlogeSimulee>) -> Moteur {
        Moteur::ouvrir(h.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]))
    }

    fn depuis(surface: &str, monotone_ms: u64, genre: GenreEvenement) -> RawEvent {
        RawEvent {
            source: Source::Fake,
            monotone_ms,
            surface: Some(surface.to_string()),
            genre,
        }
    }

    /// Tout ce que le journal a ecrit, en une seule chaine.
    ///
    /// Un test qui ne regarderait que les variantes attendues laisserait passer
    /// une fuite logee dans un champ auquel il ne pense pas. Celui-ci balaie le
    /// texte entier.
    fn journal_serialise(m: &Moteur) -> String {
        m.journal()
            .iter()
            .map(|e| serde_json::to_string(e).expect("serialisation"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn pendant_la_pause_zero_ecriture() {
        // R5.2 dans sa forme litterale. Le defaut qu'il ferme est reel : avant
        // la tache 9, `traiter` ne consultait jamais la pause. Le journal
        // ecrivait un trou disant « rien n'a ete capture ici » pendant que les
        // evenements continuaient d'entrer — il mentait.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = moteur_neuf(&h);
        m.traiter(depuis(
            "chrome",
            0,
            GenreEvenement::Focus(cible("tab", "Fiche")),
        ));
        let avant = m.journal().len();
        assert_eq!(avant, 1, "l action d avant-pause est bien entree");

        m.mettre_en_pause();
        assert!(m.en_pause());
        for (i, genre) in [
            GenreEvenement::Focus(cible("textbox", "Note")),
            GenreEvenement::Saisie(cible("textbox", "Note")),
            GenreEvenement::Invocation(cible("button", "Enregistrer")),
            GenreEvenement::Soumission(cible("button", "Envoyer")),
            GenreEvenement::Copie,
            GenreEvenement::Collage { apparie: true },
            GenreEvenement::ChangementValeur(cible("combobox", "Statut")),
            GenreEvenement::ChangementStructure(cible("group", "Panneau")),
        ]
        .into_iter()
        .enumerate()
        {
            h.avancer(Duration::from_secs(3));
            m.traiter(depuis("chrome", (i as u64 + 1) * 3_000, genre));
            m.battre();
            assert_eq!(
                m.journal().len(),
                avant,
                "R5.2 : zero ecriture pendant la pause"
            );
        }
        // Ni declencheur, ni photo, ni compteur : la pause ne met rien de cote
        // pour plus tard non plus.
        assert_eq!(m.declencheurs().len(), 0);
        assert_eq!(m.snapshots_pris(), 0);
        assert_eq!(
            m.hors_perimetre(),
            0,
            "un refus de pause n est pas un refus de perimetre"
        );

        m.reprendre();
        assert!(!m.en_pause());
        assert_eq!(m.gaps(), vec![CauseGap::Pause], "un trou, avec ses bornes");
    }

    #[test]
    fn la_pause_ne_fabrique_pas_de_declencheur_d_inactivite() {
        // Une saisie juste avant la pause, puis vingt secondes de pause : le
        // delai d inactivite expire pendant la suspension. Le consigner
        // daterait une hesitation qui n a pas eu lieu — l operateur n hesitait
        // pas, il avait suspendu.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = moteur_neuf(&h);
        m.traiter(depuis(
            "chrome",
            0,
            GenreEvenement::Saisie(cible("textbox", "Note")),
        ));
        m.mettre_en_pause();
        for _ in 0..10 {
            h.avancer(Duration::from_secs(2));
            m.battre();
        }
        assert!(
            !m.declencheurs()
                .contains(&Declencheur::SaisiePuisInactivite),
            "R5.2 : la pause n est pas une hesitation"
        );
        m.reprendre();
        assert_eq!(m.gaps(), vec![CauseGap::Pause]);
    }

    #[test]
    fn une_pause_jamais_reprise_se_ferme_au_timeout() {
        // R1.3 croise R3.4 : l episode se clot tout seul a soixante minutes,
        // mais il ne peut pas se clore sur un trou jamais declare.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = moteur_neuf(&h);
        m.mettre_en_pause();
        h.avancer(Duration::from_millis(TIMEOUT_MS + 1_000));
        m.battre();
        assert!(m.clos());
        assert_eq!(
            m.gaps(),
            vec![CauseGap::Pause, CauseGap::Timeout],
            "la pause se ferme AVANT le timeout, pas apres"
        );
    }

    #[test]
    fn une_liste_blanche_vide_ne_laisse_rien_entrer() {
        // R5.4 au premier lancement : le moteur tourne, la capture non.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(h.clone(), redacteur(), "chrome");
        for (i, genre) in [
            GenreEvenement::Focus(cible("tab", "Fiche")),
            GenreEvenement::Invocation(cible("button", "Enregistrer")),
            GenreEvenement::Soumission(cible("button", "Envoyer")),
        ]
        .into_iter()
        .enumerate()
        {
            m.traiter(depuis("chrome", i as u64 * 100, genre));
        }
        assert!(
            m.journal().is_empty(),
            "R5.4 : rien n est capture par defaut"
        );
        assert_eq!(m.hors_perimetre(), 3, "mais le refus se compte");
    }

    #[test]
    fn une_surface_non_nommee_est_refusee() {
        // On n autorise pas ce qu on n a pas su identifier : un processus
        // protege ou eleve ne se nomme pas, et le doute ne profite pas a la
        // capture.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = moteur_neuf(&h);
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 0,
            surface: None,
            genre: GenreEvenement::Invocation(cible("button", "Enregistrer")),
        });
        assert!(m.journal().is_empty());
        assert_eq!(m.hors_perimetre(), 1);
    }

    #[test]
    fn le_hors_perimetre_se_compte_par_plage_et_non_par_action() {
        // Dix minutes dans une application non activee produiraient des
        // milliers de lignes disant chacune la meme chose. Elles en produisent
        // une, avec son decompte.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = moteur_neuf(&h);
        for i in 0..3 {
            m.traiter(depuis(
                "keepass.exe",
                i * 100,
                GenreEvenement::Saisie(cible("textbox", "Mot de passe")),
            ));
        }
        m.traiter(depuis(
            "chrome",
            400,
            GenreEvenement::Focus(cible("tab", "Fiche")),
        ));
        for i in 0..2 {
            m.traiter(depuis(
                "keepass.exe",
                500 + i * 100,
                GenreEvenement::Invocation(cible("button", "Copier")),
            ));
        }
        m.clore();

        let plages: Vec<u64> = m
            .journal()
            .iter()
            .filter_map(|e| match e {
                EntreeJournal::HorsPerimetre { combien, .. } => Some(*combien),
                _ => None,
            })
            .collect();
        assert_eq!(plages, vec![3, 2], "une entree par plage contigue");
        assert_eq!(m.hors_perimetre(), 5);
    }

    #[test]
    fn le_journal_ne_nomme_jamais_ce_qu_il_refuse_d_observer() {
        // La liste blanche existe pour que ce qui se passe ailleurs ne soit pas
        // observe. Un journal qui nommerait l application refusee, ou le champ
        // sur lequel on a tape dedans, aurait observe quand meme.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = moteur_neuf(&h);
        m.traiter(depuis(
            "keepass.exe",
            0,
            GenreEvenement::Saisie(cible("textbox", "Coffre Elevay")),
        ));
        m.traiter(depuis(
            "chrome",
            100,
            GenreEvenement::Focus(cible("tab", "Fiche")),
        ));
        m.clore();

        let texte = journal_serialise(&m);
        for interdit in ["keepass", "Coffre", "Elevay", "textbox"] {
            assert!(
                !texte.contains(interdit),
                "« {interdit} » a fuite dans le journal :\n{texte}"
            );
        }
        assert!(
            texte.contains("hors_perimetre"),
            "mais le refus est declare"
        );
    }

    #[test]
    fn la_bascule_hors_perimetre_entre_mais_sans_dire_ou() {
        // Ce que le journal a le droit de savoir : l operateur a quitte la
        // surface observee, et il est revenu. Ce qu il n a pas a savoir : ou il
        // est alle. Refuser la bascule tout court couterait le declencheur
        // « bascule avec retour » — sans l aller, jamais de retour.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = moteur_neuf(&h);
        m.traiter(depuis(
            "chrome",
            0,
            GenreEvenement::Focus(cible("textbox", "Note")),
        ));
        m.traiter(depuis(
            "outlook.exe",
            100,
            GenreEvenement::BasculeApplication {
                vers: "outlook.exe".into(),
            },
        ));
        m.traiter(depuis(
            "chrome",
            12_000,
            GenreEvenement::BasculeApplication {
                vers: "chrome".into(),
            },
        ));
        m.traiter(depuis(
            "chrome",
            12_500,
            GenreEvenement::Invocation(cible("button", "Enregistrer")),
        ));

        assert!(
            m.declencheurs().contains(&Declencheur::BasculeAvecRetour),
            "le declencheur doit survivre a un detour hors perimetre"
        );
        let texte = journal_serialise(&m);
        assert!(
            !texte.contains("outlook"),
            "la destination ne se nomme pas :\n{texte}"
        );
        // Le journal porte le declencheur, jamais la destination : une bascule
        // n'ecrit pas d'action, elle change l'etat du moteur. La constante
        // `HORS_PERIMETRE` protege donc la memoire, pas le disque — et les deux
        // se verifient, parce qu'une spec ulterieure pourrait ecrire l'une a
        // partir de l'autre.
        assert!(
            !texte.contains(HORS_PERIMETRE),
            "pas meme le marqueur : rien de la destination n'est ecrit"
        );
    }

    #[test]
    fn un_detour_par_deux_applications_non_observees_garde_le_retour() {
        // Deux applications non observees deviennent la meme : « ailleurs ».
        // C'est ce qui permet au retour d'etre vu. Si chacune gardait son
        // identite, le second saut ecraserait le souvenir du depart, et un
        // detour par deux applications au lieu d'une effacerait le
        // declencheur — alors que c'est un detour tres ordinaire.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = moteur_neuf(&h);
        let bascule = |vers: &str, ms: u64| {
            depuis(
                vers,
                ms,
                GenreEvenement::BasculeApplication {
                    vers: vers.to_string(),
                },
            )
        };
        m.traiter(depuis(
            "chrome",
            0,
            GenreEvenement::Focus(cible("textbox", "Note")),
        ));
        m.traiter(bascule("keepass.exe", 1_000));
        m.traiter(bascule("signal.exe", 3_000));
        m.traiter(bascule("chrome", 9_000));

        assert!(
            m.declencheurs().contains(&Declencheur::BasculeAvecRetour),
            "parti de chrome a 1 s, revenu a 9 s : le retour doit se voir"
        );
        let texte = journal_serialise(&m);
        for interdit in ["keepass", "signal"] {
            assert!(
                !texte.contains(interdit),
                "« {interdit} » a fuite :\n{texte}"
            );
        }
    }

    #[test]
    fn la_veille_ne_depend_pas_de_la_liste_blanche() {
        // La veille est un fait de la machine, pas d une application. La
        // refuser ferait disparaitre le trou que R3.3 exige — et un trou perdu
        // est exactement ce que la regle 4 interdit.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(h.clone(), redacteur(), "chrome");
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 0,
            surface: None,
            genre: GenreEvenement::Veille,
        });
        m.traiter(RawEvent {
            source: Source::Fake,
            monotone_ms: 60_000,
            surface: None,
            genre: GenreEvenement::Reveil,
        });
        assert_eq!(m.gaps(), vec![CauseGap::Sleep]);
    }

    #[test]
    fn autoriser_en_cours_d_episode_ne_recapture_pas_le_passe() {
        // L operateur qui active une application alors qu il travaille dedans
        // ne devrait pas avoir a rouvrir l episode. Mais ce qui a ete refuse
        // reste refuse : on n invente pas apres coup une observation qu on n a
        // pas faite.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(h.clone(), redacteur(), "chrome");
        m.traiter(depuis(
            "chrome",
            0,
            GenreEvenement::Focus(cible("tab", "Avant")),
        ));
        assert!(m.journal().is_empty());

        m.definir_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]));
        m.traiter(depuis(
            "chrome",
            100,
            GenreEvenement::Focus(cible("tab", "Apres")),
        ));

        let texte = journal_serialise(&m);
        assert!(texte.contains("Apres"), "ce qui suit l activation entre");
        assert!(!texte.contains("Avant"), "ce qui precede reste dehors");
        assert_eq!(m.hors_perimetre(), 1);
    }

    #[test]
    fn la_surface_n_entre_jamais_au_journal() {
        // `surface` sert a decider, pas a raconter. Le journal porte ce que
        // l operateur a fait, pas l inventaire des executables de son poste.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = moteur_neuf(&h);
        m.traiter(depuis(
            "chrome",
            0,
            GenreEvenement::Invocation(cible("button", "Enregistrer")),
        ));
        let texte = journal_serialise(&m);
        assert!(!texte.contains("surface"), "aucun champ surface :\n{texte}");
        assert!(!texte.contains("chrome"), "ni sa valeur :\n{texte}");
    }

    // -- Une seule origine de temps (revue adverse) -------------------------

    #[test]
    fn un_episode_ouvert_tard_date_depuis_son_ouverture() {
        // Tous les scenarios ouvrent le moteur a t0 = 0, ce qui fait coincider
        // l'horloge du processus et celle de l'episode. En production elles ne
        // coincident jamais : l'application tourne depuis des minutes, parfois
        // des heures, quand l'operateur ouvre son premier episode.
        //
        // Le journal doit porter des instants DEPUIS L'OUVERTURE, parce que
        // c'est ce que l'assemblage reporte sur l'intervalle mural [t0, t1] —
        // et qu'il ecrase au-dela par un `min`. Sans rebasage, tous les trous
        // d'un episode ouvert tard ressortaient horodates a t1.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        h.avancer(Duration::from_secs(600));
        let mut m = moteur_neuf(&h);

        m.traiter(depuis(
            "chrome",
            600_100,
            GenreEvenement::Invocation(cible("button", "Enregistrer")),
        ));
        h.avancer(Duration::from_secs(1));
        m.mettre_en_pause();
        h.avancer(Duration::from_secs(30));
        m.reprendre();

        let instants: Vec<u64> = m.journal().iter().map(EntreeJournal::monotone_ms).collect();
        assert!(
            instants.iter().all(|&t| t < 60_000),
            "des instants de processus ont fui dans le journal : {instants:?}"
        );
        assert_eq!(instants[0], 100, "l action est a 100 ms de l ouverture");

        let bornes: Vec<(u64, u64)> = m
            .journal()
            .iter()
            .filter_map(|e| match e {
                EntreeJournal::Gap {
                    debut_ms, fin_ms, ..
                } => Some((*debut_ms, *fin_ms)),
                _ => None,
            })
            .collect();
        assert_eq!(
            bornes,
            vec![(1_000, 31_000)],
            "les bornes du trou de pause aussi se comptent depuis l ouverture"
        );
    }

    #[test]
    fn un_timeout_sur_un_episode_ouvert_tard_est_date_a_soixante_minutes() {
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        h.avancer(Duration::from_secs(3_600));
        let mut m = moteur_neuf(&h);
        h.avancer(Duration::from_millis(TIMEOUT_MS + 500));
        m.battre();

        assert!(m.clos());
        let cloture = m
            .journal()
            .iter()
            .find_map(|e| match e {
                EntreeJournal::ClotureAuto { monotone_ms, .. } => Some(*monotone_ms),
                _ => None,
            })
            .expect("cloture automatique");
        assert_eq!(cloture, TIMEOUT_MS, "la borne est a 60 min de l OUVERTURE");
    }

    #[test]
    fn une_photo_hors_perimetre_est_refusee_et_comptee() {
        // R5.4 gardait les actions et laissait passer les photos. Le chemin est
        // reel : un `Focus` venu d'une application non activee est refuse AVANT
        // le `match` de `traiter`, donc `derniere_saisie` n'est jamais remis a
        // zero ; deux secondes plus tard le declencheur d'inactivite part et
        // photographie l'application ou l'operateur se trouve alors. Jusqu'a
        // 1500 noeuds d'une messagerie personnelle, valeurs de champs comprises.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(h.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]))
            .avec_snapshotteur(std::sync::Arc::new(PhotographeHorsPerimetre));

        m.traiter(depuis(
            "chrome",
            0,
            GenreEvenement::Soumission(cible("button", "Enregistrer")),
        ));

        assert!(
            m.declencheurs().contains(&Declencheur::Soumission),
            "le declencheur reste : c'est lui l'information"
        );
        assert_eq!(m.snapshots_pris(), 0, "aucune photo ne doit entrer");
        assert_eq!(m.photos_hors_perimetre(), 1, "et le refus se compte");
        assert!(
            !m.journal()
                .iter()
                .any(|e| matches!(e, EntreeJournal::Snapshot { .. })),
            "rien de l'application non activee dans le journal"
        );
    }

    #[test]
    fn un_refus_de_photo_ne_se_confond_pas_avec_une_panne() {
        // Deux issues distinctes et non une : un bureau muet est un incident
        // technique, un focus hors perimetre est une regle qui s'applique. Les
        // confondre empecherait de savoir si R5.4 tient.
        let h = std::sync::Arc::new(HorlogeSimulee::new());
        let mut m = Moteur::ouvrir(h.clone(), redacteur(), "chrome")
            .avec_liste_blanche(crate::surfaces::ListeBlanche::depuis(["chrome"]))
            .avec_snapshotteur(std::sync::Arc::new(PhotographeAveugle));
        m.traiter(depuis(
            "chrome",
            0,
            GenreEvenement::Soumission(cible("button", "Enregistrer")),
        ));
        assert_eq!(m.snapshots_pris(), 0);
        assert_eq!(
            m.photos_hors_perimetre(),
            0,
            "un ecran verrouille n'est pas un refus de perimetre"
        );
    }
}
