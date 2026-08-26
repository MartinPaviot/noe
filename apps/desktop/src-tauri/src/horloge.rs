//! L'horloge injectable (spec 002, design §1).
//!
//! Les déclencheurs de la capture sont tous temporels : 2 s d'inactivité après
//! une saisie, retour d'application en moins de 60 s, pause de plus de 10 s,
//! clôture automatique à 60 minutes. Les tester avec des `sleep` réels
//! coûterait une heure par exécution et rendrait la CI non déterministe — ce
//! sont exactement les tests qu'on finit par marquer `ignore`.
//!
//! Deux notions de temps, parce qu'elles ne servent pas à la même chose :
//!
//! - le **monotone** décide des déclencheurs. Il ne recule jamais, même si
//!   l'opérateur change l'heure du poste ou qu'un serveur NTP corrige une
//!   dérive. Un temps mural qui recule ferait apparaître des durées négatives
//!   et des `seq` incohérents.
//! - le **mural** date les bornes de l'épisode, parce qu'un épisode doit
//!   pouvoir être remis en face d'un courriel ou d'un enregistrement CRM.
//!
//! Les confondre est une erreur classique qui ne se voit qu'au changement
//! d'heure ou après une veille prolongée.

use std::time::{SystemTime, UNIX_EPOCH};
// `Duration` ne sert plus qu a l horloge simulee et aux tests.
#[cfg(test)]
use std::time::Duration;

pub trait Horloge: Send + Sync {
    /// Millisecondes depuis une origine arbitraire, strictement non décroissante.
    fn monotone_ms(&self) -> u64;

    /// L'heure murale, pour dater les bornes.
    fn mural(&self) -> SystemTime;

    /// L'heure murale en millisecondes depuis l'epoch Unix.
    ///
    /// Une horloge murale antérieure à l'epoch n'existe pas sur une machine
    /// saine ; si elle se présente, on rend 0 plutôt que de paniquer, parce
    /// qu'une capture en cours ne doit pas mourir d'une pendule déréglée.
    fn mural_ms(&self) -> u64 {
        self.mural()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

pub struct HorlogeReelle {
    debut: std::time::Instant,
}

impl HorlogeReelle {
    pub fn new() -> Self {
        Self {
            debut: std::time::Instant::now(),
        }
    }
}

impl Default for HorlogeReelle {
    fn default() -> Self {
        Self::new()
    }
}

impl Horloge for HorlogeReelle {
    fn monotone_ms(&self) -> u64 {
        self.debut.elapsed().as_millis() as u64
    }

    fn mural(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// L'horloge des tests : elle n'avance que si on le lui demande.
///
/// `cfg(test)` : une doublure n'a rien a faire dans le binaire livre. Si elle y
/// etait, rien n'empecherait un jour de la brancher par megarde en production,
/// et la capture s'arreterait de compter le temps sans que rien n'echoue.
///
/// `Mutex` plutôt qu'`AtomicU64` : le monotone et le mural doivent avancer
/// **ensemble**. Deux atomiques laisseraient une fenêtre où un lecteur voit
/// l'un déjà avancé et l'autre pas, ce qui produirait précisément le genre
/// d'incohérence que cette horloge est censée exclure.
#[cfg(test)]
pub struct HorlogeSimulee {
    etat: std::sync::Mutex<(u64, u64)>,
}

#[cfg(test)]
impl HorlogeSimulee {
    /// Démarre à une date murale fixe et lisible : 2026-01-01T00:00:00Z.
    pub fn new() -> Self {
        Self {
            etat: std::sync::Mutex::new((0, 1_767_225_600_000)),
        }
    }

    pub fn avancer(&self, duree: Duration) {
        let ms = duree.as_millis() as u64;
        let mut e = self.etat.lock().expect("horloge empoisonnee");
        e.0 += ms;
        e.1 += ms;
    }

    /// Fait sauter l'heure MURALE sans toucher au monotone.
    ///
    /// C'est ce qui arrive à une machine qui se resynchronise, ou dont
    /// l'opérateur change le fuseau. Le monotone, lui, ne bouge pas : c'est
    /// précisément la propriété sur laquelle les déclencheurs s'appuient, et
    /// elle mérite un test qui la met en défaut.
    pub fn deregler_mural(&self, ecart_ms: i64) {
        let mut e = self.etat.lock().expect("horloge empoisonnee");
        e.1 = e.1.saturating_add_signed(ecart_ms);
    }
}

#[cfg(test)]
impl Default for HorlogeSimulee {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl Horloge for HorlogeSimulee {
    fn monotone_ms(&self) -> u64 {
        self.etat.lock().expect("horloge empoisonnee").0
    }

    fn mural(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(self.etat.lock().expect("horloge empoisonnee").1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_simulee_n_avance_que_sur_demande() {
        let h = HorlogeSimulee::new();
        assert_eq!(h.monotone_ms(), 0);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(h.monotone_ms(), 0, "le temps reel ne doit rien y faire");

        h.avancer(Duration::from_secs(3600));
        assert_eq!(h.monotone_ms(), 3_600_000);
    }

    #[test]
    fn le_monotone_et_le_mural_avancent_ensemble() {
        let h = HorlogeSimulee::new();
        let m0 = h.mural_ms();
        h.avancer(Duration::from_secs(42));
        assert_eq!(h.monotone_ms(), 42_000);
        assert_eq!(h.mural_ms() - m0, 42_000);
    }

    #[test]
    fn un_mural_deregle_ne_touche_pas_au_monotone() {
        let h = HorlogeSimulee::new();
        h.avancer(Duration::from_secs(10));
        let avant = h.monotone_ms();

        // La machine recule d une heure : changement de fuseau, correction NTP.
        h.deregler_mural(-3_600_000);

        assert_eq!(
            h.monotone_ms(),
            avant,
            "les declencheurs ne doivent RIEN devoir a l heure murale"
        );
        assert!(h.mural_ms() < 1_767_225_600_000);
    }

    #[test]
    fn la_reelle_avance_toute_seule_et_ne_recule_pas() {
        let h = HorlogeReelle::new();
        let a = h.monotone_ms();
        std::thread::sleep(Duration::from_millis(15));
        let b = h.monotone_ms();
        assert!(b >= a, "monotone : {b} apres {a}");
        assert!(b > 0, "15 ms devraient avoir passe");
    }

    #[test]
    fn la_reelle_donne_une_date_plausible() {
        let h = HorlogeReelle::new();
        // Apres 2026-01-01, sinon c est que le mural n est pas branche.
        assert!(h.mural_ms() > 1_767_225_600_000);
    }
}
