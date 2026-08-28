//! L'empreinte et la dégradation ordonnée (spec 002, R7.1 et R7.2).
//!
//! **Un opérateur qui sent la capture la désinstalle.** C'est la user story de
//! R7, et elle a une conséquence qu'on oublie facilement : le budget n'est pas
//! une cible de performance, c'est une condition d'existence du produit.
//!
//! R7.1 fixe les bornes — moins de 5 % de CPU soutenu sur une fenêtre de 30 s,
//! moins de 200 Mo de mémoire. R7.2 dit quoi faire quand elles cèdent : dégrader
//! **dans un ordre écrit**, plutôt que de laisser chauffer.
//!
//! ## Pourquoi trois fenêtres et pas une
//!
//! Une seule fenêtre au-dessus du budget, c'est un pic : un rendu de page lourde,
//! une indexation de Windows qui passe. Dégrader là-dessus ferait perdre des
//! snapshots pour rien, et le produit deviendrait moins bon à chaque hoquet de la
//! machine. Trois fenêtres consécutives — quatre-vingt-dix secondes — c'est une
//! tendance, pas un accident.
//!
//! ## Pourquoi chaque palier s'écrit
//!
//! « Une qualité qui baisse en silence biaise les statistiques en silence. » Un
//! épisode dont les snapshots ont été suspendus n'a pas moins de photos par
//! hasard : il en a moins parce que la machine chauffait. Sans la trace, la spec
//! 004 comparerait des épisodes dégradés à des épisodes complets sans le savoir,
//! et conclurait que la capture est instable.

/// Le budget de R7.1, en pourcentage de CPU d'un cœur.
pub const BUDGET_CPU_PCT: f64 = 5.0;
/// Le budget de R7.1, en octets.
pub const BUDGET_RAM_OCTETS: u64 = 200 * 1024 * 1024;
/// La fenêtre d'observation de R7.1.
pub const FENETRE_MS: u64 = 30_000;
/// Combien de fenêtres consécutives au-dessus avant de dégrader (R7.2).
pub const FENETRES_AVANT_DEGRADATION: u32 = 3;

/// Le débounce nominal du walker, et celui de repli.
///
/// 300 ms vient du spike UIA ; 900 ms est le premier palier de repli. Trois fois
/// moins de photos pour la même durée d'épisode, ce qui est exactement ce qu'on
/// veut quand la machine ne suit plus.
pub const DEBOUNCE_NOMINAL_MS: u64 = 300;
pub const DEBOUNCE_DEGRADE_MS: u64 = 900;

/// Une mesure de fenêtre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mesure {
    /// Pourcentage d'un cœur, moyenné sur la fenêtre.
    pub cpu_pct: f64,
    pub ram_octets: u64,
}

impl Mesure {
    pub fn dans_le_budget(&self) -> bool {
        self.cpu_pct <= BUDGET_CPU_PCT && self.ram_octets <= BUDGET_RAM_OCTETS
    }
}

/// Les paliers, **dans l'ordre où R7.2 les énumère**.
///
/// L'ordre n'est pas arbitraire et il n'est pas négociable. On suspend d'abord ce
/// qui coûte le plus et se perd le moins : un snapshot manquant fait perdre le
/// détail d'un écran, pas la chronologie. On élargit ensuite le débounce, ce qui
/// dégrade la finesse sans rien supprimer. On alerte en dernier, parce qu'une
/// alerte ne libère aucune ressource — elle rend la main à l'opérateur.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Palier {
    /// Rien n'est dégradé.
    #[default]
    Nominal,
    /// R7.2, premier : les snapshots ne sont plus pris.
    SnapshotsSuspendus,
    /// R7.2, deuxième : le débounce passe à `DEBOUNCE_DEGRADE_MS`.
    DebounceElargi,
    /// R7.2, troisième : on prévient, et on ne descend pas plus bas.
    Alerte,
}

impl Palier {
    fn suivant(self) -> Option<Self> {
        match self {
            Self::Nominal => Some(Self::SnapshotsSuspendus),
            Self::SnapshotsSuspendus => Some(Self::DebounceElargi),
            Self::DebounceElargi => Some(Self::Alerte),
            // On ne descend pas plus bas : au-delà, la seule dégradation
            // restante serait d'arrêter de capturer, et ça, c'est une décision
            // de l'opérateur — pas d'un seuil.
            Self::Alerte => None,
        }
    }

    /// Ce que le palier change, tel que l'événement `degraded` le nomme.
    ///
    /// **`None` pour `Nominal`**, et c'est le point. Ce bras fabriquait
    /// `("rien", "nominal", "nominal")` — un événement de dégradation qui dit
    /// que rien n'a été dégradé. Il est inatteignable aujourd'hui, puisque
    /// `suivant()` ne rend jamais `Nominal` ; mais un jour où il le deviendrait,
    /// l'épisode aurait porté une dégradation vide, et la spec 004 aurait
    /// comparé des épisodes « dégradés » à des épisodes complets sans savoir
    /// qu'elle le faisait.
    ///
    /// Le rendre impossible vaut mieux que le rendre inoffensif : ici, si
    /// l'impossible arrive, rien n'est écrit.
    fn transition(self) -> Option<(&'static str, String, String)> {
        Some(match self {
            Self::Nominal => return None,
            Self::SnapshotsSuspendus => ("snapshots", "actifs".into(), "suspendus".into()),
            Self::DebounceElargi => (
                "debounce",
                DEBOUNCE_NOMINAL_MS.to_string(),
                DEBOUNCE_DEGRADE_MS.to_string(),
            ),
            Self::Alerte => ("alerte", "silencieux".into(), "operateur prevenu".into()),
        })
    }
}

/// Une dégradation à écrire au flux — `degraded{what, from, to}`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Degradation {
    pub what: String,
    pub from: String,
    pub to: String,
}

/// Le suiveur d'empreinte : il compte les fenêtres et décide des paliers.
///
/// Séparé de toute mesure réelle, à dessein. Ce qu'il faut vérifier, c'est la
/// règle — trois fenêtres, ordre des paliers, un événement par palier — et une
/// règle qui exigerait un vrai processus qui chauffe pour se tester ne se
/// testerait jamais.
#[derive(Debug, Clone, Default)]
pub struct Empreinte {
    palier: Palier,
    consecutives: u32,
    /// Combien de fenêtres ont été mesurées, tous verdicts confondus.
    fenetres: u64,
    /// Le pire CPU vu, pour le bilan de clôture.
    pire_cpu_pct: f64,
    pire_ram_octets: u64,
}

impl Empreinte {
    pub fn nouvelle() -> Self {
        Self::default()
    }

    pub fn palier(&self) -> Palier {
        self.palier
    }

    /// R7.2 — les snapshots sont-ils encore pris ?
    pub fn snapshots_actifs(&self) -> bool {
        self.palier < Palier::SnapshotsSuspendus
    }

    /// R7.2 — le débounce en vigueur.
    pub fn debounce_ms(&self) -> u64 {
        if self.palier >= Palier::DebounceElargi {
            DEBOUNCE_DEGRADE_MS
        } else {
            DEBOUNCE_NOMINAL_MS
        }
    }

    pub fn fenetres(&self) -> u64 {
        self.fenetres
    }

    pub fn pire_cpu_pct(&self) -> f64 {
        self.pire_cpu_pct
    }

    pub fn pire_ram_octets(&self) -> u64 {
        self.pire_ram_octets
    }

    /// Consomme une fenêtre de mesure et rend la dégradation à écrire, s'il y en
    /// a une.
    ///
    /// **Une seule par appel.** Trois fenêtres au-dessus font descendre d'UN
    /// palier, pas de trois : dégrader en cascade sur une seule tendance
    /// supprimerait tout d'un coup, et on ne saurait plus lequel des trois
    /// paliers a suffi.
    ///
    /// Le compteur repart à zéro à chaque dégradation, et à chaque fenêtre dans
    /// le budget : c'est ce qui fait de « trois consécutives » une tendance et
    /// non un cumul.
    pub fn observer(&mut self, m: Mesure) -> Option<Degradation> {
        self.fenetres += 1;
        self.pire_cpu_pct = self.pire_cpu_pct.max(m.cpu_pct);
        self.pire_ram_octets = self.pire_ram_octets.max(m.ram_octets);

        if m.dans_le_budget() {
            self.consecutives = 0;
            return None;
        }
        self.consecutives += 1;
        if self.consecutives < FENETRES_AVANT_DEGRADATION {
            return None;
        }
        self.consecutives = 0;
        let suivant = self.palier.suivant()?;
        self.palier = suivant;
        let (what, de, vers) = suivant.transition()?;
        Some(Degradation {
            what: what.to_string(),
            from: de,
            to: vers,
        })
    }
}

/// Mesure le processus courant : CPU consommé depuis le dernier appel, mémoire
/// de travail.
///
/// **Le processus courant, pas la machine.** Un opérateur ne désinstalle pas
/// Noe parce que Windows indexe son disque ; il le désinstalle parce que Noe
/// chauffe. Mesurer la charge globale ferait dégrader la capture pour des
/// raisons qui ne la concernent pas — et, pire, laisserait passer une fuite de
/// Noe sur une machine par ailleurs oisive.
#[cfg(not(test))]
#[derive(Debug, Default)]
pub struct Compteur {
    dernier_cpu_100ns: u64,
    dernier_instant_ms: u64,
}

#[cfg(not(test))]
impl Compteur {
    pub fn nouveau() -> Self {
        Self::default()
    }

    /// Rend la mesure de la fenêtre écoulée, ou `None` au tout premier appel.
    ///
    /// Le premier appel n'a rien à comparer : rendre 0 % ferait croire à une
    /// fenêtre calme, ce qui est une affirmation qu'on n'a pas mesurée.
    pub fn fenetre(&mut self, maintenant_ms: u64) -> Option<Mesure> {
        let (cpu_100ns, ram) = lire_processus()?;
        let precedent_cpu = self.dernier_cpu_100ns;
        let precedent_ms = self.dernier_instant_ms;
        self.dernier_cpu_100ns = cpu_100ns;
        self.dernier_instant_ms = maintenant_ms;

        if precedent_ms == 0 {
            return None;
        }
        let ecoule_ms = maintenant_ms.saturating_sub(precedent_ms);
        if ecoule_ms == 0 {
            return None;
        }
        // Les compteurs de Windows sont en centaines de nanosecondes : dix mille
        // par milliseconde.
        let cpu_ms = cpu_100ns.saturating_sub(precedent_cpu) as f64 / 10_000.0;
        Some(Mesure {
            cpu_pct: 100.0 * cpu_ms / ecoule_ms as f64,
            ram_octets: ram,
        })
    }
}

/// Le temps CPU (noyau + utilisateur) et la mémoire de travail du processus.
#[cfg(not(test))]
fn lire_processus() -> Option<(u64, u64)> {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    // SAFETY : la pseudo-poignée du processus courant n'a pas à être refermée,
    // et les trois structures sont des tampons de pile vivants pendant l'appel.
    unsafe {
        let p = GetCurrentProcess();
        let mut creation = Default::default();
        let mut fin = Default::default();
        let mut noyau = Default::default();
        let mut utilisateur = Default::default();
        GetProcessTimes(p, &mut creation, &mut fin, &mut noyau, &mut utilisateur).ok()?;

        let en_100ns = |t: windows::Win32::Foundation::FILETIME| {
            (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
        };
        let cpu = en_100ns(noyau) + en_100ns(utilisateur);

        let mut memoire = PROCESS_MEMORY_COUNTERS::default();
        let taille = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        GetProcessMemoryInfo(p, &mut memoire, taille).ok()?;
        Some((cpu, memoire.WorkingSetSize as u64))
    }
}

/// En test, il n'y a pas de processus à mesurer : c'est la RÈGLE qui se vérifie,
/// et une règle qui exigerait une machine qui chauffe ne se testerait jamais.
#[cfg(test)]
#[derive(Debug, Default)]
pub struct Compteur;

#[cfg(test)]
impl Compteur {
    pub fn nouveau() -> Self {
        Self
    }

    pub fn fenetre(&mut self, _maintenant_ms: u64) -> Option<Mesure> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: Mesure = Mesure {
        cpu_pct: 1.0,
        ram_octets: 40 * 1024 * 1024,
    };
    const CHAUD: Mesure = Mesure {
        cpu_pct: 12.0,
        ram_octets: 40 * 1024 * 1024,
    };
    const GROS: Mesure = Mesure {
        cpu_pct: 1.0,
        ram_octets: 300 * 1024 * 1024,
    };

    #[test]
    fn les_bornes_sont_celles_de_l_exigence() {
        // R7.1 les fixe en chiffres. Les changer doit etre une decision, pas un
        // effet de bord d'un reglage de performance.
        assert_eq!(BUDGET_CPU_PCT, 5.0);
        assert_eq!(BUDGET_RAM_OCTETS, 200 * 1024 * 1024);
        assert_eq!(FENETRE_MS, 30_000);
        assert_eq!(FENETRES_AVANT_DEGRADATION, 3);
    }

    #[test]
    fn une_seule_fenetre_au_dessus_ne_degrade_rien() {
        // Un pic n'est pas une tendance : un rendu de page lourde, une
        // indexation de Windows qui passe. Degrader la-dessus ferait perdre des
        // snapshots pour rien, et le produit deviendrait moins bon a chaque
        // hoquet de la machine.
        let mut e = Empreinte::nouvelle();
        assert_eq!(e.observer(CHAUD), None);
        assert_eq!(e.palier(), Palier::Nominal);
        assert!(e.snapshots_actifs());
    }

    #[test]
    fn deux_fenetres_au_dessus_ne_degradent_pas_non_plus() {
        let mut e = Empreinte::nouvelle();
        assert_eq!(e.observer(CHAUD), None);
        assert_eq!(e.observer(CHAUD), None);
        assert_eq!(e.palier(), Palier::Nominal);
    }

    #[test]
    fn trois_fenetres_consecutives_suspendent_les_snapshots() {
        // R7.2, premier palier : ce qui coute le plus et se perd le moins.
        let mut e = Empreinte::nouvelle();
        e.observer(CHAUD);
        e.observer(CHAUD);
        let d = e.observer(CHAUD).expect("degradation");
        assert_eq!(d.what, "snapshots");
        assert_eq!(d.from, "actifs");
        assert_eq!(d.to, "suspendus");
        assert!(!e.snapshots_actifs());
        assert_eq!(
            e.debounce_ms(),
            DEBOUNCE_NOMINAL_MS,
            "le debounce ne bouge pas encore"
        );
    }

    #[test]
    fn une_fenetre_dans_le_budget_remet_le_compteur_a_zero() {
        // « Trois consecutives » est une tendance, pas un cumul. Sans cette
        // remise a zero, une machine qui chauffe une fois par heure finirait par
        // degrader toute seule au bout de trois heures.
        let mut e = Empreinte::nouvelle();
        e.observer(CHAUD);
        e.observer(CHAUD);
        e.observer(OK);
        assert_eq!(e.observer(CHAUD), None);
        assert_eq!(e.observer(CHAUD), None);
        assert_eq!(e.palier(), Palier::Nominal);
    }

    #[test]
    fn les_paliers_descendent_un_par_un_dans_l_ordre_de_l_exigence() {
        // « Suspendre les snapshots, elargir le debounce, alerter ». L'ordre est
        // ecrit dans R7.2 ; il est fige ici.
        let mut e = Empreinte::nouvelle();
        let mut vus = Vec::new();
        for _ in 0..12 {
            if let Some(d) = e.observer(CHAUD) {
                vus.push(d.what);
            }
        }
        assert_eq!(vus, vec!["snapshots", "debounce", "alerte"]);
        assert_eq!(e.palier(), Palier::Alerte);
        assert!(!e.snapshots_actifs());
        assert_eq!(e.debounce_ms(), DEBOUNCE_DEGRADE_MS);
    }

    #[test]
    fn trois_fenetres_ne_font_descendre_que_d_un_palier() {
        // Degrader en cascade sur une seule tendance supprimerait tout d'un
        // coup, et on ne saurait plus lequel des trois paliers a suffi.
        let mut e = Empreinte::nouvelle();
        e.observer(CHAUD);
        e.observer(CHAUD);
        e.observer(CHAUD);
        assert_eq!(e.palier(), Palier::SnapshotsSuspendus);
    }

    #[test]
    fn on_ne_descend_pas_sous_l_alerte() {
        // Au-dela, la seule degradation restante serait d'arreter de capturer —
        // et ca, c'est une decision de l'operateur, pas d'un seuil.
        let mut e = Empreinte::nouvelle();
        for _ in 0..30 {
            e.observer(CHAUD);
        }
        assert_eq!(e.palier(), Palier::Alerte);
        for _ in 0..30 {
            assert_eq!(e.observer(CHAUD), None, "aucun palier au-dela de l alerte");
        }
    }

    #[test]
    fn la_memoire_declenche_aussi() {
        // R7.1 pose DEUX bornes. Ne surveiller que le CPU laisserait une fuite
        // memoire grandir jusqu'a ce que Windows s'en mele.
        let mut e = Empreinte::nouvelle();
        e.observer(GROS);
        e.observer(GROS);
        assert!(e.observer(GROS).is_some());
    }

    #[test]
    fn le_budget_est_inclusif_a_la_borne() {
        // 5,0 % pile est DANS le budget : une borne exclusive ferait degrader
        // une machine qui tient exactement l'objectif.
        let pile = Mesure {
            cpu_pct: BUDGET_CPU_PCT,
            ram_octets: BUDGET_RAM_OCTETS,
        };
        assert!(pile.dans_le_budget());
    }

    #[test]
    fn le_pire_est_retenu_pour_le_bilan() {
        // L'operateur doit pouvoir savoir a quel point on s'est approche, meme
        // si on n'a jamais degrade.
        let mut e = Empreinte::nouvelle();
        e.observer(OK);
        e.observer(Mesure {
            cpu_pct: 4.2,
            ram_octets: 90 * 1024 * 1024,
        });
        e.observer(OK);
        assert!((e.pire_cpu_pct() - 4.2).abs() < 1e-9);
        assert_eq!(e.pire_ram_octets(), 90 * 1024 * 1024);
        assert_eq!(e.fenetres(), 3);
    }
}
