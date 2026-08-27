//! La fédération côté capteur : lire l'état du monde pendant l'épisode
//! (spec 003, R3 et R5.2).
//!
//! ## Pourquoi ce module vit ici et pas dans le harness
//!
//! R3.1 exige la lecture de `state_before` **pendant l'épisode ouvert**, à
//! l'instant où l'entité est résolue. Le harness, lui, ne voit les épisodes
//! qu'après coup. Le contrat vit en TypeScript — `packages/core/src/ports` — et
//! ce module en est le **miroir de production**, exactement comme `motifs.rs`
//! est le miroir de la bibliothèque PII. Une divergence entre les deux rendrait
//! les mesures incomparables ; les vecteurs partagés la feraient voir.
//!
//! ## La règle qui commande tout le reste
//!
//! **La clôture n'attend jamais le réseau.** Une lecture qui n'est pas revenue au
//! bout de soixante secondes devient un trou déclaré, et l'épisode se ferme. Un
//! épisode qui attendrait une API en panne serait un épisode qu'on ne peut pas
//! clore — et l'opérateur qui appuie sur son hotkey de fin a le droit que ça
//! s'arrête.
//!
//! C'est la même doctrine que partout ailleurs dans ce capteur : mieux vaut un
//! trou déclaré qu'une attente silencieuse.

//! ## Pas encore branché
//!
//! Rien en production n'appelle encore ce module : c'est la **tâche 4** qui
//! fournira l'adaptateur réel et la **tâche 6** qui posera le hook de première
//! vue. D'ici là, l'annotation ci-dessous dit qu'on le sait — et elle porte le
//! numéro de la tâche qui devra la retirer, comme la spec 002 l'a fait pour
//! `source.rs` et `moteur.rs`.
//!
//! Un `allow` sans échéance devient un `allow` permanent, et il finit par
//! masquer du vrai code mort.
#![allow(dead_code)] // retiré par la tâche 4 de la spec 003

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

/// R3.2 + design §4 : au-delà, la clôture n'attend plus.
pub const ATTENTE_MAX_CLOTURE_MS: u64 = 60_000;

/// R5.3 : le budget d'appels par épisode, par défaut.
pub const BUDGET_APPELS: u32 = 30;

/// Une référence d'enregistrement chez le système distant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct RefApi {
    pub connector: String,
    pub object: String,
    pub id: String,
}

/// Un état plat, tel que le juge le lit.
pub type EtatPlat = BTreeMap<String, serde_json::Value>;

/// Ce que le juge doit retirer de son verdict, champ par champ (§7).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MetaChamp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconstituted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unknown_before: Option<bool>,
    /// Obligatoire dès qu'un champ sort du verdict : jamais d'exclusion muette.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub type MetaEtat = BTreeMap<String, MetaChamp>;

/// Le verdict d'une résolution (R2.2), miroir du type TypeScript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Resolue {
        reference: RefApi,
        /// La clé qui a tranché.
        par: String,
        /// Quand, en mural ISO.
        quand: String,
    },
    Introuvable,
    Ambigue(usize),
}

impl Resolution {
    /// La raison rendue à l'épisode. `not_found` et `ambiguous:2` n'appellent
    /// pas le même geste, et « non résolu » tout court laisse chercher au
    /// mauvais endroit.
    pub fn raison(&self) -> String {
        match self {
            Self::Resolue { par, quand, .. } => format!("resolue par {par} le {quand}"),
            Self::Introuvable => "not_found".into(),
            Self::Ambigue(n) => format!("ambiguous:{n}"),
        }
    }
}

/// Ce qu'une lecture a donné, ou pourquoi elle n'a rien donné (R5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
    Lu(EtatPlat),
    /// Les tentatives sont épuisées : ça devient un trou, avec sa cause.
    Trou(String),
    /// Droits insuffisants : hors périmètre, avec sa raison.
    HorsPerimetre(String),
}

/// Une demande adressée au worker.
#[derive(Debug, Clone)]
pub enum Demande {
    /// R3.1 : l'entité vient d'être vue pour la première fois.
    Resoudre {
        candidate_id: String,
        cles: Vec<(String, String)>,
    },
    /// R3.2 : l'épisode se clôt, relire tout ce qui est résolu.
    RelireTout,
}

/// Ce que le worker rend.
#[derive(Debug, Clone)]
pub enum Reponse {
    Resolution {
        candidate_id: String,
        resolution: Resolution,
    },
    EtatAvant {
        candidate_id: String,
        issue: Issue,
    },
    EtatApres {
        candidate_id: String,
        issue: Issue,
    },
}

/// Ce que le worker sait faire. Miroir du `ReadConnector` de TypeScript.
///
/// **Aucune écriture.** Le trait ne l'expose pas, et le compilateur l'interdit :
/// la promotion appartient à une spec ultérieure.
pub trait Federation: Send + Sync {
    fn resoudre(&self, cles: &[(String, String)]) -> Resolution;
    fn lire(&self, reference: &RefApi, champs: &[String]) -> Issue;
}

/// L'état d'une entité, du point de vue de la fédération.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EntiteFederee {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_ref: Option<RefApi>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_before: Option<EtatPlat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_after: Option<EtatPlat>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub state_meta: MetaEtat,
    /// La raison, quand la résolution a échoué. Jamais un silence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub non_resolue: Option<String>,
    /// Les trous que la fédération a déclarés sur cette entité.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub trous: Vec<String>,
}

/// Le registre des entités d'un épisode.
///
/// Partagé entre le fil de capture — qui déclare les candidates — et le worker,
/// qui les résout et les lit. Un `Mutex` et non un canal à sens unique : la
/// clôture doit pouvoir lire l'état courant sans attendre que le worker daigne
/// répondre.
#[derive(Debug, Default)]
pub struct Registre {
    entites: Mutex<BTreeMap<String, EntiteFederee>>,
}

impl Registre {
    pub fn nouveau() -> Self {
        Self::default()
    }

    pub fn appliquer(&self, r: Reponse) {
        let mut e = self.entites.lock().expect("registre empoisonne");
        match r {
            Reponse::Resolution {
                candidate_id,
                resolution,
            } => {
                let entree = e.entry(candidate_id).or_default();
                match resolution {
                    Resolution::Resolue { reference, .. } => {
                        entree.api_ref = Some(reference);
                        entree.non_resolue = None;
                    }
                    autre => {
                        // R2.2 : la raison précise, jamais « non résolu » tout
                        // court. C'est elle qui dit où chercher.
                        entree.non_resolue = Some(autre.raison());
                    }
                }
            }
            Reponse::EtatAvant {
                candidate_id,
                issue,
            } => {
                let entree = e.entry(candidate_id).or_default();
                Self::poser(&mut entree.state_before, &mut entree.trous, issue);
            }
            Reponse::EtatApres {
                candidate_id,
                issue,
            } => {
                let entree = e.entry(candidate_id).or_default();
                Self::poser(&mut entree.state_after, &mut entree.trous, issue);
            }
        }
    }

    fn poser(cible: &mut Option<EtatPlat>, trous: &mut Vec<String>, issue: Issue) {
        match issue {
            Issue::Lu(etat) => *cible = Some(etat),
            // Un échec ne laisse PAS un état vide : un état vide se lirait comme
            // « tous les champs sont nuls », et le diff inventerait des
            // changements qui n'ont pas eu lieu. On laisse `None`, et on déclare.
            Issue::Trou(cause) => trous.push(cause),
            Issue::HorsPerimetre(raison) => trous.push(format!("hors perimetre : {raison}")),
        }
    }

    pub fn instantane(&self) -> BTreeMap<String, EntiteFederee> {
        self.entites.lock().expect("registre empoisonne").clone()
    }

    /// Combien d'entités sont pleinement résolues — les deux états présents.
    ///
    /// C'est cette condition, et pas la seule présence d'`api_ref`, qui décide du
    /// grade A : une entité résolue dont on n'a pas su lire l'état n'apporte
    /// rien au juge.
    pub fn pleinement_resolues(&self) -> usize {
        self.instantane()
            .values()
            .filter(|e| e.state_before.is_some() && e.state_after.is_some())
            .count()
    }
}

/// R6.1 — les états fédérés passent le **même** pipeline que la capture.
///
/// « Brancher mes systèmes n'élargit pas ce qui touche mon disque en clair. »
/// C'est la user story de R6, et elle interdit la tentation évidente : une API
/// est une source « de confiance », donc on serait tenté d'écrire ses valeurs
/// telles quelles. Sauf que le disque, lui, ne fait pas la différence — et un
/// numéro de téléphone lu dans un CRM est exactement aussi sensible que le même
/// numéro lu à l'écran.
///
/// La récursion traverse objets et tableaux : une valeur imbriquée n'est pas
/// moins en clair parce qu'elle est profonde.
fn redacter_valeur(
    v: &serde_json::Value,
    redacteur: &crate::redaction::Redacteur,
) -> serde_json::Value {
    match v {
        serde_json::Value::String(t) => serde_json::Value::String(redacteur.redacter(t)),
        serde_json::Value::Array(a) => {
            serde_json::Value::Array(a.iter().map(|x| redacter_valeur(x, redacteur)).collect())
        }
        serde_json::Value::Object(o) => serde_json::Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), redacter_valeur(x, redacteur)))
                .collect(),
        ),
        // Nombres, booléens, null : rien à redacter, et les toucher inventerait
        // une transformation là où il n'y a pas de texte.
        autre => autre.clone(),
    }
}

/// Redacte un état plat entier.
///
/// **Les clés aussi.** Un nom de champ personnalisé peut porter une identité —
/// « Notes_Jean_Dupont__c » existe dans la vraie vie — et une clé en clair fuit
/// aussi sûrement qu'une valeur.
pub fn redacter_etat(etat: &EtatPlat, redacteur: &crate::redaction::Redacteur) -> EtatPlat {
    etat.iter()
        .map(|(k, v)| (redacteur.redacter(k), redacter_valeur(v, redacteur)))
        .collect()
}

/// Le worker : il travaille **hors du chemin de capture**.
///
/// Un `read` qui bloque trois secondes sur une API lente ne doit jamais retarder
/// l'écriture d'un événement au journal. C'est la seule raison d'être de ce fil :
/// séparer une latence réseau d'une latence d'observation.
pub struct Worker {
    demandes: Sender<Demande>,
}

impl Worker {
    /// Démarre le worker et rend de quoi lui parler.
    pub fn demarrer(
        federation: Arc<dyn Federation>,
        registre: Arc<Registre>,
        champs: Vec<String>,
        redacteur: Arc<crate::redaction::Redacteur>,
    ) -> (Self, std::thread::JoinHandle<()>) {
        let (demandes, reception) = std::sync::mpsc::channel();
        let fil = std::thread::Builder::new()
            .name("noe-federation".into())
            .spawn(move || {
                boucle(
                    &reception,
                    federation.as_ref(),
                    &registre,
                    &champs,
                    redacteur.as_ref(),
                )
            })
            .expect("fil de federation");
        (Self { demandes }, fil)
    }

    /// R3.1 — une entité vient d'apparaître.
    pub fn premiere_vue(&self, candidate_id: &str, cles: Vec<(String, String)>) {
        let _ = self.demandes.send(Demande::Resoudre {
            candidate_id: candidate_id.to_string(),
            cles,
        });
    }

    /// R3.2 — l'épisode se clôt.
    pub fn relire_tout(&self) {
        let _ = self.demandes.send(Demande::RelireTout);
    }
}

fn boucle(
    reception: &Receiver<Demande>,
    federation: &dyn Federation,
    registre: &Arc<Registre>,
    champs: &[String],
    redacteur: &crate::redaction::Redacteur,
) {
    // Le budget vit avec l'épisode, pas avec le connecteur : deux connecteurs se
    // partageraient sinon le double.
    let mut budget = BUDGET_APPELS;

    while let Ok(d) = reception.recv() {
        match d {
            Demande::Resoudre { candidate_id, cles } => {
                if budget == 0 {
                    registre.appliquer(Reponse::EtatAvant {
                        candidate_id,
                        issue: Issue::Trou(format!("budget d appels epuise ({BUDGET_APPELS})")),
                    });
                    continue;
                }
                budget -= 1;
                let r = federation.resoudre(&cles);
                let reference = match &r {
                    Resolution::Resolue { reference, .. } => Some(reference.clone()),
                    _ => None,
                };
                registre.appliquer(Reponse::Resolution {
                    candidate_id: candidate_id.clone(),
                    resolution: r,
                });
                // R3.1 : la lecture suit **immédiatement** la résolution. Attendre
                // la clôture pour lire l'état d'avant lirait l'état d'après.
                if let Some(reference) = reference {
                    if budget > 0 {
                        budget -= 1;
                        // R6.1 : la redaction AVANT que quoi que ce soit
                        // n'atteigne le registre, donc avant toute persistance.
                        let issue = redacter_issue(federation.lire(&reference, champs), redacteur);
                        registre.appliquer(Reponse::EtatAvant {
                            candidate_id,
                            issue,
                        });
                    }
                }
            }
            Demande::RelireTout => {
                for (id, e) in registre.instantane() {
                    let Some(reference) = e.api_ref else { continue };
                    if budget == 0 {
                        registre.appliquer(Reponse::EtatApres {
                            candidate_id: id,
                            issue: Issue::Trou(format!("budget d appels epuise ({BUDGET_APPELS})")),
                        });
                        continue;
                    }
                    budget -= 1;
                    let issue = redacter_issue(federation.lire(&reference, champs), redacteur);
                    registre.appliquer(Reponse::EtatApres {
                        candidate_id: id,
                        issue,
                    });
                }
                return;
            }
        }
    }
}

/// Redacte ce qui a été lu, et laisse le reste tel quel.
///
/// Les causes de trou et les raisons de hors-périmètre passent aussi : un
/// message d'erreur d'API cite volontiers l'enregistrement qu'il n'a pas trouvé,
/// et « contact jean.dupont@exemple.fr introuvable » est une fuite qui a l'air
/// d'un diagnostic.
fn redacter_issue(issue: Issue, redacteur: &crate::redaction::Redacteur) -> Issue {
    match issue {
        Issue::Lu(etat) => Issue::Lu(redacter_etat(&etat, redacteur)),
        Issue::Trou(c) => Issue::Trou(redacteur.redacter(&c)),
        Issue::HorsPerimetre(r) => Issue::HorsPerimetre(redacteur.redacter(&r)),
    }
}

/// R3.2 + design §4 — attendre la relecture, **mais pas plus que la borne**.
///
/// Rend `true` si le worker a fini à temps. `false` veut dire que la clôture
/// continue sans lui : les états manquants deviennent des trous, l'épisode
/// perd son grade A, et il se ferme quand même.
///
/// C'est la garantie que R5.2 réclame : « les lectures manquantes déclassent le
/// grade, elles n'empêchent rien ».
pub fn attendre_relecture(
    fil: std::thread::JoinHandle<()>,
    borne_ms: u64,
    horloge: &Arc<dyn crate::horloge::Horloge>,
) -> bool {
    let debut = horloge.monotone_ms();
    while !fil.is_finished() {
        if horloge.monotone_ms().saturating_sub(debut) >= borne_ms {
            // On NE JOINT PAS : joindre bloquerait exactement le temps qu'on
            // refuse d'attendre. Le fil se terminera tout seul et son résultat
            // arrivera trop tard — ce qui est précisément ce qu'on déclare.
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    fil.join().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une fédération de banc, qui obéit à un scénario.
    struct Banc {
        resolution: Resolution,
        issue: Issue,
        /// Combien de millisecondes chaque lecture met à revenir.
        lenteur_ms: u64,
        appels: Mutex<usize>,
    }

    impl Banc {
        fn nouveau(resolution: Resolution, issue: Issue) -> Self {
            Self {
                resolution,
                issue,
                lenteur_ms: 0,
                appels: Mutex::new(0),
            }
        }
    }

    impl Federation for Banc {
        fn resoudre(&self, _cles: &[(String, String)]) -> Resolution {
            *self.appels.lock().unwrap() += 1;
            self.resolution.clone()
        }
        fn lire(&self, _r: &RefApi, _c: &[String]) -> Issue {
            *self.appels.lock().unwrap() += 1;
            if self.lenteur_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(self.lenteur_ms));
            }
            self.issue.clone()
        }
    }

    fn reference() -> RefApi {
        RefApi {
            connector: "crm".into(),
            object: "lead".into(),
            id: "L-1".into(),
        }
    }

    fn resolue() -> Resolution {
        Resolution::Resolue {
            reference: reference(),
            par: "email_token".into(),
            quand: "2026-01-14T09:12:00.000Z".into(),
        }
    }

    fn etat(v: &str) -> Issue {
        Issue::Lu(BTreeMap::from([(
            "statut".to_string(),
            serde_json::json!(v),
        )]))
    }

    fn champs() -> Vec<String> {
        vec!["statut".to_string()]
    }

    fn redacteur() -> Arc<crate::redaction::Redacteur> {
        Arc::new(crate::redaction::Redacteur::new(
            &crate::cle::CleHmac::generer().expect("alea"),
        ))
    }

    #[test]
    fn la_lecture_suit_immediatement_la_resolution() {
        // R3.1 : attendre la cloture pour lire l'etat d'AVANT lirait l'etat
        // d'apres. C'est toute la difficulte de la spec en une phrase.
        let registre = Arc::new(Registre::nouveau());
        let banc = Arc::new(Banc::nouveau(resolue(), etat("nouveau")));
        let (w, fil) = Worker::demarrer(banc, registre.clone(), champs(), redacteur());
        w.premiere_vue("c1", vec![("email_token".into(), "EMAIL_aaa".into())]);
        w.relire_tout();
        assert!(fil.join().is_ok());

        let e = registre.instantane();
        let c1 = e.get("c1").expect("entite");
        assert!(c1.api_ref.is_some());
        assert!(c1.state_before.is_some(), "l etat d avant doit exister");
        assert!(c1.state_after.is_some());
    }

    #[test]
    fn une_resolution_ambigue_laisse_sa_raison() {
        // R2.2 : « not_found » et « ambiguous:2 » n'appellent pas le meme geste,
        // et « non resolu » tout court laisse chercher au mauvais endroit.
        let registre = Arc::new(Registre::nouveau());
        let banc = Arc::new(Banc::nouveau(Resolution::Ambigue(3), etat("x")));
        let (w, fil) = Worker::demarrer(banc, registre.clone(), champs(), redacteur());
        w.premiere_vue("c1", vec![]);
        w.relire_tout();
        let _ = fil.join();

        let e = registre.instantane();
        assert_eq!(e["c1"].non_resolue.as_deref(), Some("ambiguous:3"));
        assert!(e["c1"].api_ref.is_none());
        assert!(e["c1"].state_before.is_none(), "rien a lire sans reference");
    }

    #[test]
    fn une_lecture_qui_echoue_ne_laisse_pas_un_etat_vide() {
        // Un etat vide se lirait comme « tous les champs sont nuls », et le diff
        // inventerait des changements qui n'ont pas eu lieu. On laisse `None`,
        // et on declare le trou.
        let registre = Arc::new(Registre::nouveau());
        let banc = Arc::new(Banc::nouveau(
            resolue(),
            Issue::Trou("api indisponible apres 5 tentatives".into()),
        ));
        let (w, fil) = Worker::demarrer(banc, registre.clone(), champs(), redacteur());
        w.premiere_vue("c1", vec![]);
        w.relire_tout();
        let _ = fil.join();

        let e = registre.instantane();
        assert!(e["c1"].state_before.is_none());
        assert!(!e["c1"].trous.is_empty(), "le trou doit etre declare");
        assert!(e["c1"].trous[0].contains("api indisponible"));
    }

    #[test]
    fn un_refus_de_droits_est_hors_perimetre_pas_un_trou_muet() {
        // R5.2 : `permission` -> hors_perimetre AVEC raison. Le confondre avec un
        // trou accuserait la capture d'avoir rate quelque chose qu'elle n'avait
        // pas le droit de voir.
        let registre = Arc::new(Registre::nouveau());
        let banc = Arc::new(Banc::nouveau(
            resolue(),
            Issue::HorsPerimetre("droits insuffisants sur Lead.Statut".into()),
        ));
        let (w, fil) = Worker::demarrer(banc, registre.clone(), champs(), redacteur());
        w.premiere_vue("c1", vec![]);
        w.relire_tout();
        let _ = fil.join();

        let e = registre.instantane();
        assert!(e["c1"].trous[0].starts_with("hors perimetre"));
    }

    #[test]
    fn le_budget_d_appels_borne_l_episode() {
        // R5.3 : depassement -> arret des lectures + trou declare, jamais de
        // tempete de requetes.
        let registre = Arc::new(Registre::nouveau());
        let banc = Arc::new(Banc::nouveau(resolue(), etat("x")));
        let (w, fil) = Worker::demarrer(banc.clone(), registre.clone(), champs(), redacteur());
        // Chaque premiere vue coute deux appels : resolution + lecture.
        for i in 0..(BUDGET_APPELS + 10) {
            w.premiere_vue(&format!("c{i}"), vec![]);
        }
        w.relire_tout();
        let _ = fil.join();

        let appels = *banc.appels.lock().unwrap();
        assert!(
            appels <= BUDGET_APPELS as usize,
            "{appels} appels pour un budget de {BUDGET_APPELS}"
        );
    }

    #[test]
    fn la_cloture_n_attend_jamais_plus_que_la_borne() {
        // LA regle de la spec : « la cloture n'attend pas ». Un episode qui
        // attendrait une API en panne serait un episode qu'on ne peut pas clore,
        // et l'operateur qui appuie sur son hotkey de fin a le droit que ca
        // s'arrete.
        let registre = Arc::new(Registre::nouveau());
        let mut banc = Banc::nouveau(resolue(), etat("x"));
        banc.lenteur_ms = 5_000;
        let (w, fil) = Worker::demarrer(Arc::new(banc), registre.clone(), champs(), redacteur());
        w.premiere_vue("c1", vec![]);

        let horloge: Arc<dyn crate::horloge::Horloge> =
            Arc::new(crate::horloge::HorlogeReelle::new());
        let debut = horloge.monotone_ms();
        let fini = attendre_relecture(fil, 200, &horloge);
        let ecoule = horloge.monotone_ms() - debut;

        assert!(!fini, "le worker ne devait PAS finir a temps");
        assert!(ecoule < 2_000, "on a attendu {ecoule} ms au lieu de 200");
    }

    #[test]
    fn la_borne_de_cloture_est_celle_du_design() {
        assert_eq!(ATTENTE_MAX_CLOTURE_MS, 60_000);
    }

    #[test]
    fn pleinement_resolue_exige_les_deux_etats() {
        // Une entite resolue dont on n'a pas su lire l'etat n'apporte rien au
        // juge : elle ne doit pas compter pour le grade A.
        let registre = Registre::nouveau();
        registre.appliquer(Reponse::Resolution {
            candidate_id: "c1".into(),
            resolution: resolue(),
        });
        registre.appliquer(Reponse::EtatAvant {
            candidate_id: "c1".into(),
            issue: etat("nouveau"),
        });
        assert_eq!(
            registre.pleinement_resolues(),
            0,
            "il manque l etat d apres"
        );

        registre.appliquer(Reponse::EtatApres {
            candidate_id: "c1".into(),
            issue: etat("qualifie"),
        });
        assert_eq!(registre.pleinement_resolues(), 1);
    }

    #[test]
    fn le_worker_ne_sait_pas_ecrire() {
        // Le trait n'expose ni `write`, ni `update`, ni `create` : la promotion
        // appartient a une spec ulterieure, et le compilateur l'interdit ici.
        // Ce test est une note executable — il echouerait a la compilation si
        // quelqu'un ajoutait un verbe d'ecriture et l'appelait.
        fn accepte_une_federation<F: Federation>(_f: &F) {}
        let banc = Banc::nouveau(resolue(), etat("x"));
        accepte_une_federation(&banc);
    }

    // -- R6 : la confidentialite de la federation --------------------------

    /// Les quatre formes interdites du corpus de canaris, telles quelles.
    const CANARIS: [&str; 4] = [
        "canary.pii@example.invalid",
        "+33600000000",
        "FR7630006000011234567890189",
        "4539148803436467",
    ];

    #[test]
    fn un_etat_lu_passe_le_meme_pipeline_que_la_capture() {
        // R6.1 : « brancher mes systemes n'elargit pas ce qui touche mon disque
        // en clair ». La tentation evidente est de traiter une API comme une
        // source de confiance et d'ecrire ses valeurs telles quelles — sauf que
        // le disque ne fait pas la difference.
        let r = redacteur();
        let etat: EtatPlat = BTreeMap::from([
            ("email".to_string(), serde_json::json!(CANARIS[0])),
            ("telephone".to_string(), serde_json::json!(CANARIS[1])),
            ("iban".to_string(), serde_json::json!(CANARIS[2])),
            ("carte".to_string(), serde_json::json!(CANARIS[3])),
        ]);
        let redacte = redacter_etat(&etat, &r);
        let texte = serde_json::to_string(&redacte).unwrap();
        for c in CANARIS {
            assert!(!texte.contains(c), "« {c} » a fuite :\n{texte}");
        }
        assert!(texte.contains("EMAIL_"), "{texte}");
        assert!(texte.contains("TEL_FR_"), "{texte}");
    }

    #[test]
    fn les_cles_aussi_sont_redactees() {
        // Un nom de champ personnalise peut porter une identite —
        // « Notes_jean.dupont@exemple.fr__c » existe dans la vraie vie — et une
        // cle en clair fuit aussi surement qu'une valeur.
        let r = redacteur();
        let etat: EtatPlat =
            BTreeMap::from([(format!("notes_{}", CANARIS[0]), serde_json::json!("rien"))]);
        let texte = serde_json::to_string(&redacter_etat(&etat, &r)).unwrap();
        assert!(!texte.contains(CANARIS[0]), "{texte}");
    }

    #[test]
    fn une_valeur_imbriquee_n_est_pas_moins_en_clair_parce_qu_elle_est_profonde() {
        let r = redacteur();
        let etat: EtatPlat = BTreeMap::from([(
            "contacts".to_string(),
            serde_json::json!([{ "principal": { "mail": CANARIS[0] } }]),
        )]);
        let texte = serde_json::to_string(&redacter_etat(&etat, &r)).unwrap();
        assert!(!texte.contains(CANARIS[0]), "{texte}");
    }

    #[test]
    fn les_nombres_et_booleens_traversent_intacts() {
        // Les toucher inventerait une transformation la ou il n'y a pas de
        // texte — et un montant tokenise ne se compare plus a rien.
        let r = redacteur();
        let etat: EtatPlat = BTreeMap::from([
            ("montant".to_string(), serde_json::json!(4200)),
            ("actif".to_string(), serde_json::json!(true)),
            ("cloture".to_string(), serde_json::json!(null)),
        ]);
        assert_eq!(redacter_etat(&etat, &r), etat);
    }

    #[test]
    fn une_cause_de_trou_est_redactee_elle_aussi() {
        // Un message d'erreur d'API cite volontiers l'enregistrement qu'il n'a
        // pas trouve. « contact canary.pii@example.invalid introuvable » est une
        // fuite qui a l'air d'un diagnostic.
        let r = redacteur();
        let issue = redacter_issue(
            Issue::Trou(format!("contact {} introuvable", CANARIS[0])),
            &r,
        );
        match issue {
            Issue::Trou(c) => {
                assert!(!c.contains(CANARIS[0]), "{c}");
                assert!(c.contains("EMAIL_"), "{c}");
            }
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn aucun_canari_ne_traverse_le_worker() {
        // Le meme controle, mais de bout en bout : ce qui compte n'est pas que
        // la fonction de redaction marche, c'est que RIEN n'atteigne le registre
        // sans etre passe par elle.
        let registre = Arc::new(Registre::nouveau());
        let sale: EtatPlat = BTreeMap::from([
            ("email".to_string(), serde_json::json!(CANARIS[0])),
            ("tel".to_string(), serde_json::json!(CANARIS[1])),
        ]);
        let banc = Arc::new(Banc::nouveau(resolue(), Issue::Lu(sale)));
        let (w, fil) = Worker::demarrer(banc, registre.clone(), champs(), redacteur());
        w.premiere_vue("c1", vec![]);
        w.relire_tout();
        let _ = fil.join();

        let texte = serde_json::to_string(&registre.instantane()).unwrap();
        for c in CANARIS {
            assert!(
                !texte.contains(c),
                "« {c} » a atteint le registre :\n{texte}"
            );
        }
    }
}
