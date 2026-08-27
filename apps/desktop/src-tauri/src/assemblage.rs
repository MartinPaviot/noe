//! De la capture à l'épisode (spec 002, R1.1, R1.3, R1.4).
//!
//! Le journal est le **film** : tout ce que le capteur a vu, y compris ce que
//! l'application a fait toute seule. L'épisode est le **récit de ce que
//! l'opérateur a fait** : un artefact dérivé, au format que la spec 001 a fixé
//! et que le harness sait juger.
//!
//! Les deux ne contiennent donc pas la même chose, et c'est voulu. Un
//! re-rendu de conteneur appartient au film ; il n'appartient pas au récit,
//! parce que personne ne l'a décidé. Le journal reste sur disque : rien n'est
//! perdu, la sélection est réversible.
//!
//! **Le grade est recalculé ici, exactement comme le fait `gradeOf`.** Si les
//! deux divergent, le harness refuse l'épisode — et c'est le comportement voulu :
//! mieux vaut un refus qu'un corpus dont les grades ne veulent rien dire. Un test
//! croisé compare les deux implémentations sur les mêmes entrées.

use crate::moteur::{CauseGap, EntreeJournal};
use crate::redaction::Redacteur;
use crate::source::GenreEvenement;

/// Version du schéma d'épisode (spec 001).
pub const SCHEMA_V: u32 = 1;

/// Le nom porté par une cible que le capteur n'a pas su résoudre.
///
/// Le schéma exige un nom non vide ; R2.4 exige que l'événement existe quand
/// même. Un marqueur explicite satisfait les deux, et se voit à la lecture — là
/// où un nom inventé se prendrait pour une donnée.
pub const NOM_NON_RESOLU: &str = "(non resolu)";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Cible {
    pub role: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Trou {
    pub cause: CauseGap,
    pub from_seq: u64,
    pub to_seq: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Evenement {
    UiAction {
        schema_v: u32,
        seq: u64,
        ts: String,
        source: String,
        action: String,
        target: Cible,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<String>,
    },
    /// R4.1 — un changement observé du côté du système.
    ///
    /// **Le schéma TypeScript le porte depuis la spec 001 ; ce miroir ne l'avait
    /// jamais eu.** Résultat : le type Rust refusait purement et simplement les
    /// épisodes du corpus doré, et personne ne s'en apercevait parce qu'aucun
    /// test ne le lui demandait. C'est ici que les changements collectés par le
    /// delta entrent dans l'épisode, et c'est sur eux que la réconciliation
    /// travaille.
    ApiChange {
        schema_v: u32,
        seq: u64,
        ts: String,
        source: String,
        connector: String,
        object: String,
        object_id: String,
        /// Les champs qui ont bougé, quand le système les nomme.
        ///
        /// Vide n'est pas faux : un delta dit qu'il s'est passé quelque chose,
        /// pas quoi. Une liste inventée ferait croire à une précision qu'on n'a
        /// pas.
        fields_changed: Vec<String>,
    },
    Gap {
        schema_v: u32,
        seq: u64,
        ts: String,
        source: String,
        gap: Trou,
    },
    /// R7.2 : une qualité qui baisse, dite au flux.
    Degraded {
        schema_v: u32,
        seq: u64,
        ts: String,
        source: String,
        degraded: crate::empreinte::Degradation,
    },
}

impl Evenement {
    /// Le numéro de séquence. Sert aux tests qui vérifient la stricte croissance
    /// exigée par le schéma ; la production lit le champ directement.
    #[cfg(test)]
    pub fn seq(&self) -> u64 {
        match self {
            Self::UiAction { seq, .. }
            | Self::ApiChange { seq, .. }
            | Self::Gap { seq, .. }
            | Self::Degraded { seq, .. } => *seq,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CleEntite {
    #[serde(rename = "type")]
    pub type_entite: String,
    pub value_pseudo: String,
}

#[allow(dead_code)] // retiré quand la tâche 0 permet de collecter un vrai delta
impl crate::federation::ChangementApi {
    /// Le changement, tel qu'il entre dans l'épisode (R4.1).
    ///
    /// La source est `api` et jamais `ui` : c'est ce qui permet à la
    /// réconciliation de distinguer ce que le monde a fait de ce que l'opérateur
    /// a fait. Les confondre expliquerait le premier par le second.
    pub fn en_evenement(&self, seq: u64, ts: &str) -> Evenement {
        Evenement::ApiChange {
            schema_v: SCHEMA_V,
            seq,
            ts: ts.to_owned(),
            source: "api".into(),
            connector: self.reference.connector.clone(),
            object: self.reference.object.clone(),
            object_id: self.reference.id.clone(),
            fields_changed: self.champs.clone(),
        }
    }
}

/// R2.3 (spec 003) : la clé qui a tranché la résolution, et quand.
///
/// **Sans elle, une résolution fausse est indiagnosticable** — on ne sait même
/// pas ce qui l'a décidée. Le schéma TypeScript la porte depuis le début ; ce
/// miroir l'avait oubliée, comme il avait oublié `state_meta`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Resolue {
    pub by: String,
    pub at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Entite {
    pub key: CleEntite,
    pub first_seen_seq: u64,
    pub api_refs: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_before: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_after: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved: Option<Resolue>,
    /// Les champs que le juge doit exclure de son verdict, et pourquoi (§7).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_meta: Option<crate::federation::MetaEtat>,
}

impl Entite {
    fn resolue(&self) -> bool {
        self.state_before.is_some() && self.state_after.is_some()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Completude {
    pub explained: u64,
    pub out_of_scope: u64,
    pub gaps: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Episode {
    pub schema_v: u32,
    pub id: String,
    pub task_slug: String,
    pub t0: String,
    pub t1: String,
    pub events: Vec<Evenement>,
    pub entities: Vec<Entite>,
    pub grade: String,
    pub grade_reason: String,
    pub scope_fields: Vec<String>,
    pub completeness: Completude,
    /// INVARIANT IV — l'épisode que celui-ci **remplace**, s'il en remplace un.
    ///
    /// Un épisode clôturé n'est jamais modifié : une correction produit un
    /// épisode neuf qui référence l'ancien. Ce miroir ne portait pas le champ, ce
    /// qui veut dire qu'une lecture-écriture côté Rust l'aurait **effacé en
    /// silence** — et avec lui le seul fil qui relie une correction à ce qu'elle
    /// corrige. C'est exactement le trou rebouché sans le dire que la règle 4
    /// interdit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
}

/// Pourquoi un épisode ne peut pas être assemblé.
///
/// La quarantaine n'est pas un échec silencieux : l'épisode est conservé avec sa
/// raison (R1.4), parce qu'un épisode qu'on jette est un épisode qu'on ne peut
/// pas diagnostiquer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Quarantaine {
    /// Aucune action de l'opérateur : il n'y a pas de récit à écrire.
    AucuneAction,
    /// L'horloge murale n'a pas de sens (t1 avant t0).
    BornesIncoherentes,
}

impl std::fmt::Display for Quarantaine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AucuneAction => write!(f, "aucune action de l operateur dans l episode"),
            Self::BornesIncoherentes => write!(f, "t1 anterieur a t0"),
        }
    }
}

/// Ce qu'un genre d'événement devient dans le récit — ou rien.
///
/// `None` signifie « ce n'est pas un geste de l'opérateur ». Un re-rendu de
/// conteneur en est le cas typique : l'application se réorganise toute seule, et
/// le faire figurer comme une action attribuerait à l'humain une décision qu'il
/// n'a pas prise.
fn action_de(genre: &GenreEvenement) -> Option<&'static str> {
    match genre {
        GenreEvenement::Invocation(_) => Some("invoke"),
        GenreEvenement::Saisie(_) => Some("input"),
        GenreEvenement::ChangementValeur(_) => Some("toggle"),
        GenreEvenement::Soumission(_) => Some("submit"),
        GenreEvenement::Focus(_) => Some("navigate"),
        GenreEvenement::BasculeApplication { .. } => Some("navigate"),
        GenreEvenement::Copie => Some("copy"),
        GenreEvenement::Collage { .. } => Some("paste"),
        // L'application a bougé, pas l'opérateur.
        GenreEvenement::ChangementStructure(_) => None,
        GenreEvenement::Veille | GenreEvenement::Reveil => None,
    }
}

/// Les actions qui touchent une valeur — celles qui définissent le périmètre.
fn touche_une_valeur(action: &str) -> bool {
    matches!(action, "input" | "toggle" | "submit")
}

pub fn horodater(mural_ms: u64) -> String {
    // ISO 8601 en UTC, sans dépendance : la conversion est arithmétique.
    let secondes = (mural_ms / 1000) as i64;
    let millis = mural_ms % 1000;
    let jours = secondes.div_euclid(86_400);
    let reste = secondes.rem_euclid(86_400);
    let (h, m, s) = (reste / 3600, (reste % 3600) / 60, reste % 60);

    // Algorithme civil-from-days (Howard Hinnant), pour éviter une dépendance
    // de calendrier là où trois lignes suffisent.
    let z = jours + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mois = if mp < 10 { mp + 3 } else { mp - 9 };
    let annee = if mois <= 2 { y + 1 } else { y };

    format!("{annee:04}-{mois:02}-{d:02}T{h:02}:{m:02}:{s:02}.{millis:03}Z")
}

/// Assemble le récit à partir du film.
///
/// `t0_ms` et `t1_ms` sont **muraux** : un épisode doit pouvoir être rapproché
/// d'un courriel ou d'un enregistrement CRM, ce qu'un compteur interne au
/// processus ne permet pas.
pub fn assembler(
    id: &str,
    task_slug: &str,
    t0_ms: u64,
    t1_ms: u64,
    journal: &[EntreeJournal],
    redacteur: &Redacteur,
) -> Result<Episode, Quarantaine> {
    if t1_ms < t0_ms {
        return Err(Quarantaine::BornesIncoherentes);
    }

    let duree_ms = t1_ms.saturating_sub(t0_ms);
    // Les entrées portent un instant monotone depuis l'ouverture ; le mural
    // s'obtient en le reportant sur l'intervalle réel.
    let mural = |monotone_ms: u64| horodater(t0_ms + monotone_ms.min(duree_ms));

    let mut evenements: Vec<Evenement> = Vec::new();
    let mut champs: Vec<String> = Vec::new();
    let mut noms_touches: Vec<String> = Vec::new();
    let mut trous = 0u64;
    let mut hors_perimetre = 0u64;
    let mut dernier_seq_utile = 0u64;
    let mut seq = 0u64;

    for entree in journal {
        match entree {
            EntreeJournal::UiAction {
                monotone_ms,
                genre,
                unresolved,
                ..
            } => {
                let Some(action) = action_de(genre) else {
                    continue;
                };
                let cible = genre.cible();
                let (role, nom, region) = match cible {
                    Some(c) => (
                        if c.role.trim().is_empty() {
                            "generic".to_string()
                        } else {
                            c.role.clone()
                        },
                        if *unresolved || c.nom.trim().is_empty() {
                            NOM_NON_RESOLU.to_string()
                        } else {
                            c.nom.clone()
                        },
                        c.region.clone(),
                    ),
                    // Copie, collage, bascule : pas de cible, mais un geste.
                    None => ("generic".to_string(), action.to_string(), None),
                };

                if touche_une_valeur(action) && nom != NOM_NON_RESOLU && !champs.contains(&nom) {
                    champs.push(nom.clone());
                }
                if nom != NOM_NON_RESOLU && !noms_touches.contains(&nom) {
                    noms_touches.push(nom.clone());
                }

                seq += 1;
                dernier_seq_utile = seq;
                evenements.push(Evenement::UiAction {
                    schema_v: SCHEMA_V,
                    seq,
                    ts: mural(*monotone_ms),
                    source: "ui".to_string(),
                    action: action.to_string(),
                    target: Cible {
                        role,
                        name: nom,
                        region,
                    },
                    // R2.3 exige `paste{paired:false}` : que le collage dise
                    // d'ou vient ce qu'on colle. Le format n'a pas de champ
                    // `paired` — `payload` porte donc un marqueur, deux mots
                    // ASCII, jamais du contenu. Ajouter un champ au format est
                    // une decision de spec 001, a prendre au gate ; perdre
                    // l'information en attendant n'en est pas une.
                    payload: match genre {
                        GenreEvenement::Collage { apparie } => {
                            Some(if *apparie { "paired" } else { "unpaired" }.to_string())
                        }
                        _ => None,
                    },
                });
            }

            EntreeJournal::Gap {
                monotone_ms, cause, ..
            } => {
                seq += 1;
                trous += 1;
                evenements.push(Evenement::Gap {
                    schema_v: SCHEMA_V,
                    seq,
                    ts: mural(*monotone_ms),
                    source: "system".to_string(),
                    gap: Trou {
                        cause: *cause,
                        // Le trou s'ouvre après la dernière action utile et se
                        // ferme sur lui-même : sans plan API, on ne sait pas ce
                        // qui aurait dû s'y passer.
                        from_seq: dernier_seq_utile,
                        to_seq: seq,
                    },
                });
            }

            // Déclencheurs, snapshots et clôture appartiennent au film. Le récit
            // n'en a pas besoin : le premier est une raison de photographier, le
            // deuxième une photo, le troisième une borne déjà portée par `t1`.
            // R5.4 : ce que l'episode n'a pas vu se compte, mais ne se raconte
            // pas. La completude porte le nombre ; la frise n'a rien a montrer,
            // parce qu'il n'y a rien eu a observer.
            EntreeJournal::HorsPerimetre { combien, .. } => {
                hors_perimetre += combien;
            }

            // R7.2 : celle-ci, en revanche, se raconte. Elle explique pourquoi
            // la suite de l'episode est moins fournie, et sans elle on lirait
            // cette pauvrete comme une propriete du travail observe.
            EntreeJournal::Degradation {
                monotone_ms, quoi, ..
            } => {
                seq += 1;
                dernier_seq_utile = seq;
                evenements.push(Evenement::Degraded {
                    schema_v: SCHEMA_V,
                    seq,
                    ts: mural(*monotone_ms),
                    source: "system".to_string(),
                    degraded: quoi.clone(),
                });
            }

            // Déclencheurs, snapshots et clôture appartiennent au film. Le récit
            // n'en a pas besoin : le premier est une raison de photographier, le
            // deuxième une photo, le troisième une borne déjà portée par `t1`.
            EntreeJournal::Declencheur { .. }
            | EntreeJournal::Snapshot { .. }
            | EntreeJournal::ClotureAuto { .. } => {}
        }
    }

    if evenements
        .iter()
        .all(|e| matches!(e, Evenement::Gap { .. }))
    {
        return Err(Quarantaine::AucuneAction);
    }

    // R2.2 : le périmètre est ce que l'opérateur a effectivement touché. À
    // défaut de champ de valeur, on retombe sur les cibles nommées — un épisode
    // de pure navigation a quand même un périmètre observé.
    let scope_fields = if !champs.is_empty() {
        champs
    } else if !noms_touches.is_empty() {
        noms_touches.clone()
    } else {
        return Err(Quarantaine::AucuneAction);
    };

    // Spec 002 : aucune entité n'est résolue, faute de connecteur API — c'est la
    // spec 003 qui les résout. L'entité existe quand même, sans état : c'est
    // exactement ce que « grade B, entités non résolues » veut dire, et le gate
    // de cette spec l'attend.
    let entities: Vec<Entite> = noms_touches
        .first()
        .map(|nom| {
            vec![Entite {
                key: CleEntite {
                    type_entite: "capture".to_string(),
                    value_pseudo: redacteur.jeton("CIBLE", nom),
                },
                first_seen_seq: 1,
                api_refs: Vec::new(),
                state_before: None,
                state_after: None,
                resolved: None,
                state_meta: None,
            }]
        })
        .unwrap_or_default();

    let explained = evenements
        .iter()
        .filter(|e| matches!(e, Evenement::UiAction { .. }))
        .count() as u64;

    let mut episode = Episode {
        schema_v: SCHEMA_V,
        id: id.to_string(),
        task_slug: task_slug.to_string(),
        t0: horodater(t0_ms),
        t1: horodater(t1_ms),
        events: evenements,
        entities,
        grade: String::new(),
        grade_reason: String::new(),
        scope_fields,
        completeness: Completude {
            explained,
            out_of_scope: hors_perimetre,
            gaps: trous,
        },
        supersedes: None,
    };

    let (grade, raison) = grade_de(&episode);
    episode.grade = grade;
    episode.grade_reason = raison;
    Ok(episode)
}

/// Miroir exact de `gradeOf` (spec 001, R2.1).
///
/// Le harness recalcule le grade à la lecture et **refuse** l'épisode s'il
/// diffère du grade déclaré. Une divergence entre ces deux implémentations ne
/// produit donc pas un mauvais grade : elle produit un refus. C'est le bon
/// comportement — mieux vaut un épisode rejeté qu'un corpus dont les grades ne
/// veulent rien dire — mais ça n'excuse pas la divergence, et un test croisé la
/// surveille.
/// Toutes les chaînes d'une valeur JSON, clés comprises.
///
/// Sert au filet du juge : il s'applique champ par champ, jamais sur l'objet
/// entier. Miroir de la fonction `chaines` de `redaction.ts`.
fn chaines(valeur: &serde_json::Value) -> Vec<String> {
    let mut vues = Vec::new();
    fn descendre(v: &serde_json::Value, vues: &mut Vec<String>) {
        match v {
            serde_json::Value::String(s) => vues.push(s.clone()),
            serde_json::Value::Array(a) => {
                for x in a {
                    descendre(x, vues);
                }
            }
            serde_json::Value::Object(o) => {
                for (k, x) in o {
                    vues.push(k.clone());
                    descendre(x, vues);
                }
            }
            _ => {}
        }
    }
    descendre(valeur, &mut vues);
    vues
}

pub fn grade_de(episode: &Episode) -> (String, String) {
    let gaps = episode
        .events
        .iter()
        .filter(|e| matches!(e, Evenement::Gap { .. }))
        .count();
    let non_resolues = episode.entities.iter().filter(|e| !e.resolue()).count();

    // Deux conditions distinctes, souvent confondues.
    //
    // La première est structurelle : une clé d'entité porte `value_pseudo`,
    // jamais la valeur réelle. Une clé vide signale que la pseudonymisation n'a
    // pas tourné.
    if episode
        .entities
        .iter()
        .any(|e| e.key.value_pseudo.trim().is_empty())
    {
        return (
            "C".into(),
            "redaction non validee : une cle d entite est vide".into(),
        );
    }

    // La seconde est celle que R4.6 définit mécaniquement : zéro motif PII dans
    // l'épisode entièrement sérialisé — plus le filet, champ par champ.
    //
    // **Le filet ne partage pas les motifs**, et c'est tout l'intérêt. Un juge
    // adossé à la bibliothèque qui a servi à redacter est aveugle par
    // construction : tout trou de motif passe deux fois, à l'écriture puis à la
    // validation, et l'épisode ressort gradé « redaction validée ». Trois fois
    // que ça arrive — D24, puis les graphies `(0)` et insécable.
    //
    // Champ par champ et jamais sur l'objet sérialisé : en compactant un JSON
    // entier, les chiffres de deux champs voisins se colleraient et
    // fabriqueraient des numéros que personne n'a écrits. Un faux positif ici
    // déclasse un épisode honnête sans recours.
    if let Ok(serialise) = serde_json::to_string(episode) {
        let mut occurrences = crate::motifs::chercher(&serialise);
        if let Ok(valeur) = serde_json::to_value(episode) {
            for c in chaines(&valeur) {
                occurrences.extend(crate::motifs::chercher_compact(&c));
            }
        }
        if !occurrences.is_empty() {
            let mut par_type: std::collections::BTreeMap<&str, usize> =
                std::collections::BTreeMap::new();
            for o in &occurrences {
                *par_type.entry(o.type_pii.as_str()).or_default() += 1;
            }
            let resume = par_type
                .iter()
                .map(|(t, n)| format!("{n}×{t}"))
                .collect::<Vec<_>>()
                .join(", ");
            return (
                "C".into(),
                format!("redaction non validee : {resume} dans l episode serialise"),
            );
        }
    }

    // INVARIANT 7 — bornes confirmées API. Neutralisé jusqu'à la spec 003 (D5).
    let bornes_confirmees =
        !CONFIRMATION_API_VERIFIABLE || episode.entities.iter().all(|e| !e.api_refs.is_empty());

    if gaps == 0 && non_resolues == 0 && bornes_confirmees {
        return (
            "A".into(),
            "sequence sans trou, toutes entites resolues, redaction validee".into(),
        );
    }
    if gaps == 0 && non_resolues == 0 {
        return (
            "B".into(),
            "declasse en B : bornes non confirmees par API".into(),
        );
    }

    // **Au plus UN défaut** pour rester en B. C'est le seuil que le harness
    // applique, et je l'avais mal mirroré : un trou ET une entité non résolue
    // font deux défauts, donc C. Le test croisé l'a trouvé avant qu'un seul
    // épisode réel ne soit produit.
    let defauts = gaps + non_resolues;
    if defauts <= 1 {
        let quoi = if gaps == 1 {
            "1 trou de capture"
        } else {
            "1 entite non resolue"
        };
        return ("B".into(), format!("declasse en B : {quoi}"));
    }
    (
        "C".into(),
        format!("declasse en C : {gaps} trou(s) et {non_resolues} entite(s) non resolue(s)"),
    )
}

/// INVARIANT 7 — **allumé par la spec 003** (D5, D34).
///
/// Miroir de la constante homonyme d'`episode-spec`, et les deux doivent
/// basculer ensemble : un capteur qui graderait A ce que le harness grade B
/// produirait des épisodes que le juge refuse, sans que personne comprenne
/// pourquoi. Un test croisé compare les deux sur dix vecteurs.
///
/// Elle valait `false` tant qu'aucun connecteur n'existait : exiger des
/// `api_refs` que rien ne pouvait produire aurait interdit le grade A à tout le
/// corpus, et un invariant qu'on ne peut pas satisfaire ne protège de rien — il
/// se contourne.
const CONFIRMATION_API_VERIFIABLE: bool = true;

/// Écrit l'épisode et le rend **immuable** (R1.4).
///
/// « Les épisodes sont immuables » est la quatrième des cinq règles du projet.
/// Le fichier reçoit donc l'attribut lecture seule : ce n'est pas une sécurité
/// contre un adversaire — qui peut l'enlever — mais contre l'accident, qui est
/// le vrai risque. Un épisode modifié après coup invaliderait tout le corpus
/// sans qu'on puisse dire lequel.
pub fn persister(
    racine: &std::path::Path,
    episode: &Episode,
) -> std::io::Result<std::path::PathBuf> {
    let dossier = racine.join(&episode.id);
    std::fs::create_dir_all(&dossier)?;
    let chemin = dossier.join("episode.json");

    // INVARIANT IV — un épisode déjà écrit ne se réécrit pas. L'attribut lecture
    // seule posé plus bas fait échouer la seconde écriture, et un test le montre.
    //
    // **Un seul chemin lève cet attribut** : `panique::rendre_inscriptible`, et
    // il ne réécrit rien — il efface. Effacer sur ordre de l'opérateur et
    // réécrire en douce ne sont pas le même geste, et la distinction vaut d'être
    // écrite ici plutôt que laissée à qui relira.
    //
    // L'attribut ne protège pas d'un adversaire ; il protège de l'accident, qui
    // est le vrai risque.
    let json = serde_json::to_string_pretty(episode)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&chemin, json)?;

    let mut perms = std::fs::metadata(&chemin)?.permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&chemin, perms)?;
    Ok(chemin)
}

/// Conserve un épisode qu'on n'a pas su assembler, avec sa raison (R1.4).
///
/// **Jamais silencieusement jeté.** Un épisode qu'on jette est un épisode qu'on
/// ne peut pas diagnostiquer : la panne se répétera et on n'aura rien pour la
/// comprendre.
pub fn mettre_en_quarantaine(
    racine: &std::path::Path,
    id: &str,
    raison: &str,
) -> std::io::Result<std::path::PathBuf> {
    let dossier = racine.join("quarantaine").join(id);
    std::fs::create_dir_all(&dossier)?;
    let chemin = dossier.join("raison.txt");
    std::fs::write(&chemin, raison)?;
    Ok(chemin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cle::CleHmac;
    use crate::source::{Cible as CibleSource, Source};

    fn redacteur() -> Redacteur {
        Redacteur::new(&CleHmac::generer().expect("alea"))
    }

    /// 2026-01-01T00:00:00.000Z
    const T0: u64 = 1_767_225_600_000;

    fn action(seq: u64, ms: u64, genre: GenreEvenement) -> EntreeJournal {
        EntreeJournal::UiAction {
            seq,
            monotone_ms: ms,
            source: Source::Uia,
            genre,
            unresolved: false,
        }
    }

    fn cible(role: &str, nom: &str) -> CibleSource {
        CibleSource::new(role, nom)
    }

    fn journal_ordinaire() -> Vec<EntreeJournal> {
        vec![
            action(1, 0, GenreEvenement::Focus(cible("tab", "Details"))),
            action(
                2,
                500,
                GenreEvenement::Saisie(cible("textbox", "Description")),
            ),
            action(
                3,
                1_500,
                GenreEvenement::Soumission(cible("button", "Enregistrer")),
            ),
        ]
    }

    // -- Horodatage ----------------------------------------------------------

    #[test]
    fn l_horodatage_est_iso_8601_en_utc() {
        assert_eq!(horodater(T0), "2026-01-01T00:00:00.000Z");
        assert_eq!(horodater(T0 + 3_661_042), "2026-01-01T01:01:01.042Z");
    }

    #[test]
    fn l_horodatage_traverse_les_annees_bissextiles() {
        // 2028 est bissextile : le 29 fevrier doit exister.
        let bissextile = 1_835_395_200_000u64; // 2028-02-29T00:00:00Z
        assert_eq!(horodater(bissextile), "2028-02-29T00:00:00.000Z");
    }

    // -- Le récit et le film ne contiennent pas la même chose -----------------

    #[test]
    fn le_hors_perimetre_remonte_dans_la_completude() {
        // R5.4 : l'episode dit combien d'actions il n'a pas vues. Le compte, et
        // rien d'autre — ni quand, ni ou, ni quoi. Sans lui, un episode de deux
        // actions passerait pour complet alors que le travail s'est fait
        // ailleurs.
        let mut j = journal_ordinaire();
        j.push(EntreeJournal::HorsPerimetre {
            seq: 9,
            monotone_ms: 2_500,
            combien: 7,
        });
        let ep = assembler("01TEST", "t", T0, T0 + 3_000, &j, &redacteur()).unwrap();
        assert_eq!(ep.completeness.out_of_scope, 7);
        assert_eq!(ep.completeness.gaps, 0, "un refus n'est pas un trou");

        // Et il ne raconte rien : le recit ne porte aucun evenement de plus.
        let texte = serde_json::to_string(&ep).unwrap();
        assert!(!texte.contains("hors_perimetre"), "{texte}");
    }

    #[test]
    fn sans_refus_la_completude_hors_perimetre_reste_a_zero() {
        let ep = assembler(
            "01TEST",
            "t",
            T0,
            T0 + 3_000,
            &journal_ordinaire(),
            &redacteur(),
        )
        .unwrap();
        assert_eq!(ep.completeness.out_of_scope, 0);
    }

    #[test]
    fn un_re_rendu_de_conteneur_n_entre_pas_dans_le_recit() {
        // L'application a bouge, pas l'operateur. Le faire figurer comme une
        // action lui attribuerait une decision qu'il n'a pas prise.
        let mut j = journal_ordinaire();
        j.push(action(
            4,
            2_000,
            GenreEvenement::ChangementStructure(cible("generic", "Panneau")),
        ));
        let ep = assembler("01TEST", "t", T0, T0 + 3_000, &j, &redacteur()).unwrap();
        assert_eq!(ep.events.len(), 3, "{:?}", ep.events);
    }

    #[test]
    fn les_declencheurs_et_les_snapshots_restent_dans_le_film() {
        let mut j = journal_ordinaire();
        j.push(EntreeJournal::Declencheur {
            seq: 9,
            monotone_ms: 1_600,
            quoi: crate::moteur::Declencheur::Soumission,
        });
        let ep = assembler("01TEST", "t", T0, T0 + 3_000, &j, &redacteur()).unwrap();
        assert_eq!(ep.events.len(), 3);
    }

    #[test]
    fn chaque_geste_a_son_action_du_schema() {
        // Le schema ferme l'enum : un geste sans traduction ferait refuser
        // l'episode par le harness, et on ne le saurait qu'a la lecture.
        for (genre, attendu) in [
            (GenreEvenement::Invocation(cible("button", "X")), "invoke"),
            (GenreEvenement::Saisie(cible("textbox", "X")), "input"),
            (
                GenreEvenement::ChangementValeur(cible("combobox", "X")),
                "toggle",
            ),
            (GenreEvenement::Soumission(cible("button", "X")), "submit"),
            (GenreEvenement::Focus(cible("tab", "X")), "navigate"),
            (GenreEvenement::Copie, "copy"),
            (GenreEvenement::Collage { apparie: true }, "paste"),
        ] {
            assert_eq!(action_de(&genre), Some(attendu), "{genre:?}");
        }
    }

    // -- Séquences et bornes -------------------------------------------------

    #[test]
    fn les_seq_du_recit_sont_strictement_croissants() {
        // Le schema le verifie et refuse sinon. Les seq du film ne conviennent
        // pas : le recit en ecarte des entrees, donc ses numeros sont a lui.
        let ep = assembler(
            "01TEST",
            "t",
            T0,
            T0 + 3_000,
            &journal_ordinaire(),
            &redacteur(),
        )
        .unwrap();
        for paire in ep.events.windows(2) {
            assert!(paire[1].seq() > paire[0].seq());
        }
        assert_eq!(ep.events[0].seq(), 1, "la numerotation commence a 1");
    }

    #[test]
    fn tous_les_instants_tombent_entre_t0_et_t1() {
        // Le schema refuse un `ts` hors bornes.
        let ep = assembler(
            "01TEST",
            "t",
            T0,
            T0 + 3_000,
            &journal_ordinaire(),
            &redacteur(),
        )
        .unwrap();
        assert_eq!(ep.t0, horodater(T0));
        assert_eq!(ep.t1, horodater(T0 + 3_000));
        for e in &ep.events {
            let ts = match e {
                Evenement::UiAction { ts, .. }
                | Evenement::ApiChange { ts, .. }
                | Evenement::Gap { ts, .. }
                | Evenement::Degraded { ts, .. } => ts,
            };
            assert!(ts.as_str() >= ep.t0.as_str(), "{ts} avant t0");
            assert!(ts.as_str() <= ep.t1.as_str(), "{ts} apres t1");
        }
    }

    #[test]
    fn un_instant_au_dela_de_t1_est_ramene_a_la_borne() {
        // Un evenement date apres la cloture ferait refuser l'episode entier.
        // Le ramener vaut mieux que perdre tout le reste.
        let j = [action(
            1,
            999_999,
            GenreEvenement::Invocation(cible("button", "X")),
        )];
        let ep = assembler("01TEST", "t", T0, T0 + 1_000, &j, &redacteur()).unwrap();
        match &ep.events[0] {
            Evenement::UiAction { ts, .. } => assert_eq!(*ts, ep.t1),
            autre => panic!("{autre:?}"),
        }
    }

    // -- Grade ---------------------------------------------------------------

    #[test]
    fn un_episode_de_la_spec_002_sort_en_b_entites_non_resolues() {
        // C'est LE gate de cette spec : sans connecteur API, aucune entite ne
        // peut etre resolue, et l'episode doit le dire.
        let ep = assembler(
            "01TEST",
            "t",
            T0,
            T0 + 3_000,
            &journal_ordinaire(),
            &redacteur(),
        )
        .unwrap();
        assert_eq!(ep.grade, "B");
        assert!(
            ep.grade_reason.contains("non resolue"),
            "{}",
            ep.grade_reason
        );
        assert_eq!(ep.entities.len(), 1);
        assert!(ep.entities[0].api_refs.is_empty());
    }

    #[test]
    fn un_trou_apparait_dans_la_completude_et_dans_la_raison() {
        let mut j = journal_ordinaire();
        j.push(EntreeJournal::Gap {
            seq: 4,
            monotone_ms: 2_000,
            cause: CauseGap::Sleep,
            debut_ms: 1_800,
            fin_ms: 2_000,
        });
        let ep = assembler("01TEST", "t", T0, T0 + 3_000, &j, &redacteur()).unwrap();
        assert_eq!(ep.completeness.gaps, 1);
        assert!(ep.grade_reason.contains("trou"), "{}", ep.grade_reason);
    }

    #[test]
    fn une_pii_qui_survivrait_ferait_tomber_le_grade_en_c() {
        // Le controle de derniere ligne : meme si la redaction amont a echoue,
        // l'episode ne peut pas sortir en A ou B avec une PII dedans.
        let mut ep = assembler(
            "01TEST",
            "t",
            T0,
            T0 + 3_000,
            &journal_ordinaire(),
            &redacteur(),
        )
        .unwrap();
        if let Evenement::UiAction { target, .. } = &mut ep.events[0] {
            target.name = "jean.dupont@exemple.fr".into();
        }
        let (grade, raison) = grade_de(&ep);
        assert_eq!(grade, "C");
        assert!(raison.contains("EMAIL"), "{raison}");
    }

    #[test]
    fn un_episode_assemble_ne_contient_aucune_pii() {
        let j = [
            action(
                1,
                0,
                GenreEvenement::Saisie(cible("textbox", "Contact jean@exemple.fr")),
            ),
            action(
                2,
                100,
                GenreEvenement::Soumission(cible("button", "Envoyer")),
            ),
        ];
        // Le journal arrive DEJA redacte du moteur ; ici on verifie que
        // l'assemblage n'en reintroduit pas.
        let r = redacteur();
        let redacte: Vec<EntreeJournal> = j
            .iter()
            .map(|e| match e {
                EntreeJournal::UiAction {
                    seq,
                    monotone_ms,
                    source,
                    genre,
                    unresolved,
                } => EntreeJournal::UiAction {
                    seq: *seq,
                    monotone_ms: *monotone_ms,
                    source: *source,
                    genre: r.redacter_genre(genre),
                    unresolved: *unresolved,
                },
                autre => autre.clone(),
            })
            .collect();

        let ep = assembler("01TEST", "t", T0, T0 + 1_000, &redacte, &r).unwrap();
        let serialise = serde_json::to_string(&ep).unwrap();
        assert!(
            crate::motifs::chercher(&serialise).is_empty(),
            "{serialise}"
        );
        assert_ne!(ep.grade, "C", "{}", ep.grade_reason);
    }

    // -- Quarantaine (R1.4) --------------------------------------------------

    #[test]
    fn un_episode_sans_aucune_action_part_en_quarantaine() {
        // Il n'y a pas de recit a ecrire. Le jeter le rendrait indiagnosticable ;
        // la quarantaine le conserve avec sa raison.
        let j = vec![EntreeJournal::Gap {
            seq: 1,
            monotone_ms: 0,
            cause: CauseGap::Crash,
            debut_ms: 0,
            fin_ms: 0,
        }];
        assert_eq!(
            assembler("01TEST", "t", T0, T0 + 10, &j, &redacteur()),
            Err(Quarantaine::AucuneAction)
        );
    }

    #[test]
    fn un_journal_vide_part_en_quarantaine() {
        assert_eq!(
            assembler("01TEST", "t", T0, T0 + 10, &[], &redacteur()),
            Err(Quarantaine::AucuneAction)
        );
    }

    #[test]
    fn des_bornes_incoherentes_partent_en_quarantaine() {
        assert_eq!(
            assembler(
                "01TEST",
                "t",
                T0 + 100,
                T0,
                &journal_ordinaire(),
                &redacteur()
            ),
            Err(Quarantaine::BornesIncoherentes)
        );
    }

    // -- Périmètre -----------------------------------------------------------

    #[test]
    fn le_perimetre_est_ce_que_l_operateur_a_touche() {
        let ep = assembler(
            "01TEST",
            "t",
            T0,
            T0 + 3_000,
            &journal_ordinaire(),
            &redacteur(),
        )
        .unwrap();
        assert!(ep.scope_fields.contains(&"Description".to_string()));
        assert!(!ep.scope_fields.is_empty(), "le schema exige au moins un");
    }

    #[test]
    fn un_episode_de_pure_navigation_a_quand_meme_un_perimetre() {
        // Le schema exige `scope_fields` non vide. Une navigation sans saisie
        // aurait sinon un episode irrecevable.
        let j = vec![action(1, 0, GenreEvenement::Focus(cible("tab", "Details")))];
        let ep = assembler("01TEST", "t", T0, T0 + 100, &j, &redacteur()).unwrap();
        assert_eq!(ep.scope_fields, vec!["Details".to_string()]);
    }

    #[test]
    fn une_cible_non_resolue_garde_sa_place_mais_pas_le_perimetre() {
        // R2.4 : l'evenement existe. Mais un nom qu'on n'a pas su lire ne peut
        // pas definir le perimetre de la tache.
        let j = vec![
            EntreeJournal::UiAction {
                seq: 1,
                monotone_ms: 0,
                source: Source::Uia,
                genre: GenreEvenement::Saisie(cible("textbox", "")),
                unresolved: true,
            },
            action(2, 100, GenreEvenement::Saisie(cible("textbox", "Note"))),
        ];
        let ep = assembler("01TEST", "t", T0, T0 + 1_000, &j, &redacteur()).unwrap();
        assert_eq!(ep.events.len(), 2, "l evenement non resolu reste");
        match &ep.events[0] {
            Evenement::UiAction { target, .. } => assert_eq!(target.name, NOM_NON_RESOLU),
            autre => panic!("{autre:?}"),
        }
        assert_eq!(ep.scope_fields, vec!["Note".to_string()]);
    }

    // ---------------------------------------------------------------------
    // Le seuil de grade, compare a TypeScript sur les memes entrees.
    // ---------------------------------------------------------------------

    #[derive(serde::Deserialize)]
    struct EntiteVecteur {
        resolue: bool,
        pseudo: Option<String>,
    }

    #[derive(serde::Deserialize)]
    struct CasGrade {
        gaps: usize,
        entites: Vec<EntiteVecteur>,
        grade: String,
        reason: String,
    }

    #[derive(serde::Deserialize)]
    struct VecteursGrade {
        cas: Vec<CasGrade>,
    }

    const VECTEURS_GRADE: &str =
        include_str!("../../../../packages/episode-spec/vecteurs-grade.json");

    fn episode_synthetique(cas: &CasGrade) -> Episode {
        let mut events: Vec<Evenement> = (0..cas.gaps)
            .map(|i| Evenement::Gap {
                schema_v: SCHEMA_V,
                seq: i as u64 + 1,
                ts: horodater(T0),
                source: "system".into(),
                gap: Trou {
                    cause: CauseGap::Sleep,
                    from_seq: 0,
                    to_seq: i as u64 + 1,
                },
            })
            .collect();
        events.push(Evenement::UiAction {
            schema_v: SCHEMA_V,
            seq: 900,
            ts: horodater(T0),
            source: "ui".into(),
            action: "invoke".into(),
            target: Cible {
                role: "button".into(),
                name: "X".into(),
                region: None,
            },
            payload: None,
        });

        Episode {
            schema_v: SCHEMA_V,
            id: "01TEST".into(),
            task_slug: "t".into(),
            t0: horodater(T0),
            t1: horodater(T0 + 1000),
            events,
            entities: cas
                .entites
                .iter()
                .enumerate()
                .map(|(i, e)| Entite {
                    key: CleEntite {
                        type_entite: "capture".into(),
                        value_pseudo: e
                            .pseudo
                            .clone()
                            .unwrap_or_else(|| format!("CIBLE_{i}0000000")),
                    },
                    first_seen_seq: 1,
                    api_refs: Vec::new(),
                    state_before: e.resolue.then(|| serde_json::json!({})),
                    state_after: e.resolue.then(|| serde_json::json!({})),
                    resolved: None,
                    state_meta: None,
                })
                .collect(),
            grade: String::new(),
            grade_reason: String::new(),
            scope_fields: vec!["X".into()],
            completeness: Completude {
                explained: 1,
                out_of_scope: 0,
                gaps: cas.gaps as u64,
            },
            supersedes: None,
        }
    }

    /// LE test qui aurait trouve le defaut avant qu'un episode reel ne le fasse.
    ///
    /// Le seuil de `gradeOf` — au plus UN defaut pour rester en B — avait ete
    /// mal miroite au premier essai. Le harness avait refuse l'episode, ce qui
    /// est le bon comportement ; mais la divergence ne s'etait vue qu'en
    /// produisant un episode complet et en le lui soumettant. Ces vecteurs la
    /// font voir en CI, sur dix cas.
    #[test]
    fn le_seuil_de_grade_est_le_meme_qu_en_typescript() {
        let v: VecteursGrade = serde_json::from_str(VECTEURS_GRADE).expect("vecteurs-grade.json");
        assert!(v.cas.len() >= 8, "trop peu de cas : {}", v.cas.len());

        let mut desaccords = Vec::new();
        for cas in &v.cas {
            let (grade, raison) = grade_de(&episode_synthetique(cas));
            if grade != cas.grade || raison != cas.reason {
                desaccords.push(format!(
                    "  gaps={} entites={}\n    TypeScript : {} — {}\n    Rust       : {grade} — {raison}",
                    cas.gaps,
                    cas.entites.len(),
                    cas.grade,
                    cas.reason
                ));
            }
        }
        assert!(
            desaccords.is_empty(),
            "{} desaccord(s) sur le grade :\n{}",
            desaccords.len(),
            desaccords.join("\n")
        );
    }

    #[test]
    fn deux_defauts_font_tomber_en_c() {
        // Le seuil, isole. Un trou ET une entite non resolue ne font pas un B
        // indulgent : ils font deux defauts, donc C.
        let mut j = journal_ordinaire();
        j.push(EntreeJournal::Gap {
            seq: 9,
            monotone_ms: 2_000,
            cause: CauseGap::Sleep,
            debut_ms: 1_900,
            fin_ms: 2_000,
        });
        let ep = assembler("01TEST", "t", T0, T0 + 3_000, &j, &redacteur()).unwrap();
        assert_eq!(ep.grade, "C", "{}", ep.grade_reason);
    }

    #[test]
    fn un_seul_defaut_reste_en_b() {
        // Le gate de la spec 002 : zero trou, une entite non resolue.
        let ep = assembler(
            "01TEST",
            "t",
            T0,
            T0 + 3_000,
            &journal_ordinaire(),
            &redacteur(),
        )
        .unwrap();
        assert_eq!(ep.grade, "B");
        assert_eq!(ep.grade_reason, "declasse en B : 1 entite non resolue");
    }

    // -- Persistance (R1.4) --------------------------------------------------

    fn racine_test(nom: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("noe-ep-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn un_episode_persiste_est_immuable() {
        // Quatrieme regle du projet. L attribut lecture seule ne protege pas
        // d un adversaire — il protege de l accident, qui est le vrai risque.
        let r = racine_test("immuable");
        let ep = assembler(
            "01TESTIMMUABLE",
            "t",
            T0,
            T0 + 3_000,
            &journal_ordinaire(),
            &redacteur(),
        )
        .unwrap();
        let chemin = persister(&r, &ep).unwrap();

        assert!(chemin.exists());
        assert!(
            std::fs::metadata(&chemin).unwrap().permissions().readonly(),
            "l episode doit etre en lecture seule"
        );
        assert!(
            std::fs::write(&chemin, "modifie").is_err(),
            "une reecriture doit echouer"
        );
    }

    #[test]
    fn persister_deux_fois_le_meme_episode_echoue() {
        // Le test voisin ecrit a la main dans le fichier ; celui-ci refait le
        // geste qu'on ferait par accident — rappeler `persister`, par exemple
        // apres une reprise d'episode orphelin. Rien ne doit passer.
        let r = racine_test("deux-fois");
        let ep = assembler(
            "01TESTDEUXFOIS",
            "t",
            T0,
            T0 + 3_000,
            &journal_ordinaire(),
            &redacteur(),
        )
        .unwrap();
        assert!(persister(&r, &ep).is_ok());
        assert!(
            persister(&r, &ep).is_err(),
            "un second passage a reecrit l episode"
        );
    }

    #[test]
    fn l_episode_persiste_se_relit_a_l_identique() {
        let r = racine_test("relecture");
        let ep = assembler(
            "01TESTRELECTURE",
            "t",
            T0,
            T0 + 3_000,
            &journal_ordinaire(),
            &redacteur(),
        )
        .unwrap();
        let chemin = persister(&r, &ep).unwrap();

        let relu: Episode =
            serde_json::from_str(&std::fs::read_to_string(&chemin).unwrap()).unwrap();
        assert_eq!(relu, ep, "l aller-retour JSON doit etre fidele");
    }

    #[test]
    fn un_episode_en_quarantaine_garde_sa_raison() {
        // Jamais silencieusement jete : la panne se repeterait et on n aurait
        // rien pour la comprendre.
        let r = racine_test("quarantaine");
        let chemin = mettre_en_quarantaine(
            &r,
            "01TESTQUARANTAINE",
            &Quarantaine::AucuneAction.to_string(),
        )
        .unwrap();
        let raison = std::fs::read_to_string(&chemin).unwrap();
        assert!(raison.contains("aucune action"), "{raison}");
    }

    // -- INVARIANT 7 : la confirmation API (spec 003, R7.1) ----------------

    /// Un episode parfait : sans trou, entites resolues, api_refs presentes.
    fn episode_parfait() -> Episode {
        Episode {
            schema_v: SCHEMA_V,
            id: "01JQA1B2C3D4E5F6G7H8J9K0M1".into(),
            task_slug: "maj-crm-post-echange".into(),
            t0: "2026-01-14T09:12:00.000Z".into(),
            t1: "2026-01-14T09:16:42.000Z".into(),
            events: vec![Evenement::UiAction {
                schema_v: SCHEMA_V,
                seq: 1,
                ts: "2026-01-14T09:12:01.000Z".into(),
                source: "ui".into(),
                action: "invoke".into(),
                target: Cible {
                    role: "button".into(),
                    name: "Enregistrer".into(),
                    region: None,
                },
                payload: None,
            }],
            entities: vec![Entite {
                key: CleEntite {
                    type_entite: "contact".into(),
                    value_pseudo: "EMAIL_aaa".into(),
                },
                first_seen_seq: 1,
                api_refs: vec![serde_json::json!({
                    "connector": "crm", "object": "lead", "id": "L-1"
                })],
                state_before: Some(serde_json::json!({ "statut": "nouveau" })),
                state_after: Some(serde_json::json!({ "statut": "qualifie" })),
                resolved: None,
                state_meta: None,
            }],
            grade: String::new(),
            grade_reason: String::new(),
            scope_fields: vec!["statut".into()],
            completeness: Completude {
                explained: 1,
                out_of_scope: 0,
                gaps: 0,
            },
            supersedes: None,
        }
    }

    /// Ce test EST la garde de `CONFIRMATION_API_VERIFIABLE`.
    ///
    /// Une assertion sur la constante elle-meme serait refusee par clippy — a
    /// juste titre : il connait deja son issue. Celle-ci, non : elle verifie un
    /// COMPORTEMENT, et elle rougit si quelqu'un remet la constante a `false`,
    /// parce que l'episode sans `api_refs` redeviendrait un A.
    #[test]
    fn l_invariant_sept_mord_vraiment() {
        // Basculer la constante sans test, c'est changer un chiffre et croire
        // que quelque chose s'est passe : rien n'aurait rougi, et rien ne
        // rougirait le jour ou quelqu'un la remettrait a `false`.
        let parfait = episode_parfait();
        assert_eq!(grade_de(&parfait).0, "A");

        let mut sans_refs = episode_parfait();
        sans_refs.entities[0].api_refs.clear();
        let (grade, raison) = grade_de(&sans_refs);
        assert_eq!(grade, "B", "un A sans api_refs affirme sans avoir verifie");
        assert!(raison.contains("bornes non confirmees"), "{raison}");
    }
}

/// Fait entrer les résultats de la fédération dans l'épisode (spec 003, R3, §7).
///
/// **Une fonction séparée, et pas un paramètre de plus sur `assembler`.** Un
/// épisode repris après un crash n'a pas de résultats de fédération, et ce n'est
/// pas un oubli : le worker est mort avec l'application. Les deux chemins
/// d'assemblage n'ont pas la même matière, et une signature commune les ferait
/// mentir tous les deux.
///
/// La fusion **remplace** les entités de la spec 002. Celles-ci existaient
/// précisément parce que rien ne pouvait être résolu — elles portaient une cible
/// pseudonymisée, sans état ni référence. Le jour où quelque chose peut être
/// résolu, elles n'ont plus de raison d'être, et les garder ferait traîner une
/// entité non résolue qui empêcherait à jamais le grade A.
///
/// Le grade est **recalculé** à la fin (tâche 10) : les règles ne bougent pas,
/// ce sont les entités qui deviennent résolues.
///
/// **Pas encore appelée depuis la clôture.** Le registre qu'elle attend est
/// produit par le worker, que rien ne démarre tant qu'il n'y a pas de jeton.
/// Poser dès maintenant un appel qu'aucun test ne peut exercer serait pire que
/// l'annotation : ce serait un garde-fou décoratif de plus, et ce dépôt en a
/// déjà démasqué trois.
#[allow(dead_code)] // retiré quand la tâche 0 permet de démarrer le worker
pub fn fusionner_federation(
    episode: &mut Episode,
    entites: &std::collections::BTreeMap<String, crate::federation::EntiteFederee>,
) {
    if entites.is_empty() {
        return;
    }

    let mut nouvelles: Vec<Entite> = Vec::new();
    let mut trous = 0_u64;
    for (candidate_id, federee) in entites {
        // L'identifiant de candidate est `<connecteur>:<jeton>` — le jeton, et
        // jamais la valeur claire : il traverse l'épisode et l'épisode est écrit.
        let (type_entite, value_pseudo) = match candidate_id.split_once(':') {
            Some((t, v)) if !t.is_empty() => (t.to_owned(), v.to_owned()),
            // Un identifiant malformé ne fait pas disparaître l'entité : un
            // épisode avec une entité en moins a l'air plus propre et ment. La
            // clé vide sera vue par le grade, qui déclasse en C.
            _ => ("capture".to_owned(), candidate_id.clone()),
        };
        trous += federee.trous.len() as u64;
        nouvelles.push(Entite {
            key: CleEntite {
                type_entite,
                value_pseudo,
            },
            first_seen_seq: federee.first_seen_seq,
            api_refs: federee
                .api_ref
                .as_ref()
                .and_then(|r| serde_json::to_value(r).ok())
                .map(|v| vec![v])
                .unwrap_or_default(),
            state_before: federee
                .state_before
                .as_ref()
                .and_then(|e| serde_json::to_value(e).ok()),
            state_after: federee
                .state_after
                .as_ref()
                .and_then(|e| serde_json::to_value(e).ok()),
            resolved: federee.resolved.clone(),
            state_meta: (!federee.state_meta.is_empty()).then(|| federee.state_meta.clone()),
        });
    }

    episode.entities = nouvelles;
    // R4.3 : les trous déclarés par la fédération comptent dans la complétude.
    // Un trou de lecture qu'on ne compterait pas se lirait comme un système qui
    // n'a rien fait.
    episode.completeness.gaps += trous;

    let (grade, raison) = grade_de(episode);
    episode.grade = grade;
    episode.grade_reason = raison;
}

#[cfg(test)]
mod tests_corpus_dore {
    use super::*;

    /// Les épisodes dorés, tels que le harness les valide avec Zod.
    ///
    /// **Rien ne les lisait côté Rust.** Le type `Episode` a été écrit à la main
    /// d'après le schéma TypeScript, et deux champs lui ont été ajoutés le
    /// 2026-08-27 — sans qu'aucun contrôle ne dise si les deux descriptions
    /// parlent toujours du même objet. Un capteur qui écrirait un épisode que le
    /// harness refuse ne le découvrirait qu'au rejeu, c'est-à-dire après.
    const DORES: &[(&str, &str)] = &[
        (
            "001_nominal",
            include_str!("../../../../packages/harness/golden/001_nominal.json"),
        ),
        (
            "002_branche_alt",
            include_str!("../../../../packages/harness/golden/002_branche_alt.json"),
        ),
        (
            "003_trou",
            include_str!("../../../../packages/harness/golden/003_trou.json"),
        ),
        (
            "004_hors_perimetre",
            include_str!("../../../../packages/harness/golden/004_hors_perimetre.json"),
        ),
        (
            "005_canaris",
            include_str!("../../../../packages/harness/golden/005_canaris.json"),
        ),
    ];

    /// L'episode « tout allume », produit par `scripts/generer-episode-complet.mjs`.
    ///
    /// **Le corpus dore ne suffit pas.** Un controle ne vaut que par ce que ses
    /// vecteurs exercent, et aucun de ses cinq episodes ne porte `resolved`,
    /// `state_meta` ni `supersedes`. Une divergence sur eux serait passee sans
    /// qu'aucun test ne rougisse — et c'est exactement ce qui s'est produit pour
    /// `api_change`, absent du miroir Rust pendant deux specs.
    const COMPLET: &str = include_str!("../../../../packages/episode-spec/episode-complet.json");

    #[test]
    fn l_episode_tout_allume_traverse_le_rust_sans_rien_perdre() {
        let avant: serde_json::Value = serde_json::from_str(COMPLET).expect("json");
        let relu: Episode = serde_json::from_str(COMPLET).expect("episode complet");
        let apres = serde_json::to_value(&relu).expect("re-serialisation");
        let perdus = champs_perdus(&avant, &apres, String::new());
        assert!(
            perdus.is_empty(),
            "le type Rust perd des champs du schema :
{perdus:#?}"
        );
    }

    #[test]
    fn l_episode_tout_allume_exerce_bien_ce_que_le_corpus_n_exerce_pas() {
        // Sans cette assertion, quelqu'un pourrait alleger le fichier genere et
        // le test precedent resterait vert en ne prouvant plus rien.
        let ep: Episode = serde_json::from_str(COMPLET).expect("episode complet");
        assert!(ep.supersedes.is_some(), "supersedes non exerce");
        let e = ep.entities.first().expect("une entite");
        assert!(e.resolved.is_some(), "resolved non exerce");
        assert!(e.state_meta.is_some(), "state_meta non exerce");
        assert!(
            ep.events
                .iter()
                .any(|ev| matches!(ev, Evenement::ApiChange { .. })),
            "api_change non exerce"
        );
        assert!(
            ep.events
                .iter()
                .any(|ev| matches!(ev, Evenement::Degraded { .. })),
            "degraded non exerce"
        );
    }

    #[test]
    fn le_type_rust_lit_tous_les_episodes_dores() {
        for (nom, brut) in DORES {
            serde_json::from_str::<Episode>(brut)
                .unwrap_or_else(|e| panic!("{nom} refuse par le type Rust : {e}"));
        }
    }

    #[test]
    fn un_episode_dore_traverse_le_rust_sans_rien_perdre() {
        // Serde ignore en silence les champs qu'il ne connait pas. Un aller-retour
        // les fait donc DISPARAITRE, et c'est la seule facon de voir qu'ils
        // existaient : comparer les deux JSON champ par champ.
        for (nom, brut) in DORES {
            let avant: serde_json::Value = serde_json::from_str(brut).expect("json");
            let relu: Episode = serde_json::from_str(brut).expect("episode");
            let apres = serde_json::to_value(&relu).expect("re-serialisation");
            let perdus = champs_perdus(&avant, &apres, String::new());
            assert!(
                perdus.is_empty(),
                "{nom} : le type Rust perd des champs du schema :\n{perdus:#?}"
            );
        }
    }

    #[test]
    fn un_changement_collecte_devient_un_evenement_du_schema() {
        // R4.1 : c'est ici que le delta entre dans l'episode. L'evenement doit
        // porter exactement les cles que le schema TypeScript exige — un champ
        // en moins et le harness refuse l'episode entier, au rejeu, c'est-a-dire
        // apres.
        let c = crate::federation::ChangementApi {
            reference: crate::federation::RefApi {
                connector: "salesforce".into(),
                object: "Lead".into(),
                id: "0035g00000LmT4EAAV".into(),
            },
            quand: "2026-08-27T10:12:00.000Z".into(),
            champs: vec!["Status".into()],
            acteur: Some("005AAA".into()),
        };
        let v = serde_json::to_value(c.en_evenement(7, "2026-08-27T10:12:00.000Z")).unwrap();
        let cles: std::collections::BTreeSet<&str> =
            v.as_object().unwrap().keys().map(String::as_str).collect();
        let attendues: std::collections::BTreeSet<&str> = [
            "kind",
            "schema_v",
            "seq",
            "ts",
            "source",
            "connector",
            "object",
            "object_id",
            "fields_changed",
        ]
        .into_iter()
        .collect();
        assert_eq!(cles, attendues, "l evenement ne correspond pas au schema");
        assert_eq!(v["kind"], serde_json::json!("api_change"));
        // La source est `api` et jamais `ui` : c'est ce qui permet a la
        // reconciliation de distinguer ce que le monde a fait de ce que
        // l'operateur a fait.
        assert_eq!(v["source"], serde_json::json!("api"));
        // L'acteur ne descend PAS dans l'evenement : le schema ne le porte pas,
        // et un identifiant d'utilisateur du CRM n'a rien a faire dans un
        // episode. Il sert au tri hors perimetre, en memoire.
        assert!(!v.as_object().unwrap().contains_key("actor"));
        let entier = serde_json::to_string(&v).unwrap();
        assert!(!entier.contains("005AAA"), "l acteur a fuite : {entier}");
    }

    #[test]
    fn la_version_de_schema_est_celle_du_corpus() {
        // `SCHEMA_V` est recopie a la main des deux cotes. Ici il est confronte a
        // ce que le corpus porte VRAIMENT, et pas a une seconde copie.
        for (nom, brut) in DORES {
            let v: serde_json::Value = serde_json::from_str(brut).expect("json");
            assert_eq!(
                v.get("schema_v").and_then(serde_json::Value::as_u64),
                Some(u64::from(SCHEMA_V)),
                "{nom} porte une autre version de schema"
            );
        }
    }

    #[test]
    fn les_grades_du_corpus_sont_ceux_que_ce_juge_recalcule() {
        // Le corpus dore porte un grade ECRIT. Si ce juge-ci en recalcule un
        // autre, les deux implementations ne notent pas pareil — et le rejeu d'un
        // episode reel donnerait un verdict que le harness ne confirmerait pas.
        for (nom, brut) in DORES {
            let ep: Episode = serde_json::from_str(brut).expect("episode");
            let (grade, raison) = grade_de(&ep);
            assert_eq!(
                grade, ep.grade,
                "{nom} : {raison} (ecrit : {})",
                ep.grade_reason
            );
        }
    }

    /// Les chemins presents dans `avant` et absents ou differents dans `apres`.
    fn champs_perdus(
        avant: &serde_json::Value,
        apres: &serde_json::Value,
        chemin: String,
    ) -> Vec<String> {
        let mut perdus = Vec::new();
        match (avant, apres) {
            (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
                for (cle, va) in a {
                    let sous = if chemin.is_empty() {
                        cle.clone()
                    } else {
                        format!("{chemin}.{cle}")
                    };
                    match b.get(cle) {
                        None => perdus.push(sous),
                        Some(vb) => perdus.extend(champs_perdus(va, vb, sous)),
                    }
                }
            }
            (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
                if a.len() != b.len() {
                    perdus.push(format!("{chemin} (longueur {} -> {})", a.len(), b.len()));
                } else {
                    for (i, (va, vb)) in a.iter().zip(b).enumerate() {
                        perdus.extend(champs_perdus(va, vb, format!("{chemin}[{i}]")));
                    }
                }
            }
            (a, b) if a != b => perdus.push(format!("{chemin} ({a} -> {b})")),
            _ => {}
        }
        perdus
    }
}

#[cfg(test)]
mod tests_fusion {
    use super::*;
    use crate::federation::{EntiteFederee, MetaChamp, RefApi};
    use std::collections::BTreeMap;

    /// 2026-01-01T00:00:00.000Z, comme le reste du module.
    const T0: u64 = 1_767_225_600_000;

    fn redacteur() -> Redacteur {
        Redacteur::new(&crate::cle::CleHmac::generer().expect("alea"))
    }

    fn episode_de_base() -> Episode {
        let j = vec![crate::moteur::EntreeJournal::UiAction {
            seq: 1,
            monotone_ms: 10,
            source: crate::source::Source::Uia,
            genre: crate::source::GenreEvenement::Soumission(crate::source::Cible::new(
                "button",
                "Enregistrer",
            )),
            unresolved: false,
        }];
        assembler("01TESTFUSION", "t", T0, T0 + 3_000, &j, &redacteur()).unwrap()
    }

    fn etat(valeur: &str) -> crate::federation::EtatPlat {
        let mut e = crate::federation::EtatPlat::new();
        e.insert("Statut__c".into(), valeur.into());
        e
    }

    fn federee_resolue() -> EntiteFederee {
        EntiteFederee {
            api_ref: Some(RefApi {
                connector: "salesforce".into(),
                object: "Contact".into(),
                id: "0035g00000LmT4EAAV".into(),
            }),
            state_before: Some(etat("Nouveau")),
            state_after: Some(etat("Qualifie")),
            state_meta: BTreeMap::new(),
            resolved: Some(Resolue {
                by: "system_id".into(),
                at: "2026-08-27T10:00:00.000Z".into(),
            }),
            first_seen_seq: 7,
            non_resolue: None,
            trous: Vec::new(),
        }
    }

    fn snapshot(e: EntiteFederee) -> BTreeMap<String, EntiteFederee> {
        let mut m = BTreeMap::new();
        m.insert("salesforce:CIBLE_aaaaaaaaaaaaa".to_owned(), e);
        m
    }

    #[test]
    fn sans_federation_l_episode_ne_bouge_pas() {
        // Un episode repris apres un crash n'a pas de resultats de federation, et
        // ce n'est pas un oubli : le worker est mort avec l'application.
        let mut ep = episode_de_base();
        let avant = ep.clone();
        fusionner_federation(&mut ep, &BTreeMap::new());
        assert_eq!(ep, avant);
    }

    #[test]
    fn une_entite_federee_remplace_le_placeholder_de_la_002() {
        // Le placeholder existait parce que rien ne pouvait etre resolu. Le
        // garder ferait trainer une entite non resolue qui empecherait a jamais
        // le grade A.
        let mut ep = episode_de_base();
        assert_eq!(ep.entities.len(), 1);
        assert!(ep.entities[0].api_refs.is_empty(), "le placeholder");
        fusionner_federation(&mut ep, &snapshot(federee_resolue()));
        assert_eq!(ep.entities.len(), 1);
        assert_eq!(ep.entities[0].key.type_entite, "salesforce");
        assert_eq!(ep.entities[0].key.value_pseudo, "CIBLE_aaaaaaaaaaaaa");
        assert_eq!(ep.entities[0].api_refs.len(), 1);
    }

    #[test]
    fn une_entite_resolue_porte_la_cle_qui_a_tranche() {
        // Sans elle, une resolution fausse est indiagnosticable : on ne sait meme
        // pas ce qui l'a decidee.
        let mut ep = episode_de_base();
        fusionner_federation(&mut ep, &snapshot(federee_resolue()));
        let r = ep.entities[0].resolved.as_ref().expect("resolved");
        assert_eq!(r.by, "system_id");
        assert_eq!(r.at, "2026-08-27T10:00:00.000Z");
    }

    #[test]
    fn le_rang_de_premiere_vue_vient_de_la_capture() {
        // Un compteur interne au worker compterait l'ordre des RESOLUTIONS, qui
        // depend du reseau : deux rejeux du meme episode ne le rendraient pas
        // pareil.
        let mut ep = episode_de_base();
        fusionner_federation(&mut ep, &snapshot(federee_resolue()));
        assert_eq!(ep.entities[0].first_seen_seq, 7);
    }

    #[test]
    fn une_entite_resolue_des_deux_cotes_ouvre_enfin_le_grade_a() {
        // C'est ce que la spec 003 promet, et c'est la premiere fois qu'un
        // episode de ce depot peut l'atteindre : l'INVARIANT 7 exige des
        // `api_refs`, et rien ne pouvait en produire avant la federation.
        let mut ep = episode_de_base();
        assert_eq!(ep.grade, "B", "{}", ep.grade_reason);
        fusionner_federation(&mut ep, &snapshot(federee_resolue()));
        assert_eq!(ep.grade, "A", "{}", ep.grade_reason);
    }

    #[test]
    fn une_entite_sans_api_ref_ne_monte_pas_en_a() {
        // L'INVARIANT 7 : un A sans `api_refs`, c'est un episode qui affirme
        // avoir tout explique sans avoir rien verifie.
        let mut ep = episode_de_base();
        let mut f = federee_resolue();
        f.api_ref = None;
        f.non_resolue = Some("blocked:droits insuffisants".into());
        fusionner_federation(&mut ep, &snapshot(f));
        assert_ne!(ep.grade, "A", "{}", ep.grade_reason);
    }

    #[test]
    fn une_lecture_manquante_laisse_l_entite_non_resolue() {
        // Un etat absent n'est pas un etat vide : le second se lirait comme
        // « tous les champs sont nuls » et ferait inventer des changements.
        let mut ep = episode_de_base();
        let mut f = federee_resolue();
        f.state_before = None;
        fusionner_federation(&mut ep, &snapshot(f));
        assert!(ep.entities[0].state_before.is_none());
        assert_eq!(ep.grade, "B", "{}", ep.grade_reason);
        assert!(
            ep.grade_reason.contains("entite non resolue"),
            "{}",
            ep.grade_reason
        );
    }

    #[test]
    fn les_champs_retires_du_verdict_descendent_dans_l_episode() {
        // §7 : le juge retire les champs `unknown_before` AVEC TRACE. Un retrait
        // sans trace serait un verdict truque, d'autant plus flatteur qu'il
        // regarde moins.
        let mut ep = episode_de_base();
        let mut f = federee_resolue();
        f.state_meta.insert(
            "Description".into(),
            MetaChamp {
                unknown_before: Some(true),
                reason: Some("champ non historise".into()),
                ..MetaChamp::default()
            },
        );
        fusionner_federation(&mut ep, &snapshot(f));
        let meta = ep.entities[0].state_meta.as_ref().expect("state_meta");
        assert_eq!(
            meta["Description"].reason.as_deref(),
            Some("champ non historise")
        );
    }

    #[test]
    fn les_trous_de_la_federation_comptent_dans_la_completude() {
        // Un trou de lecture qu'on ne compterait pas se lirait comme un systeme
        // qui n'a rien fait.
        let mut ep = episode_de_base();
        let avant = ep.completeness.gaps;
        let mut f = federee_resolue();
        f.trous = vec!["quota : 429".into(), "lecture apres : 500".into()];
        fusionner_federation(&mut ep, &snapshot(f));
        assert_eq!(ep.completeness.gaps, avant + 2);
    }

    #[test]
    fn un_identifiant_de_candidate_malforme_ne_fait_pas_disparaitre_l_entite() {
        // Un episode avec une entite en moins a l'air plus propre et ment.
        let mut ep = episode_de_base();
        let mut m = BTreeMap::new();
        m.insert("sansdeuxpoints".to_owned(), federee_resolue());
        fusionner_federation(&mut ep, &m);
        assert_eq!(ep.entities.len(), 1);
        assert_eq!(ep.entities[0].key.value_pseudo, "sansdeuxpoints");
    }

    #[test]
    fn un_etat_non_redacte_fait_tomber_l_episode_en_c() {
        // La preuve que le filet du juge couvre AUSSI les etats federes, et pas
        // seulement les evenements de capture. Un etat qui entrerait en clair
        // ferait de l'episode une fuite gradee A.
        let mut ep = episode_de_base();
        let mut f = federee_resolue();
        let mut sale = crate::federation::EtatPlat::new();
        sale.insert("Email".into(), "jean.dupont@exemple.fr".into());
        f.state_after = Some(sale);
        fusionner_federation(&mut ep, &m_un(f));
        assert_eq!(ep.grade, "C", "{}", ep.grade_reason);
        assert!(ep.grade_reason.contains("redaction"), "{}", ep.grade_reason);
    }

    fn m_un(e: EntiteFederee) -> BTreeMap<String, EntiteFederee> {
        snapshot(e)
    }
}
