//! Ce que la fenêtre a le droit de savoir (spec 002, tâche 8bis — D26).
//!
//! Le squelette traversant lit les **vrais** épisodes du poste. Ce module fait
//! la seule chose que l'interface ne doit pas faire elle-même : décider ce qui
//! sort du disque, et sous quelle forme.
//!
//! **Un résumé, pas l'épisode.** La vue liste des dizaines d'épisodes ; leur
//! envoyer chacun ses centaines d'événements ferait passer des mégaoctets par
//! l'IPC pour afficher trois lignes. Le détail se demande épisode par épisode.
//!
//! **Rien ne sort qui ne soit déjà redacté.** L'épisode sur disque a traversé le
//! pipeline ; ce module ne fait que le relire. Il ne compose aucun texte à
//! partir d'autre chose, et n'a donc aucun moyen de réintroduire ce que la
//! redaction a retiré.

use std::path::Path;

use crate::assemblage::{Episode, Evenement};

/// Une ligne de la liste.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ResumeEpisode {
    pub id: String,
    pub task_slug: String,
    pub t0: String,
    pub t1: String,
    pub grade: String,
    pub grade_reason: String,
    /// Nombre d'actions de l'opérateur.
    pub actions: u64,
    /// Nombre de trous — la mesure la plus parlante de ce qui manque.
    pub trous: u64,
    /// Part des événements qui sont des actions, en pourcentage entier.
    pub completude_pct: u64,
    /// R5.4 : combien d'actions ont eu lieu hors des surfaces activées.
    ///
    /// Affiché parce qu'il change la lecture de tout le reste : quatorze
    /// actions sur une tâche qui en comptait quarante ne dit pas la même chose
    /// selon qu'on a refusé zéro ou vingt-six gestes. L'épisode n'en sait pas
    /// plus — ni où, ni quoi — et la vue n'invente rien.
    pub hors_perimetre: u64,
    pub scope_fields: Vec<String>,
}

/// Un point de la frise, tel que la vue l'affiche.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PointFrise {
    pub seq: u64,
    pub ts: String,
    /// `action` ou `trou`.
    pub genre: String,
    /// Le verbe (`invoke`, `input`…) ou la cause du trou.
    pub quoi: String,
    /// Le nom de la cible, déjà redacté. Vide pour un trou.
    pub cible: String,
    pub region: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DetailEpisode {
    pub resume: ResumeEpisode,
    pub frise: Vec<PointFrise>,
}

/// La complétude affichée : la part des événements qui sont des actions.
///
/// Ce n'est PAS la complétude au sens de la spec 003, qui compare aux états
/// d'API. Tant qu'il n'y a pas de plan API, la seule chose honnête à montrer est
/// « combien de ce qu'on a enregistré est du geste, et combien est du trou ».
/// La vue le nomme comme tel ; la spec 003 remplacera le chiffre par le vrai.
fn completude_pct(actions: u64, trous: u64) -> u64 {
    let total = actions + trous;
    if total == 0 {
        return 0;
    }
    actions * 100 / total
}

pub fn resumer(episode: &Episode) -> ResumeEpisode {
    let actions = episode
        .events
        .iter()
        .filter(|e| matches!(e, Evenement::UiAction { .. }))
        .count() as u64;
    let trous = episode
        .events
        .iter()
        .filter(|e| matches!(e, Evenement::Gap { .. }))
        .count() as u64;

    ResumeEpisode {
        id: episode.id.clone(),
        task_slug: episode.task_slug.clone(),
        t0: episode.t0.clone(),
        t1: episode.t1.clone(),
        grade: episode.grade.clone(),
        grade_reason: episode.grade_reason.clone(),
        actions,
        trous,
        completude_pct: completude_pct(actions, trous),
        hors_perimetre: episode.completeness.out_of_scope,
        scope_fields: episode.scope_fields.clone(),
    }
}

pub fn friser(episode: &Episode) -> Vec<PointFrise> {
    episode
        .events
        .iter()
        .map(|e| match e {
            Evenement::UiAction {
                seq,
                ts,
                action,
                target,
                ..
            } => PointFrise {
                seq: *seq,
                ts: ts.clone(),
                genre: "action".into(),
                quoi: action.clone(),
                cible: target.name.clone(),
                region: target.region.clone(),
            },
            Evenement::Gap { seq, ts, gap, .. } => PointFrise {
                seq: *seq,
                ts: ts.clone(),
                genre: "trou".into(),
                quoi: format!("{:?}", gap.cause).to_lowercase(),
                cible: String::new(),
                region: None,
            },
        })
        .collect()
}

/// Lit les épisodes du poste, du plus récent au plus ancien.
///
/// Un épisode illisible est **sauté sans faire échouer la liste** : un fichier
/// abîmé ne doit pas rendre l'application aveugle à tous les autres. Il reste sur
/// disque, et son absence de la liste est le signal.
pub fn lister(racine: &Path) -> Vec<ResumeEpisode> {
    let Ok(entrees) = std::fs::read_dir(racine) else {
        return Vec::new();
    };
    let mut resumes: Vec<ResumeEpisode> = entrees
        .flatten()
        .filter_map(|e| lire(&e.path().join("episode.json")))
        .map(|ep| resumer(&ep))
        .collect();

    // Du plus récent au plus ancien : c'est ce qu'on vient de faire qui
    // intéresse, pas ce qu'on a fait la semaine dernière.
    resumes.sort_by(|a, b| b.t0.cmp(&a.t0));
    resumes
}

pub fn detail(racine: &Path, id: &str) -> Option<DetailEpisode> {
    let ep = lire(&racine.join(id).join("episode.json"))?;
    Some(DetailEpisode {
        resume: resumer(&ep),
        frise: friser(&ep),
    })
}

fn lire(chemin: &Path) -> Option<Episode> {
    let texte = std::fs::read_to_string(chemin).ok()?;
    serde_json::from_str(&texte).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemblage::{Cible, CleEntite, Completude, Entite, Trou, SCHEMA_V};
    use crate::moteur::CauseGap;

    fn action(seq: u64, verbe: &str, nom: &str) -> Evenement {
        Evenement::UiAction {
            schema_v: SCHEMA_V,
            seq,
            ts: "2026-01-01T00:00:00.000Z".into(),
            source: "ui".into(),
            action: verbe.into(),
            target: Cible {
                role: "button".into(),
                name: nom.into(),
                region: Some("Fiche".into()),
            },
            payload: None,
        }
    }

    fn trou(seq: u64) -> Evenement {
        Evenement::Gap {
            schema_v: SCHEMA_V,
            seq,
            ts: "2026-01-01T00:00:01.000Z".into(),
            source: "system".into(),
            gap: Trou {
                cause: CauseGap::Sleep,
                from_seq: seq - 1,
                to_seq: seq,
            },
        }
    }

    fn episode(id: &str, t0: &str, events: Vec<Evenement>) -> Episode {
        Episode {
            schema_v: SCHEMA_V,
            id: id.into(),
            task_slug: "maj-crm-post-echange".into(),
            t0: t0.into(),
            t1: "2026-01-01T00:05:00.000Z".into(),
            events,
            entities: vec![Entite {
                key: CleEntite {
                    type_entite: "capture".into(),
                    value_pseudo: "CIBLE_abcd1234".into(),
                },
                first_seen_seq: 1,
                api_refs: Vec::new(),
                state_before: None,
                state_after: None,
            }],
            grade: "B".into(),
            grade_reason: "declasse en B : 1 entite non resolue".into(),
            scope_fields: vec!["Description".into()],
            completeness: Completude {
                explained: 2,
                out_of_scope: 0,
                gaps: 0,
            },
        }
    }

    fn racine(nom: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("noe-vue-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn deposer(racine: &Path, ep: &Episode) {
        let d = racine.join(&ep.id);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("episode.json"), serde_json::to_string(ep).unwrap()).unwrap();
    }

    #[test]
    fn le_resume_porte_le_hors_perimetre() {
        // R5.4 : le fondateur doit VOIR ce que l'episode n'a pas vu. Quatorze
        // actions ne disent pas la meme chose selon qu'on a refuse zero ou
        // vingt-six gestes — et la vue est le seul endroit ou ce chiffre
        // rencontre un oeil humain.
        let mut ep = episode(
            "01A",
            "2026-01-01T00:00:00.000Z",
            vec![action(1, "input", "Description")],
        );
        ep.completeness.out_of_scope = 26;
        assert_eq!(resumer(&ep).hors_perimetre, 26);
    }

    #[test]
    fn le_resume_compte_les_actions_et_les_trous() {
        let ep = episode(
            "01A",
            "2026-01-01T00:00:00.000Z",
            vec![
                action(1, "input", "Description"),
                trou(2),
                action(3, "submit", "Enregistrer"),
            ],
        );
        let r = resumer(&ep);
        assert_eq!(r.actions, 2);
        assert_eq!(r.trous, 1);
        assert_eq!(r.grade, "B");
    }

    #[test]
    fn la_completude_est_la_part_de_geste() {
        assert_eq!(completude_pct(3, 1), 75);
        assert_eq!(completude_pct(4, 0), 100);
        assert_eq!(completude_pct(0, 2), 0);
    }

    #[test]
    fn un_episode_vide_ne_divise_pas_par_zero() {
        assert_eq!(completude_pct(0, 0), 0);
    }

    #[test]
    fn la_frise_suit_l_ordre_des_seq() {
        let ep = episode(
            "01A",
            "2026-01-01T00:00:00.000Z",
            vec![
                action(1, "navigate", "Details"),
                trou(2),
                action(3, "submit", "Enregistrer"),
            ],
        );
        let f = friser(&ep);
        assert_eq!(f.len(), 3);
        assert_eq!(f[0].genre, "action");
        assert_eq!(f[1].genre, "trou");
        assert_eq!(f[1].quoi, "sleep", "la cause du trou est lisible");
        assert_eq!(f[2].quoi, "submit");
    }

    #[test]
    fn la_liste_va_du_plus_recent_au_plus_ancien() {
        // C'est ce qu'on vient de faire qui interesse.
        let r = racine("ordre");
        deposer(
            &r,
            &episode(
                "01ANCIEN",
                "2026-01-01T00:00:00.000Z",
                vec![action(1, "input", "X")],
            ),
        );
        deposer(
            &r,
            &episode(
                "01RECENT",
                "2026-03-01T00:00:00.000Z",
                vec![action(1, "input", "Y")],
            ),
        );

        let ids: Vec<String> = lister(&r).into_iter().map(|e| e.id).collect();
        assert_eq!(ids, vec!["01RECENT", "01ANCIEN"]);
    }

    #[test]
    fn un_episode_abime_est_saute_sans_aveugler_les_autres() {
        // Un fichier corrompu ne doit pas rendre l'application aveugle a tout
        // le reste. Il reste sur disque ; son absence de la liste est le signal.
        let r = racine("abime");
        deposer(
            &r,
            &episode(
                "01BON",
                "2026-01-01T00:00:00.000Z",
                vec![action(1, "input", "X")],
            ),
        );
        let d = r.join("01CASSE");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("episode.json"), "{ ceci n est pas du json").unwrap();

        let liste = lister(&r);
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0].id, "01BON");
    }

    #[test]
    fn une_racine_absente_donne_une_liste_vide_pas_une_erreur() {
        // Jour 1 : le dossier n'existe pas encore. C'est l'etat « vide », pas
        // l'etat « erreur ».
        assert!(lister(Path::new("C:/n-existe-pas-du-tout")).is_empty());
    }

    #[test]
    fn le_detail_rend_le_resume_et_la_frise() {
        let r = racine("detail");
        deposer(
            &r,
            &episode(
                "01DETAIL",
                "2026-01-01T00:00:00.000Z",
                vec![action(1, "input", "X"), trou(2)],
            ),
        );
        let d = detail(&r, "01DETAIL").expect("un detail");
        assert_eq!(d.resume.actions, 1);
        assert_eq!(d.frise.len(), 2);
    }

    #[test]
    fn le_detail_d_un_episode_inconnu_est_none() {
        let r = racine("inconnu");
        assert!(detail(&r, "01INEXISTANT").is_none());
    }
}
