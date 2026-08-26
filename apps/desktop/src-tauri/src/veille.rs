//! Détection de la mise en veille (spec 002, R3.3).
//!
//! Windows tient deux compteurs qui ne comptent pas la même chose :
//!
//! - `GetTickCount64` — millisecondes depuis le démarrage, **veille comprise** ;
//! - `QueryUnbiasedInterruptTime` — le même temps, **veille exclue**.
//!
//! Leur écart est donc exactement le temps passé suspendu. Pas une heuristique,
//! pas un seuil deviné : la différence de deux compteurs que le système entretient
//! pour cet usage.
//!
//! L'alternative aurait été de guetter `WM_POWERBROADCAST`, qui exige une fenêtre
//! et une boucle de messages — beaucoup de code non testable pour une information
//! que deux appels rendent déjà. Et surtout : une notification manquée est
//! définitivement perdue, alors qu'un écart de compteurs se rattrape au battement
//! suivant, même si le processus a raté l'événement.

/// Ce que le système sait du temps qui passe, sous forme injectable.
pub trait TempsSysteme: Send + Sync {
    /// Millisecondes depuis le démarrage, **veille comprise**.
    fn tick_ms(&self) -> u64;
    /// Millisecondes depuis le démarrage, **veille exclue**.
    fn non_biaise_ms(&self) -> u64;
}

/// En deçà de ce seuil, l'écart n'est que du bruit d'ordonnancement.
///
/// Les deux compteurs n'ont pas la même granularité et ne sont pas lus au même
/// instant ; quelques dizaines de millisecondes d'écart sont normales. Une
/// seconde est très au-dessus de ce bruit et très en dessous de la plus brève
/// veille réelle.
pub const SEUIL_VEILLE_MS: u64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Veille {
    /// Borne de début, dans le repère du moteur. Jamais avant l'ouverture de
    /// l'épisode : un trou ne peut pas commencer avant ce qu'il troue.
    pub debut_ms: u64,
    pub fin_ms: u64,
    /// Ce que les compteurs du système ont réellement mesuré.
    ///
    /// Elle peut DÉPASSER `fin_ms - debut_ms`, et ce n'est pas une incohérence :
    /// un épisode ouvert deux secondes avant une veille de quatre-vingt-dix en
    /// subit quatre-vingt-dix, mais seules deux tombent dans son intervalle. Les
    /// deux chiffres disent des choses différentes et sont tous les deux vrais ;
    /// les confondre ferait apparaître des veilles minuscules là où la machine a
    /// dormi une nuit.
    pub duree_mesuree_ms: u64,
}

impl Veille {
    /// La part de la veille qui tombe DANS l'épisode.
    pub fn duree_dans_episode_ms(&self) -> u64 {
        self.fin_ms.saturating_sub(self.debut_ms)
    }
}

pub struct DetecteurVeille {
    dernier_tick: u64,
    dernier_non_biaise: u64,
}

impl DetecteurVeille {
    pub fn nouveau(temps: &dyn TempsSysteme) -> Self {
        Self {
            dernier_tick: temps.tick_ms(),
            dernier_non_biaise: temps.non_biaise_ms(),
        }
    }

    /// À appeler à chaque battement. Rend la veille qui vient de se terminer.
    ///
    /// `monotone_ms` est l'horloge du moteur, pour que les bornes du trou soient
    /// exprimées dans le même repère que le reste du journal.
    pub fn battre(&mut self, temps: &dyn TempsSysteme, monotone_ms: u64) -> Option<Veille> {
        let tick = temps.tick_ms();
        let non_biaise = temps.non_biaise_ms();

        let ecoule_total = tick.saturating_sub(self.dernier_tick);
        let ecoule_actif = non_biaise.saturating_sub(self.dernier_non_biaise);
        let suspendu = ecoule_total.saturating_sub(ecoule_actif);

        self.dernier_tick = tick;
        self.dernier_non_biaise = non_biaise;

        if suspendu < SEUIL_VEILLE_MS {
            return None;
        }
        // La veille s'est terminée maintenant ; elle a donc commencé `suspendu`
        // millisecondes plus tôt dans le repère du moteur.
        Some(Veille {
            debut_ms: monotone_ms.saturating_sub(suspendu),
            fin_ms: monotone_ms,
            duree_mesuree_ms: suspendu,
        })
    }
}

/// L'implementation reelle. Toujours compilee : c'est `run()` qui l'utilise,
/// et `run()` existe aussi dans le binaire de test.
pub struct TempsWindows;

impl TempsSysteme for TempsWindows {
    fn tick_ms(&self) -> u64 {
        // SAFETY : lecture d'un compteur, sans paramètre ni allocation.
        unsafe { windows::Win32::System::SystemInformation::GetTickCount64() }
    }

    fn non_biaise_ms(&self) -> u64 {
        let mut cent_ns: u64 = 0;
        // SAFETY : le pointeur vise une variable locale vivante pendant l'appel.
        let ok = unsafe {
            windows::Win32::System::WindowsProgramming::QueryUnbiasedInterruptTime(&mut cent_ns)
        };
        if !ok.as_bool() {
            // Rendre le tick en repli annule l'écart, donc ne signale aucune
            // veille. Un faux négatif vaut mieux qu'un faux gap : un trou
            // inventé salirait le corpus sans qu'on puisse le distinguer d'un
            // vrai.
            return self.tick_ms();
        }
        // Le compteur est en centaines de nanosecondes.
        cent_ns / 10_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Deux compteurs qu'on pilote à la main, comme le système les tiendrait.
    struct TempsFaux {
        etat: Mutex<(u64, u64)>,
    }

    impl TempsFaux {
        fn new() -> Self {
            Self {
                etat: Mutex::new((0, 0)),
            }
        }

        /// Le temps passe, machine éveillée : les deux compteurs avancent.
        fn eveille(&self, ms: u64) {
            let mut e = self.etat.lock().unwrap();
            e.0 += ms;
            e.1 += ms;
        }

        /// La machine dort : seul le compteur biaisé avance.
        fn dort(&self, ms: u64) {
            let mut e = self.etat.lock().unwrap();
            e.0 += ms;
        }
    }

    impl TempsSysteme for TempsFaux {
        fn tick_ms(&self) -> u64 {
            self.etat.lock().unwrap().0
        }
        fn non_biaise_ms(&self) -> u64 {
            self.etat.lock().unwrap().1
        }
    }

    #[test]
    fn une_machine_eveillee_ne_produit_aucune_veille() {
        let t = TempsFaux::new();
        let mut d = DetecteurVeille::nouveau(&t);
        for i in 1..=10 {
            t.eveille(1_000);
            assert_eq!(d.battre(&t, i * 1_000), None, "battement {i}");
        }
    }

    #[test]
    fn une_veille_est_detectee_avec_sa_duree_exacte() {
        let t = TempsFaux::new();
        let mut d = DetecteurVeille::nouveau(&t);

        t.eveille(1_000);
        assert_eq!(d.battre(&t, 1_000), None);

        // 90 secondes de veille, puis le battement qui suit le reveil.
        t.dort(90_000);
        t.eveille(1_000);
        let v = d.battre(&t, 2_000).expect("R3.3 : la veille doit etre vue");
        assert_eq!(
            v.duree_mesuree_ms, 90_000,
            "la duree vient de l ecart des compteurs"
        );
        assert_eq!(v.fin_ms, 2_000, "elle se termine au battement courant");
        assert_eq!(
            v.debut_ms, 0,
            "l episode n avait que 2 s : le trou ne peut pas commencer avant lui"
        );
        assert_eq!(
            v.duree_dans_episode_ms(),
            2_000,
            "seules 2 s de la veille tombent dans cet episode"
        );
    }

    #[test]
    fn le_bruit_d_ordonnancement_ne_declenche_pas() {
        // Les deux compteurs ne sont pas lus au meme instant : quelques
        // millisecondes d ecart sont normales et ne sont pas une veille.
        let t = TempsFaux::new();
        let mut d = DetecteurVeille::nouveau(&t);
        t.eveille(1_000);
        t.dort(SEUIL_VEILLE_MS - 1);
        assert_eq!(d.battre(&t, 1_000), None, "sous le seuil : rien");
    }

    #[test]
    fn le_seuil_exact_declenche() {
        let t = TempsFaux::new();
        let mut d = DetecteurVeille::nouveau(&t);
        t.eveille(1_000);
        t.dort(SEUIL_VEILLE_MS);
        assert!(d.battre(&t, 1_000).is_some(), "au seuil : detecte");
    }

    #[test]
    fn deux_veilles_successives_sont_vues_toutes_les_deux() {
        // L etat est remis a chaque battement : une premiere veille ne doit pas
        // masquer la seconde, ni etre recomptee.
        let t = TempsFaux::new();
        let mut d = DetecteurVeille::nouveau(&t);

        t.dort(30_000);
        t.eveille(1_000);
        assert_eq!(
            d.battre(&t, 100_000).map(|v| v.duree_mesuree_ms),
            Some(30_000)
        );

        t.eveille(1_000);
        assert_eq!(d.battre(&t, 101_000), None, "pas de veille recomptee");

        t.dort(45_000);
        t.eveille(1_000);
        assert_eq!(
            d.battre(&t, 102_000).map(|v| v.duree_mesuree_ms),
            Some(45_000)
        );
    }

    #[test]
    fn les_bornes_sont_dans_le_repere_du_moteur() {
        // Le trou doit s exprimer dans la meme horloge que le reste du journal,
        // sinon on ne peut pas le situer entre deux evenements.
        let t = TempsFaux::new();
        let mut d = DetecteurVeille::nouveau(&t);
        t.dort(20_000);
        t.eveille(500);
        let v = d.battre(&t, 123_456).unwrap();
        assert_eq!(v.fin_ms, 123_456);
        assert_eq!(v.debut_ms, 123_456 - 20_000);
    }
}
