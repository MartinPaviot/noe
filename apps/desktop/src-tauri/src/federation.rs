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

/// R5.3 : le budget d'appels par épisode, **quand le terrain n'en fixe pas**.
///
/// La valeur réelle vient de `terrain.json` (`budgets.reads_per_episode`), comme
/// le design §2 le prévoit. Celle-ci n'est qu'un défaut : une constante qui
/// gouvernerait pour de bon serait un réglage de terrain encodé dans le binaire,
/// exactement ce que R1.1 refuse pour le nom du CRM.
pub const BUDGET_APPELS: u32 = crate::terrain::BUDGET_PAR_DEFAUT;

/// Une référence d'enregistrement chez le système distant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct RefApi {
    pub connector: String,
    pub object: String,
    pub id: String,
}

/// L'instant mural, en ISO 8601 UTC.
///
/// **La même fonction que les bornes d'épisode.** Deux horodatages du même
/// corpus qui ne se formatent pas pareil ne se comparent plus — et c'est le
/// genre de divergence qui ne se voit qu'au moment où on en a besoin.
pub fn maintenant_iso() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    crate::assemblage::horodater(ms)
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
    /// **On n'a pas pu regarder.** Droits refusés, quota, panne, réponse
    /// illisible.
    ///
    /// Ce n'est pas `Introuvable`, et la distinction n'est pas cosmétique :
    /// `not_found` affirme que l'enregistrement n'existe pas, ce qui est une
    /// conclusion. Une résolution empêchée n'en tire aucune — c'est un trou de
    /// couverture, et la règle 4 dit qu'un trou s'enregistre au lieu de se
    /// reboucher.
    ///
    /// Le contrat TypeScript exprime déjà cette différence : `resoudre` y rend un
    /// `Result`, donc un échec d'appel sort en `err` et ne devient jamais un
    /// `not_found`. Le miroir Rust avait aplati le `Result` et perdu la nuance.
    Empechee(String),
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
            Self::Empechee(cause) => format!("blocked:{cause}"),
        }
    }
}

/// R4.1 — un changement observé du côté du système, miroir d'`ApiChange`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChangementApi {
    pub reference: RefApi,
    /// Quand, en mural ISO.
    pub quand: String,
    /// Les champs qui ont bougé, quand le système les nomme.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub champs: Vec<String>,
    /// L'acteur, **quand le système l'expose**.
    ///
    /// `None` veut dire « inconnu » et **jamais « l'opérateur »** : R4.2 range un
    /// changement d'un autre acteur hors périmètre, et supposer l'opérateur par
    /// défaut expliquerait des changements qu'il n'a pas faits — c'est-à-dire
    /// gonflerait la métrique de santé avec le travail des collègues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acteur: Option<String>,
}

impl ChangementApi {
    /// R4.2 — ce changement est-il hors périmètre parce qu'un autre l'a fait ?
    ///
    /// Un acteur inconnu n'est **pas** un autre acteur : sans information, on ne
    /// range rien hors périmètre, et le changement suit le chemin ordinaire.
    /// Ranger sur une supposition retirerait du dénominateur des changements
    /// qu'on n'a pas expliqués.
    pub fn fait_par_un_autre(&self, operateur: Option<&str>) -> bool {
        match (self.acteur.as_deref(), operateur) {
            (Some(a), Some(o)) => a != o,
            _ => false,
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
        /// Le rang de l'événement qui l'a fait apparaître.
        ///
        /// Il vient de la capture et de nulle part ailleurs : un compteur
        /// interne au worker compterait l'ordre des RÉSOLUTIONS, qui dépend du
        /// réseau, et deux rejeux du même épisode ne le rendraient pas pareil.
        first_seen_seq: u64,
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
        first_seen_seq: u64,
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

/// Normalise une valeur d'identification avant tokenisation (R2.1, R6.2).
///
/// **Miroir de `normaliserIdentifiant`.** La phrase « les mêmes règles des deux
/// côtés » était écrite en commentaire dans quatre fichiers, et un commentaire ne
/// vérifie rien : deux graphies d'une adresse qui ne convergeraient que d'un côté
/// donneraient deux jetons pour une personne, et la jointure serait perdue sans
/// que personne ne le voie. Les vecteurs de `vecteurs-resolution.json` la
/// gardent.
///
/// - **courriel** : la casse d'une adresse n'est pas significative en pratique,
///   et les blancs de bordure viennent des copier-coller — ils n'appartiennent à
///   personne.
/// - **domaine** : même chose.
/// - **identifiant système** : opaque. On ne touche pas à sa casse, qui peut
///   être significative — chez Salesforce elle porte le suffixe de contrôle,
///   donc de l'information. Seuls les blancs de bordure partent.
pub fn normaliser_identifiant(genre: &str, valeur: &str) -> String {
    match genre {
        "system_id" => valeur.trim().to_owned(),
        _ => valeur.trim().to_lowercase(),
    }
}

/// Normalise un libellé lu à l'écran, pour le rapprocher d'un libellé d'API.
///
/// Un nom accessible n'est pas un libellé propre : il porte les deux-points du
/// rendu, l'astérisque du champ obligatoire, des espaces doubles, et une casse
/// qui varie d'un thème à l'autre. Rapprocher sans normaliser ne rapprocherait
/// presque rien — et l'échec serait silencieux, sous la forme d'un champ observé
/// qu'on ne lit jamais.
pub fn normaliser_libelle(brut: &str) -> String {
    brut.replace(['*', ':', '\u{00a0}'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// R3.1 — les champs à lire : le **périmètre de la tâche**, plus les **champs
/// observés changés**.
///
/// L'index vient du système lui-même (voir `salesforce::index_des_libelles`) et
/// jamais d'une table écrite à la main : **les libellés sont traduits**. Une org
/// en français montre « Statut » là où l'API dit `Status`, et une table figée
/// marcherait sur la machine de son auteur et nulle part ailleurs.
///
/// Rend aussi les libellés observés qu'on **n'a pas su** rapprocher. Ils ne sont
/// pas jetés : un champ observé qu'on ne lit pas est un état incomplet, et R3.3
/// veut savoir pourquoi plutôt que de compter un `unknown_before` sans cause.
pub fn champs_a_lire(
    scope_fields: &[String],
    libelles_observes: &[String],
    index: &BTreeMap<String, Option<String>>,
) -> (Vec<String>, Vec<String>) {
    let mut champs: Vec<String> = scope_fields.to_vec();
    let mut sans_correspondance = Vec::new();

    for observe in libelles_observes {
        match index.get(&normaliser_libelle(observe)) {
            // Le libellé désigne exactement un champ : il entre.
            Some(Some(nom)) => {
                if !champs.contains(nom) {
                    champs.push(nom.clone());
                }
            }
            // **Deux champs portent ce libellé.** Prendre les deux ferait entrer
            // dans l'épisode un champ que personne n'a touché ; prendre l'un des
            // deux serait deviner, ce que R2.2 interdit ailleurs pour la même
            // raison. On ne prend rien, et on le dit.
            Some(None) => sans_correspondance.push(format!("{observe} (libelle ambigu)")),
            None => sans_correspondance.push(format!("{observe} (libelle inconnu)")),
        }
    }
    (champs, sans_correspondance)
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
    /// R2.3 : la clé qui a tranché, et quand.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved: Option<crate::assemblage::Resolue>,
    /// Le rang de l'événement de première vue, tel que la capture l'a donné.
    #[serde(default)]
    pub first_seen_seq: u64,
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
                first_seen_seq,
                resolution,
            } => {
                let entree = e.entry(candidate_id).or_default();
                entree.first_seen_seq = first_seen_seq;
                match resolution {
                    Resolution::Resolue {
                        reference,
                        par,
                        quand,
                    } => {
                        entree.api_ref = Some(reference);
                        entree.non_resolue = None;
                        // R2.3 : ce qui a tranché, et quand. Sans ça, une
                        // résolution fausse est indiagnosticable.
                        entree.resolved = Some(crate::assemblage::Resolue { by: par, at: quand });
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
        budget: Arc<crate::client::Budget>,
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
                    budget.as_ref(),
                )
            })
            .expect("fil de federation");
        (Self { demandes }, fil)
    }

    /// R3.1 — une entité vient d'apparaître.
    pub fn premiere_vue(
        &self,
        candidate_id: &str,
        first_seen_seq: u64,
        cles: Vec<(String, String)>,
    ) {
        let _ = self.demandes.send(Demande::Resoudre {
            candidate_id: candidate_id.to_string(),
            first_seen_seq,
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
    budget: &crate::client::Budget,
) {
    // Le budget vit avec l'épisode, pas avec le connecteur : deux connecteurs se
    // partageraient sinon le double. Et il n'est **pas débité ici** : c'est le
    // client commun qui le fait, une prise par tentative — y compris par
    // tentative ratée, parce que c'est précisément quand ça échoue qu'on
    // martèle. Le worker se contente de refuser de partir quand il ne reste
    // rien, ce qui évite d'ouvrir une opération dont aucune requête ne pourra
    // sortir.

    while let Ok(d) = reception.recv() {
        match d {
            Demande::Resoudre {
                candidate_id,
                first_seen_seq,
                cles,
            } => {
                if budget.reste() == 0 {
                    registre.appliquer(Reponse::EtatAvant {
                        candidate_id,
                        issue: Issue::Trou(format!(
                            "budget d appels epuise ({})",
                            budget.plafond()
                        )),
                    });
                    continue;
                }
                let r = federation.resoudre(&cles);
                let reference = match &r {
                    Resolution::Resolue { reference, .. } => Some(reference.clone()),
                    _ => None,
                };
                registre.appliquer(Reponse::Resolution {
                    candidate_id: candidate_id.clone(),
                    first_seen_seq,
                    resolution: r,
                });
                // R3.1 : la lecture suit **immédiatement** la résolution. Attendre
                // la clôture pour lire l'état d'avant lirait l'état d'après.
                if let Some(reference) = reference {
                    if budget.reste() > 0 {
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
                    if budget.reste() == 0 {
                        registre.appliquer(Reponse::EtatApres {
                            candidate_id: id,
                            issue: Issue::Trou(format!(
                                "budget d appels epuise ({})",
                                budget.plafond()
                            )),
                        });
                        continue;
                    }
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
        /// Le budget, quand ce banc joue le rôle d'un vrai adaptateur.
        ///
        /// **Seul le client débite**, et un adaptateur qui ne passerait pas par
        /// lui ne consommerait rien : le banc doit donc débiter lui aussi, sinon
        /// il testerait un monde où le budget ne borne rien.
        budget: Option<Arc<crate::client::Budget>>,
    }

    impl Banc {
        fn nouveau(resolution: Resolution, issue: Issue) -> Self {
            Self {
                resolution,
                issue,
                lenteur_ms: 0,
                appels: Mutex::new(0),
                budget: None,
            }
        }

        /// Le banc passe par un budget, comme le ferait le client commun.
        fn sur(mut self, budget: &Arc<crate::client::Budget>) -> Self {
            self.budget = Some(budget.clone());
            self
        }

        /// Prend un appel. Rend `false` quand le budget est épuisé.
        fn appeler(&self) -> bool {
            *self.appels.lock().unwrap() += 1;
            self.budget.as_ref().is_none_or(|b| b.prendre())
        }
    }

    impl Federation for Banc {
        fn resoudre(&self, _cles: &[(String, String)]) -> Resolution {
            if !self.appeler() {
                // Ce que rend le client commun quand le budget est épuisé, et ce
                // que l'adaptateur en fait : on n'a pas pu regarder.
                return Resolution::Empechee(format!(
                    "budget d appels epuise ({})",
                    self.budget.as_ref().map_or(0, |b| b.plafond())
                ));
            }
            self.resolution.clone()
        }
        fn lire(&self, _r: &RefApi, _c: &[String]) -> Issue {
            if !self.appeler() {
                return Issue::Trou(format!(
                    "budget d appels epuise ({})",
                    self.budget.as_ref().map_or(0, |b| b.plafond())
                ));
            }
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

    /// Un budget d'épisode, comme le terrain en fixe un.
    fn budget(plafond: u32) -> Arc<crate::client::Budget> {
        Arc::new(crate::client::Budget::nouveau(plafond))
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
        let (w, fil) = Worker::demarrer(
            banc,
            registre.clone(),
            champs(),
            redacteur(),
            budget(BUDGET_APPELS),
        );
        w.premiere_vue("c1", 1, vec![("email_token".into(), "EMAIL_aaa".into())]);
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
        let (w, fil) = Worker::demarrer(
            banc,
            registre.clone(),
            champs(),
            redacteur(),
            budget(BUDGET_APPELS),
        );
        w.premiere_vue("c1", 1, vec![]);
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
        let (w, fil) = Worker::demarrer(
            banc,
            registre.clone(),
            champs(),
            redacteur(),
            budget(BUDGET_APPELS),
        );
        w.premiere_vue("c1", 1, vec![]);
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
        let (w, fil) = Worker::demarrer(
            banc,
            registre.clone(),
            champs(),
            redacteur(),
            budget(BUDGET_APPELS),
        );
        w.premiere_vue("c1", 1, vec![]);
        w.relire_tout();
        let _ = fil.join();

        let e = registre.instantane();
        assert!(e["c1"].trous[0].starts_with("hors perimetre"));
    }

    #[test]
    fn le_budget_vient_du_terrain_et_pas_d_une_constante() {
        // R5.3 et le design §2 : `budgets.reads_per_episode` vit dans
        // terrain.json. Une constante qui gouvernerait pour de bon serait un
        // reglage de terrain encode dans le binaire — exactement ce que R1.1
        // refuse pour le nom du CRM.
        let registre = Arc::new(Registre::nouveau());
        let b = budget(2);
        let banc = Arc::new(Banc::nouveau(resolue(), etat("x")).sur(&b));
        let (w, fil) = Worker::demarrer(banc.clone(), registre.clone(), champs(), redacteur(), b);
        for i in 0..10 {
            w.premiere_vue(&format!("c{i}"), 1, vec![]);
        }
        w.relire_tout();
        let _ = fil.join();

        let appels = *banc.appels.lock().unwrap();
        assert!(appels <= 2, "{appels} appels pour un budget de 2");
        assert!(appels > 0, "un budget de 2 n est pas un budget de 0");
    }

    #[test]
    fn un_depassement_de_budget_se_dit_avec_le_chiffre_du_terrain() {
        // Le trou declare doit citer le budget REEL : « epuise (30) » sur un
        // terrain qui en fixe 2 enverrait chercher au mauvais endroit.
        let registre = Arc::new(Registre::nouveau());
        let b = budget(2);
        let banc = Arc::new(Banc::nouveau(resolue(), etat("x")).sur(&b));
        let (w, fil) = Worker::demarrer(banc, registre.clone(), champs(), redacteur(), b);
        for i in 0..6 {
            w.premiere_vue(&format!("c{i}"), 1, vec![]);
        }
        drop(w);
        let _ = fil.join();

        let trous: Vec<String> = registre
            .instantane()
            .into_values()
            .flat_map(|e| e.trous)
            .collect();
        assert!(
            trous.iter().any(|t| t.contains("(2)")),
            "aucun trou ne cite le budget du terrain : {trous:?}"
        );
    }

    #[test]
    fn le_budget_d_appels_borne_l_episode() {
        // R5.3 : depassement -> arret des lectures + trou declare, jamais de
        // tempete de requetes.
        let registre = Arc::new(Registre::nouveau());
        let b = budget(BUDGET_APPELS);
        let banc = Arc::new(Banc::nouveau(resolue(), etat("x")).sur(&b));
        let (w, fil) = Worker::demarrer(
            banc.clone(),
            registre.clone(),
            champs(),
            redacteur(),
            b.clone(),
        );
        // Chaque premiere vue coute deux appels : resolution + lecture.
        for i in 0..(BUDGET_APPELS + 10) {
            w.premiere_vue(&format!("c{i}"), 1, vec![]);
        }
        w.relire_tout();
        let _ = fil.join();

        // C'est le BUDGET qui borne, et pas le compteur du banc : le banc compte
        // toutes les sollicitations, y compris celles qu'il refuse — ce que fait
        // aussi le client quand il rend « budget epuise » sans appeler.
        assert_eq!(
            b.consommes(),
            BUDGET_APPELS,
            "le budget n a pas ete consomme jusqu au bout"
        );
        assert!(
            b.reste() == 0,
            "il en reste apres {} sollicitations",
            *banc.appels.lock().unwrap()
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
        let (w, fil) = Worker::demarrer(
            Arc::new(banc),
            registre.clone(),
            champs(),
            redacteur(),
            budget(BUDGET_APPELS),
        );
        w.premiere_vue("c1", 1, vec![]);

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
            first_seen_seq: 1,
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
        // La promotion appartient a une spec ulterieure : le trait n'expose que
        // `resoudre` et `lire`, et une methode ajoutee « pour plus tard » finit
        // implementee puis appelee.
        //
        // La premiere version de ce test n'assertait RIEN. Elle passait une
        // implementation a une fonction generique, et son commentaire affirmait
        // qu'elle echouerait a la compilation si un verbe d'ecriture
        // apparaissait. C'etait faux deux fois : ce qui casserait, c'est l'`impl`
        // du banc, pas cet appel — et surtout, un trait peut gagner une methode
        // a valeur par defaut sans casser quoi que ce soit.
        //
        // On lit donc la declaration elle-meme.
        let source = include_str!("federation.rs");
        let debut = source
            .find("pub trait Federation: Send + Sync {")
            .expect("la declaration du trait a change de forme");
        let corps = &source[debut..];
        let fin = corps
            .find(
                "
}",
            )
            .expect("fin du trait");
        let methodes: Vec<&str> = corps[..fin]
            .lines()
            .filter_map(|l| l.trim().strip_prefix("fn "))
            .filter_map(|l| l.split('(').next())
            .collect();

        assert_eq!(
            methodes,
            vec!["resoudre", "lire"],
            "le trait a change de surface"
        );
        for verbe in [
            "write",
            "ecrire",
            "update",
            "creer",
            "create",
            "supprimer",
            "delete",
        ] {
            assert!(
                !corps[..fin].contains(verbe),
                "le trait expose un verbe d ecriture : {verbe}"
            );
        }
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
        let (w, fil) = Worker::demarrer(
            banc,
            registre.clone(),
            champs(),
            redacteur(),
            budget(BUDGET_APPELS),
        );
        w.premiere_vue("c1", 1, vec![]);
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

/// La clé qui dit à quel connecteur une candidate s'adresse.
///
/// Ce n'est **pas** une clé forte : elle ne désigne aucun enregistrement, elle
/// désigne un système. Le routeur la retire avant de passer la main, pour qu'un
/// adaptateur ne la voie jamais et ne puisse pas la confondre avec une clé.
pub const CLE_CONNECTEUR: &str = "connecteur";

/// Plusieurs systèmes, une seule fédération.
///
/// Le worker ne connaît qu'un `Federation`, et c'est bien ainsi : il n'a pas à
/// savoir combien de systèmes sont branchés. Mais une adresse de courriel désigne
/// une personne dans le CRM pendant qu'un identifiant de fil désigne un fil chez
/// Gmail — ce ne sont pas les mêmes entités, et elles ne se résolvent pas au même
/// endroit. C'est la candidate qui porte sa destination ; le routeur la lit.
///
/// **Il n'invente jamais.** Sans destination nommée, ou avec une destination
/// inconnue, il rend `Empechee` plutôt que d'essayer le premier adaptateur venu :
/// résoudre une candidate contre le mauvais système donnerait une réponse — et
/// c'est bien le problème.
#[derive(Default)]
pub struct Routeur {
    adaptateurs: BTreeMap<String, Arc<dyn Federation>>,
}

impl Routeur {
    pub fn nouveau() -> Self {
        Self::default()
    }

    /// Branche un adaptateur sous le nom de son connecteur.
    pub fn brancher(&mut self, nom: &str, adaptateur: Arc<dyn Federation>) {
        self.adaptateurs.insert(nom.to_owned(), adaptateur);
    }

    /// Les connecteurs branchés, pour le dire au tray.
    pub fn branches(&self) -> Vec<&str> {
        self.adaptateurs.keys().map(String::as_str).collect()
    }
}

impl Federation for Routeur {
    fn resoudre(&self, cles: &[(String, String)]) -> Resolution {
        let Some((_, nom)) = cles.iter().find(|(genre, _)| genre == CLE_CONNECTEUR) else {
            return Resolution::Empechee("candidate sans connecteur nomme".into());
        };
        let Some(adaptateur) = self.adaptateurs.get(nom.as_str()) else {
            return Resolution::Empechee(format!("connecteur non branche : {nom}"));
        };
        let fortes: Vec<(String, String)> = cles
            .iter()
            .filter(|(genre, _)| genre != CLE_CONNECTEUR)
            .cloned()
            .collect();
        adaptateur.resoudre(&fortes)
    }

    fn lire(&self, reference: &RefApi, champs: &[String]) -> Issue {
        match self.adaptateurs.get(reference.connector.as_str()) {
            Some(adaptateur) => adaptateur.lire(reference, champs),
            None => {
                Issue::HorsPerimetre(format!("connecteur non branche : {}", reference.connector))
            }
        }
    }
}

#[cfg(test)]
mod tests_miroir_resolution {
    use super::*;

    /// Le miroir produit par `scripts/generer-resolution.mjs` depuis
    /// `resolution.ts`.
    #[derive(serde::Deserialize)]
    struct Miroir {
        priorite: Vec<String>,
        normalisation: Vec<CasNormalisation>,
        resolutions: Vec<CasResolution>,
    }
    #[derive(serde::Deserialize)]
    struct CasNormalisation {
        kind: String,
        valeur: String,
        attendu: String,
    }
    #[derive(serde::Deserialize)]
    struct CasResolution {
        nom: String,
        statut: String,
        compte: Option<usize>,
    }

    const MIROIR: &str = include_str!("../../../../packages/core/vecteurs-resolution.json");

    fn miroir() -> Miroir {
        serde_json::from_str(MIROIR).expect("vecteurs-resolution.json")
    }

    #[test]
    fn l_ordre_de_force_des_cles_est_le_meme_des_deux_cotes() {
        // Inverser deux entrees ne casserait aucun test : ca deciderait seulement
        // qu'une ambiguite de courriel se laisse trancher par un nom, ce que R2.2
        // interdit — et le corpus aurait l'air juste.
        assert_eq!(miroir().priorite, crate::salesforce::PRIORITE);
    }

    #[test]
    fn la_normalisation_est_la_meme_des_deux_cotes() {
        // « Les memes regles des deux cotes » etait ecrit en commentaire dans
        // quatre fichiers. Un commentaire ne verifie rien.
        let mut ecarts = Vec::new();
        for cas in miroir().normalisation {
            let obtenu = normaliser_identifiant(&cas.kind, &cas.valeur);
            if obtenu != cas.attendu {
                ecarts.push(format!(
                    "{} sur « {} » : « {} » ici, « {} » en TypeScript",
                    cas.kind, cas.valeur, obtenu, cas.attendu
                ));
            }
        }
        assert!(
            ecarts.is_empty(),
            "les deux normalisations divergent :\n{ecarts:#?}"
        );
    }

    #[test]
    fn la_casse_d_un_identifiant_systeme_survit_a_la_normalisation() {
        // Le miroir doit CONTENIR ce cas, sinon le test precedent passerait sur
        // un jeu qui ne l'exerce pas. Un vecteur absent est un controle absent.
        let m = miroir();
        let systeme: Vec<&CasNormalisation> = m
            .normalisation
            .iter()
            .filter(|c| c.kind == "system_id")
            .collect();
        assert!(systeme.len() >= 2, "le miroir n eprouve pas la casse");
        assert!(
            systeme
                .iter()
                .any(|c| c.attendu.chars().any(char::is_uppercase)),
            "aucun vecteur ne garde de majuscule : la regle ne serait pas eprouvee"
        );
    }

    #[test]
    fn une_ambiguite_reste_ambigue_des_deux_cotes() {
        // Le scenario qui compte : deux candidats par courriel, un seul par
        // domaine + nom. Affiner avec la cle plus faible, c'est deviner.
        let m = miroir();
        let cas = m
            .resolutions
            .iter()
            .find(|c| c.nom.contains("deux candidats"))
            .expect("le miroir doit porter ce scenario");
        assert_eq!(cas.statut, "ambiguous");
        assert_eq!(cas.compte, Some(2));
    }
}

#[cfg(test)]
mod tests_perimetre {
    use super::*;

    fn index() -> BTreeMap<String, Option<String>> {
        let mut i = BTreeMap::new();
        i.insert("statut".into(), Some("Status".into()));
        i.insert("evaluation".into(), Some("Rating".into()));
        i.insert("description".into(), Some("Description".into()));
        // Deux champs portent ce libelle : l'org a un `Priorite__c` a cote d'un
        // champ standard homonyme.
        i.insert("priorite".into(), None);
        i
    }

    fn scope() -> Vec<String> {
        vec!["Status".into(), "Rating".into()]
    }

    #[test]
    fn un_libelle_lu_a_l_ecran_se_normalise_avant_d_etre_rapproche() {
        // Un nom accessible porte les deux-points du rendu, l'asterisque du
        // champ obligatoire, des espaces doubles, une casse qui varie. Rapprocher
        // sans normaliser ne rapprocherait presque rien — et l'echec serait
        // silencieux, sous la forme d'un champ observe qu'on ne lit jamais.
        assert_eq!(normaliser_libelle("Statut :"), "statut");
        assert_eq!(normaliser_libelle("* Statut"), "statut");
        assert_eq!(
            normaliser_libelle("  Date  de   cloture "),
            "date de cloture"
        );
        assert_eq!(normaliser_libelle("Statut\u{00a0}:"), "statut");
    }

    #[test]
    fn le_perimetre_de_la_tache_est_toujours_lu() {
        // Meme si rien n'a ete observe : c'est le perimetre qui definit ce que le
        // juge compare, pas ce que l'operateur a touche.
        let (champs, sans) = champs_a_lire(&scope(), &[], &index());
        assert_eq!(champs, vec!["Status", "Rating"]);
        assert!(sans.is_empty());
    }

    #[test]
    fn un_champ_observe_hors_perimetre_s_ajoute() {
        // R3.1 : « restreint aux scope_fields de la tache PLUS les champs
        // observes changes ». Sans cette union, un changement vu a l'ecran
        // n'aurait aucun etat en face de lui.
        let (champs, sans) = champs_a_lire(&scope(), &["Description :".into()], &index());
        assert_eq!(champs, vec!["Status", "Rating", "Description"]);
        assert!(sans.is_empty());
    }

    #[test]
    fn un_champ_deja_au_perimetre_ne_se_dedouble_pas() {
        let (champs, _) = champs_a_lire(&scope(), &["Statut".into()], &index());
        assert_eq!(champs, vec!["Status", "Rating"]);
    }

    #[test]
    fn un_libelle_ambigu_n_entre_pas_et_se_declare() {
        // Prendre les deux ferait entrer un champ que personne n'a touche ;
        // prendre l'un des deux serait deviner, ce que R2.2 interdit ailleurs
        // pour exactement la meme raison.
        let (champs, sans) = champs_a_lire(&scope(), &["Priorite".into()], &index());
        assert_eq!(champs, scope());
        assert_eq!(sans.len(), 1);
        assert!(sans[0].contains("ambigu"), "{sans:?}");
    }

    #[test]
    fn un_libelle_inconnu_se_declare_au_lieu_d_etre_avale() {
        // Un champ observe qu'on ne lit pas est un etat incomplet, et R3.3 veut
        // savoir pourquoi plutot que de compter un unknown_before sans cause.
        let (champs, sans) = champs_a_lire(&scope(), &["Chiffre d affaires".into()], &index());
        assert_eq!(champs, scope());
        assert_eq!(sans.len(), 1);
        assert!(sans[0].contains("inconnu"), "{sans:?}");
    }

    #[test]
    fn l_ordre_des_champs_ne_depend_pas_de_l_ordre_d_observation() {
        // Deux episodes du meme parcours doivent demander les memes champs dans
        // le meme ordre : une URL de lecture qui varie donne deux caches, et
        // deux etats qui se comparent mal.
        let a = champs_a_lire(&scope(), &["Description".into()], &index()).0;
        let b = champs_a_lire(&scope(), &["Description".into(), "Statut".into()], &index()).0;
        assert_eq!(a, b);
    }
}

#[cfg(test)]
mod tests_routeur {
    use super::*;

    /// Un adaptateur qui note ce qu'on lui a demandé.
    struct Espion {
        nom: &'static str,
        vues: Mutex<Vec<Vec<(String, String)>>>,
    }

    impl Espion {
        fn nouveau(nom: &'static str) -> Arc<Self> {
            Arc::new(Self {
                nom,
                vues: Mutex::new(Vec::new()),
            })
        }
    }

    impl Federation for Espion {
        fn resoudre(&self, cles: &[(String, String)]) -> Resolution {
            self.vues.lock().unwrap().push(cles.to_vec());
            Resolution::Resolue {
                reference: RefApi {
                    connector: self.nom.into(),
                    object: "o".into(),
                    id: "1".into(),
                },
                par: "system_id".into(),
                quand: "2026-01-01T00:00:00.000Z".into(),
            }
        }
        fn lire(&self, _reference: &RefApi, _champs: &[String]) -> Issue {
            Issue::Trou(format!("lu par {}", self.nom))
        }
    }

    fn routeur(a: &Arc<Espion>, b: &Arc<Espion>) -> Routeur {
        let mut r = Routeur::nouveau();
        r.brancher("salesforce", a.clone());
        r.brancher("gmail", b.clone());
        r
    }

    #[test]
    fn une_candidate_va_au_connecteur_qu_elle_nomme() {
        let sf = Espion::nouveau("salesforce");
        let gm = Espion::nouveau("gmail");
        let r = routeur(&sf, &gm);
        let cles = vec![
            (CLE_CONNECTEUR.to_owned(), "gmail".to_owned()),
            ("system_id".to_owned(), "18f0".to_owned()),
        ];
        match r.resoudre(&cles) {
            Resolution::Resolue { reference, .. } => assert_eq!(reference.connector, "gmail"),
            autre => panic!("{autre:?}"),
        }
        assert!(sf.vues.lock().unwrap().is_empty(), "le CRM a ete derange");
    }

    #[test]
    fn la_cle_de_routage_ne_descend_pas_dans_l_adaptateur() {
        // Elle ne designe aucun enregistrement. La laisser passer donnerait a
        // l'adaptateur une cle de genre inconnu a interpreter.
        let sf = Espion::nouveau("salesforce");
        let gm = Espion::nouveau("gmail");
        let r = routeur(&sf, &gm);
        let _ = r.resoudre(&[
            (CLE_CONNECTEUR.to_owned(), "salesforce".to_owned()),
            ("email_token".to_owned(), "j@ex.com".to_owned()),
        ]);
        let vues = sf.vues.lock().unwrap();
        assert_eq!(vues.len(), 1);
        assert_eq!(
            vues[0],
            vec![("email_token".to_owned(), "j@ex.com".to_owned())]
        );
    }

    #[test]
    fn sans_connecteur_nomme_le_routeur_n_invente_pas() {
        // Resoudre une candidate contre le mauvais systeme donnerait une
        // reponse — et c'est bien le probleme.
        let sf = Espion::nouveau("salesforce");
        let gm = Espion::nouveau("gmail");
        let r = routeur(&sf, &gm);
        match r.resoudre(&[("email_token".to_owned(), "j@ex.com".to_owned())]) {
            Resolution::Empechee(c) => assert!(c.contains("connecteur"), "{c}"),
            autre => panic!("{autre:?}"),
        }
        assert!(sf.vues.lock().unwrap().is_empty());
        assert!(gm.vues.lock().unwrap().is_empty());
    }

    #[test]
    fn un_connecteur_inconnu_ne_tombe_pas_sur_le_premier_venu() {
        let sf = Espion::nouveau("salesforce");
        let gm = Espion::nouveau("gmail");
        let r = routeur(&sf, &gm);
        match r.resoudre(&[(CLE_CONNECTEUR.to_owned(), "hubspot".to_owned())]) {
            Resolution::Empechee(c) => assert!(c.contains("hubspot"), "{c}"),
            autre => panic!("{autre:?}"),
        }
        assert!(sf.vues.lock().unwrap().is_empty());
    }

    #[test]
    fn une_lecture_va_a_l_adaptateur_de_sa_reference() {
        let sf = Espion::nouveau("salesforce");
        let gm = Espion::nouveau("gmail");
        let r = routeur(&sf, &gm);
        let reference = RefApi {
            connector: "gmail".into(),
            object: "thread".into(),
            id: "18f0".into(),
        };
        assert_eq!(
            r.lire(&reference, &["thread.id".to_owned()]),
            Issue::Trou("lu par gmail".into())
        );
    }

    #[test]
    fn une_lecture_d_un_connecteur_non_branche_est_hors_perimetre() {
        let sf = Espion::nouveau("salesforce");
        let gm = Espion::nouveau("gmail");
        let r = routeur(&sf, &gm);
        let reference = RefApi {
            connector: "hubspot".into(),
            object: "o".into(),
            id: "1".into(),
        };
        match r.lire(&reference, &[]) {
            Issue::HorsPerimetre(c) => assert!(c.contains("hubspot"), "{c}"),
            autre => panic!("{autre:?}"),
        }
    }

    #[test]
    fn un_routeur_vide_empeche_au_lieu_de_paniquer() {
        let r = Routeur::nouveau();
        assert!(r.branches().is_empty());
        assert!(matches!(
            r.resoudre(&[(CLE_CONNECTEUR.to_owned(), "salesforce".to_owned())]),
            Resolution::Empechee(_)
        ));
    }
}
