//! Le client commun : reprises, budget, et rien qui mente (spec 003, R5).
//!
//! **Miroir de `packages/core/src/ports/client.ts`.** Les constantes, la formule
//! de délai et l'ordre des décisions sont les mêmes des deux côtés. Deux clients
//! qui reprendraient différemment produiraient deux corpus incomparables, et
//! l'écart ne se verrait qu'au moment où on essaierait de les additionner.
//!
//! ## Ce que R5.1 exige, et que l'appel direct ne donnait pas
//!
//! « TOUTE requête DOIT passer par le client commun. » Jusqu'ici, l'adaptateur
//! appelait le transport une fois : pas de reprise, pas de `Retry-After`, pas de
//! rafraîchissement sur 401. Un `429` devenait un trou définitif, et une API qui
//! demandait simplement d'attendre trois secondes coûtait une entité.
//!
//! ## L'ordre des décisions, et pourquoi il est celui-là
//!
//! 1. **Le budget se prend AVANT l'appel**, y compris pour une reprise. Une
//!    tentative coûte un appel au quota distant, qu'elle réussisse ou non ; le
//!    compter après ne protégerait de rien.
//! 2. **Une permission ne se réessaie jamais.** Réessayer ne changera pas les
//!    droits, et marteler une API qui vient de dire non ressemble à une attaque.
//! 3. **Un seul rafraîchissement sur 401.** Deux 401 après un refresh réussi
//!    veulent dire autre chose qu'un jeton expiré, et boucler dessus brûlerait le
//!    budget sur un problème que le refresh ne résout pas.
//! 4. **`Retry-After` gagne sur notre calcul.** Le serveur sait quand il sera
//!    prêt ; nous devinons. L'ignorer est la façon la plus courante de
//!    transformer une limitation en bannissement. Mais il reste **borné** : un
//!    `Retry-After` de dix minutes bloquerait la clôture, que R3.2 borne à
//!    soixante secondes.

#![allow(dead_code)] // retiré quand la tâche 0 permet de brancher un vrai transport

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::transport::{ReponseHttp, Transport};

/// R5.1 — au-delà, on déclare un trou plutôt que de marteler.
pub const TENTATIVES_MAX: u32 = 5;

/// Le premier délai d'attente.
pub const DELAI_BASE_MS: u64 = 250;

/// Le plafond d'une attente, la nôtre comme celle du serveur.
pub const DELAI_MAX_MS: u64 = 8_000;

/// Le délai avant la tentative `n`, avec **jitter**.
///
/// Le jitter n'est pas une coquetterie. Sans lui, tous les clients qui prennent
/// un `429` au même instant reviennent au même instant : la deuxième vague est
/// aussi serrée que la première, et l'API reste en colère. Le hasard les étale.
///
/// `alea` est injecté pour que le banc soit déterministe — un test qui
/// vérifierait un délai tiré au hasard vérifierait le hasard.
pub fn delai_ms(tentative: u32, alea: f64) -> u64 {
    let exposant = tentative.saturating_sub(1).min(32);
    let exponentiel = DELAI_BASE_MS
        .saturating_mul(1_u64 << exposant)
        .min(DELAI_MAX_MS);
    // Jitter « full » : entre la moitié et la totalité. Garder un plancher évite
    // qu'un tirage malheureux ne rappelle immédiatement.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let attente = (exponentiel as f64 * (0.5 + 0.5 * alea.clamp(0.0, 1.0))) as u64;
    attente
}

/// Un tirage dans `[0, 1)`.
///
/// En cas d'échec du générateur, on rend `0.5` : pas de jitter est moins bon
/// qu'un demi-jitter, et les deux valent mieux qu'une panique dans un chemin de
/// reprise — c'est-à-dire au pire moment.
pub fn alea_systeme() -> f64 {
    use ring::rand::SecureRandom;
    let mut octets = [0_u8; 8];
    if ring::rand::SystemRandom::new().fill(&mut octets).is_err() {
        return 0.5;
    }
    #[allow(clippy::cast_precision_loss)]
    let tirage = (u64::from_le_bytes(octets) >> 11) as f64 / (1_u64 << 53) as f64;
    tirage
}

/// Le compteur d'appels d'un épisode (R5.3).
///
/// Il est **partagé**, pas tenu par un client : un budget par client serait un
/// budget par connecteur, et deux connecteurs se partageraient le double. Le
/// budget appartient à l'épisode.
#[derive(Debug)]
pub struct Budget {
    plafond: u32,
    consommes: AtomicU32,
}

impl Budget {
    pub fn nouveau(plafond: u32) -> Self {
        Self {
            plafond,
            consommes: AtomicU32::new(0),
        }
    }

    /// Prend un appel. Rend `false` quand il n'en reste plus.
    pub fn prendre(&self) -> bool {
        // `fetch_update` et pas un `load` suivi d'un `store` : deux fils qui
        // liraient « il en reste un » en même temps en prendraient deux.
        self.consommes
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |c| {
                (c < self.plafond).then_some(c + 1)
            })
            .is_ok()
    }

    pub fn reste(&self) -> u32 {
        self.plafond
            .saturating_sub(self.consommes.load(Ordering::SeqCst))
    }

    pub fn plafond(&self) -> u32 {
        self.plafond
    }

    pub fn consommes(&self) -> u32 {
        self.consommes.load(Ordering::SeqCst)
    }
}

/// Ce qu'une réponse veut dire pour la reprise.
///
/// La classification est **propre à chaque connecteur** : le même `403` veut dire
/// « ralentis » chez Google et « tu n'as pas le droit » ailleurs, et ces deux-là
/// appellent des gestes opposés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classe {
    /// La réponse est exploitable telle quelle.
    Finale,
    /// Le jeton est refusé : un rafraîchissement peut sauver l'appel.
    NonAutorise,
    /// Réessayer a un sens.
    Reessayable,
}

/// Comment un connecteur classe ses réponses.
pub type Classificateur = fn(u16, &str) -> Classe;

/// La classification HTTP ordinaire, pour un connecteur qui n'a rien de spécial.
pub fn classe_http(statut: u16, _corps: &str) -> Classe {
    match statut {
        401 => Classe::NonAutorise,
        408 | 429 | 500..=599 => Classe::Reessayable,
        _ => Classe::Finale,
    }
}

/// Le client commun (R5.1).
///
/// Il implémente `Transport` : l'adaptateur ne sait pas qu'il est là, et c'est
/// exactement ce qu'on veut — une reprise qu'un adaptateur peut oublier de
/// demander est une reprise qui n'aura pas lieu.
pub struct ClientRobuste<T: Transport> {
    transport: T,
    classer: Classificateur,
    budget: std::sync::Arc<Budget>,
    tentatives_max: u32,
    /// Rend `true` si le rafraîchissement a réussi.
    rafraichir: Option<Box<dyn Fn() -> bool + Send + Sync>>,
    dormir: Box<dyn Fn(Duration) + Send + Sync>,
    alea: Box<dyn Fn() -> f64 + Send + Sync>,
}

impl<T: Transport> ClientRobuste<T> {
    pub fn nouveau(transport: T, classer: Classificateur, budget: std::sync::Arc<Budget>) -> Self {
        Self {
            transport,
            classer,
            budget,
            tentatives_max: TENTATIVES_MAX,
            rafraichir: None,
            dormir: Box::new(std::thread::sleep),
            alea: Box::new(alea_systeme),
        }
    }

    /// Branche le rafraîchissement de jeton, utilisé **une fois** par appel.
    #[must_use]
    pub fn avec_rafraichissement(
        mut self,
        rafraichir: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Self {
        self.rafraichir = Some(Box::new(rafraichir));
        self
    }

    /// Remplace l'attente et le hasard. Pour les bancs : attendre vraiment huit
    /// secondes rendrait la suite assez pénible pour qu'on cesse de la lancer.
    #[must_use]
    pub fn avec_banc(
        mut self,
        dormir: impl Fn(Duration) + Send + Sync + 'static,
        alea: impl Fn() -> f64 + Send + Sync + 'static,
    ) -> Self {
        self.dormir = Box::new(dormir);
        self.alea = Box::new(alea);
        self
    }

    #[must_use]
    pub fn avec_tentatives(mut self, tentatives_max: u32) -> Self {
        self.tentatives_max = tentatives_max;
        self
    }
}

impl<T: Transport> Transport for ClientRobuste<T> {
    fn get(&self, chemin: &str) -> Result<ReponseHttp, String> {
        let mut rafraichissement_tente = false;
        let mut derniere = String::new();

        for tentative in 1..=self.tentatives_max {
            // R5.3 : le budget se prend AVANT l'appel, reprise comprise.
            if !self.budget.prendre() {
                return Err(format!(
                    "budget d appels epuise ({})",
                    self.budget.plafond()
                ));
            }

            let reponse = match self.transport.get(chemin) {
                Ok(r) => r,
                Err(e) => {
                    // Une panne de transport est reprenable : un câble qui
                    // bouge n'est pas un refus.
                    derniere = format!("transport : {e}");
                    if tentative >= self.tentatives_max {
                        break;
                    }
                    (self.dormir)(Duration::from_millis(delai_ms(tentative, (self.alea)())));
                    continue;
                }
            };

            match (self.classer)(reponse.statut, &reponse.corps) {
                // Une permission, un `404`, un succès : c'est une réponse, et
                // c'est à l'adaptateur d'en tirer les conséquences.
                Classe::Finale => return Ok(reponse),

                Classe::NonAutorise => {
                    let Some(rafraichir) = self.rafraichir.as_ref().filter(|_| {
                        // **Un seul rafraîchissement.**
                        !rafraichissement_tente
                    }) else {
                        return Ok(reponse);
                    };
                    rafraichissement_tente = true;
                    if !rafraichir() {
                        return Ok(reponse);
                    }
                    // On rejoue tout de suite, sans attendre : le jeton est neuf.
                    continue;
                }

                Classe::Reessayable => {
                    derniere = format!("{} : {}", reponse.statut, apercu(&reponse.corps));
                    if tentative >= self.tentatives_max {
                        break;
                    }
                    // `Retry-After` gagne, mais reste borné.
                    let attente = reponse
                        .retry_after_ms
                        .unwrap_or_else(|| delai_ms(tentative, (self.alea)()));
                    (self.dormir)(Duration::from_millis(attente.min(DELAI_MAX_MS)));
                }
            }
        }

        Err(format!(
            "reprises epuisees apres {} tentatives : {derniere}",
            self.tentatives_max
        ))
    }
}

/// Un début de corps, pour une cause lisible sans recopier la réponse entière.
fn apercu(corps: &str) -> String {
    corps.chars().take(120).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};

    /// Un transport qui rend ce qu'on lui a dit, tour par tour.
    struct Faux {
        reponses: Vec<Result<ReponseHttp, String>>,
        appels: AtomicUsize,
    }

    impl Faux {
        fn nouveau(reponses: Vec<Result<ReponseHttp, String>>) -> Arc<Self> {
            Arc::new(Self {
                reponses,
                appels: AtomicUsize::new(0),
            })
        }
        fn appels(&self) -> usize {
            self.appels.load(Ordering::SeqCst)
        }
    }

    impl Transport for Arc<Faux> {
        fn get(&self, _chemin: &str) -> Result<ReponseHttp, String> {
            let i = self.appels.fetch_add(1, Ordering::SeqCst);
            self.reponses
                .get(i)
                .cloned()
                .unwrap_or_else(|| Ok(ReponseHttp::simple(200, "{}")))
        }
    }

    /// Un dormeur qui note au lieu d'attendre.
    fn banc() -> (
        Arc<Mutex<Vec<u64>>>,
        impl Fn(Duration) + Send + Sync + 'static,
    ) {
        let vu = Arc::new(Mutex::new(Vec::new()));
        let copie = vu.clone();
        #[allow(clippy::cast_possible_truncation)]
        let dormir = move |d: Duration| copie.lock().unwrap().push(d.as_millis() as u64);
        (vu, dormir)
    }

    fn client(
        faux: &Arc<Faux>,
        budget: &Arc<Budget>,
    ) -> (ClientRobuste<Arc<Faux>>, Arc<Mutex<Vec<u64>>>) {
        let (vu, dormir) = banc();
        let c = ClientRobuste::nouveau(faux.clone(), classe_http, budget.clone())
            .avec_banc(dormir, || 1.0);
        (c, vu)
    }

    fn budget() -> Arc<Budget> {
        Arc::new(Budget::nouveau(30))
    }

    // -- Le delai ----------------------------------------------------------

    #[test]
    fn le_delai_est_le_meme_que_celui_du_client_typescript() {
        // Miroir exact de `delaiMs`. Deux clients qui reprendraient differemment
        // produiraient deux corpus incomparables, et l'ecart ne se verrait qu'au
        // moment ou on essaierait de les additionner.
        assert_eq!(delai_ms(1, 1.0), 250);
        assert_eq!(delai_ms(2, 1.0), 500);
        assert_eq!(delai_ms(3, 1.0), 1_000);
        assert_eq!(delai_ms(1, 0.0), 125);
        assert_eq!(delai_ms(2, 0.0), 250);
    }

    #[test]
    fn le_delai_reste_borne_meme_tres_loin() {
        // Sans plafond, la dixieme tentative attendrait deux minutes, et la
        // cloture que R3.2 borne a soixante secondes ne fermerait jamais.
        assert_eq!(delai_ms(20, 1.0), DELAI_MAX_MS);
        assert_eq!(delai_ms(u32::MAX, 1.0), DELAI_MAX_MS);
    }

    #[test]
    fn le_jitter_etale_les_reprises() {
        // Sans lui, tous les clients qui prennent un 429 au meme instant
        // reviennent au meme instant : la deuxieme vague est aussi serree.
        let tirages: Vec<u64> = (0..40).map(|_| delai_ms(3, alea_systeme())).collect();
        let distincts: std::collections::BTreeSet<u64> = tirages.iter().copied().collect();
        assert!(
            distincts.len() > 2,
            "le hasard n etale rien : {distincts:?}"
        );
        for d in tirages {
            assert!((500..=1_000).contains(&d), "{d} hors de la moitie-totalite");
        }
    }

    // -- Le budget ---------------------------------------------------------

    #[test]
    fn le_budget_se_prend_avant_l_appel() {
        // Le compter apres ne protegerait de rien : l'appel serait deja parti.
        let faux = Faux::nouveau(vec![]);
        let b = Arc::new(Budget::nouveau(0));
        let (c, _) = client(&faux, &b);
        let e = c.get("/x").unwrap_err();
        assert!(e.contains("budget"), "{e}");
        assert_eq!(faux.appels(), 0, "un appel est parti sans budget");
    }

    #[test]
    fn le_budget_compte_aussi_les_tentatives_qui_echouent() {
        // C'est precisement quand ca echoue qu'on martele. Un budget qui ne
        // compterait que les succes ne bornerait rien du tout.
        let faux = Faux::nouveau(vec![
            Ok(ReponseHttp::simple(429, "{}")),
            Ok(ReponseHttp::simple(429, "{}")),
            Ok(ReponseHttp::simple(200, "{}")),
        ]);
        let b = budget();
        let (c, _) = client(&faux, &b);
        assert!(c.get("/x").is_ok());
        assert_eq!(b.consommes(), 3, "seuls les succes ont ete comptes");
    }

    #[test]
    fn un_budget_partage_ne_se_double_pas_entre_deux_clients() {
        // Un budget par client serait un budget par connecteur, et deux
        // connecteurs se partageraient le double.
        let b = Arc::new(Budget::nouveau(2));
        let a = Faux::nouveau(vec![]);
        let z = Faux::nouveau(vec![]);
        let (ca, _) = client(&a, &b);
        let (cz, _) = client(&z, &b);
        assert!(ca.get("/x").is_ok());
        assert!(cz.get("/y").is_ok());
        assert!(ca.get("/x").is_err(), "le budget commun n a pas mordu");
    }

    // -- Les reprises ------------------------------------------------------

    #[test]
    fn un_succes_ne_coute_qu_un_appel() {
        let faux = Faux::nouveau(vec![Ok(ReponseHttp::simple(200, "ok"))]);
        let b = budget();
        let (c, attentes) = client(&faux, &b);
        assert_eq!(c.get("/x").unwrap().corps, "ok");
        assert_eq!(faux.appels(), 1);
        assert!(attentes.lock().unwrap().is_empty(), "il a dormi pour rien");
    }

    #[test]
    fn une_permission_n_est_jamais_reessayee() {
        // Reessayer ne changera pas les droits, et marteler une API qui vient de
        // dire non ressemble a une attaque.
        let faux = Faux::nouveau(vec![Ok(ReponseHttp::simple(403, "refuse"))]);
        let b = budget();
        let (c, _) = client(&faux, &b);
        assert_eq!(c.get("/x").unwrap().statut, 403);
        assert_eq!(faux.appels(), 1);
    }

    #[test]
    fn un_introuvable_n_est_jamais_reessaye() {
        let faux = Faux::nouveau(vec![Ok(ReponseHttp::simple(404, "{}"))]);
        let b = budget();
        let (c, _) = client(&faux, &b);
        assert_eq!(c.get("/x").unwrap().statut, 404);
        assert_eq!(faux.appels(), 1);
    }

    #[test]
    fn une_rafale_de_429_finit_par_un_trou_qui_dit_son_compte() {
        let faux = Faux::nouveau(vec![Ok(ReponseHttp::simple(429, "trop vite")); 9]);
        let b = budget();
        let (c, attentes) = client(&faux, &b);
        let e = c.get("/x").unwrap_err();
        assert!(e.contains("5 tentatives"), "{e}");
        assert!(e.contains("429"), "{e}");
        assert_eq!(faux.appels(), 5, "au-dela, c'est du martelage");
        // Quatre attentes pour cinq tentatives : on ne dort pas apres la
        // derniere, ce serait attendre pour rien avant d'abandonner.
        assert_eq!(attentes.lock().unwrap().len(), 4);
    }

    #[test]
    fn une_panne_de_transport_est_reprenable() {
        // Un cable qui bouge n'est pas un refus.
        let faux = Faux::nouveau(vec![
            Err("connexion refusee".into()),
            Ok(ReponseHttp::simple(200, "enfin")),
        ]);
        let b = budget();
        let (c, _) = client(&faux, &b);
        assert_eq!(c.get("/x").unwrap().corps, "enfin");
        assert_eq!(faux.appels(), 2);
    }

    #[test]
    fn retry_after_gagne_sur_notre_calcul() {
        // Le serveur sait quand il sera pret ; nous devinons. L'ignorer est la
        // facon la plus courante de transformer une limitation en bannissement.
        let faux = Faux::nouveau(vec![
            Ok(ReponseHttp {
                statut: 429,
                corps: "{}".into(),
                retry_after_ms: Some(3_000),
            }),
            Ok(ReponseHttp::simple(200, "ok")),
        ]);
        let b = budget();
        let (c, attentes) = client(&faux, &b);
        assert!(c.get("/x").is_ok());
        assert_eq!(*attentes.lock().unwrap(), vec![3_000]);
    }

    #[test]
    fn un_retry_after_delirant_reste_borne() {
        // Dix minutes bloqueraient la cloture, que R3.2 borne a soixante
        // secondes. On respecte l'intention, pas le chiffre.
        let faux = Faux::nouveau(vec![
            Ok(ReponseHttp {
                statut: 503,
                corps: "{}".into(),
                retry_after_ms: Some(600_000),
            }),
            Ok(ReponseHttp::simple(200, "ok")),
        ]);
        let b = budget();
        let (c, attentes) = client(&faux, &b);
        assert!(c.get("/x").is_ok());
        assert_eq!(*attentes.lock().unwrap(), vec![DELAI_MAX_MS]);
    }

    // -- Le rafraichissement -----------------------------------------------

    #[test]
    fn un_401_declenche_un_rafraichissement_et_rejoue_sans_attendre() {
        let faux = Faux::nouveau(vec![
            Ok(ReponseHttp::simple(401, "expire")),
            Ok(ReponseHttp::simple(200, "avec le neuf")),
        ]);
        let b = budget();
        let compteur = Arc::new(AtomicUsize::new(0));
        let vu = compteur.clone();
        let (dormir_vu, dormir) = banc();
        let c = ClientRobuste::nouveau(faux.clone(), classe_http, b)
            .avec_banc(dormir, || 1.0)
            .avec_rafraichissement(move || {
                vu.fetch_add(1, Ordering::SeqCst);
                true
            });
        assert_eq!(c.get("/x").unwrap().corps, "avec le neuf");
        assert_eq!(compteur.load(Ordering::SeqCst), 1);
        // Le jeton est neuf : attendre serait attendre pour rien.
        assert!(dormir_vu.lock().unwrap().is_empty());
    }

    #[test]
    fn deux_401_ne_declenchent_qu_un_seul_rafraichissement() {
        // Deux 401 apres un refresh reussi veulent dire autre chose qu'un jeton
        // expire, et boucler dessus brulerait le budget sur un probleme que le
        // refresh ne resout pas.
        let faux = Faux::nouveau(vec![Ok(ReponseHttp::simple(401, "non")); 5]);
        let b = budget();
        let compteur = Arc::new(AtomicUsize::new(0));
        let vu = compteur.clone();
        let (_, dormir) = banc();
        let c = ClientRobuste::nouveau(faux.clone(), classe_http, b)
            .avec_banc(dormir, || 1.0)
            .avec_rafraichissement(move || {
                vu.fetch_add(1, Ordering::SeqCst);
                true
            });
        assert_eq!(c.get("/x").unwrap().statut, 401);
        assert_eq!(compteur.load(Ordering::SeqCst), 1);
        assert_eq!(faux.appels(), 2);
    }

    #[test]
    fn un_rafraichissement_qui_echoue_rend_le_401_au_lieu_de_boucler() {
        let faux = Faux::nouveau(vec![Ok(ReponseHttp::simple(401, "mort")); 5]);
        let b = budget();
        let (_, dormir) = banc();
        let c = ClientRobuste::nouveau(faux.clone(), classe_http, b)
            .avec_banc(dormir, || 1.0)
            .avec_rafraichissement(|| false);
        assert_eq!(c.get("/x").unwrap().statut, 401);
        assert_eq!(faux.appels(), 1);
    }

    #[test]
    fn sans_rafraichisseur_un_401_remonte_tel_quel() {
        // Et pas en boucle : l'adaptateur le classera en « reauth requise ».
        let faux = Faux::nouveau(vec![Ok(ReponseHttp::simple(401, "x")); 5]);
        let b = budget();
        let (c, _) = client(&faux, &b);
        assert_eq!(c.get("/x").unwrap().statut, 401);
        assert_eq!(faux.appels(), 1);
    }
}
