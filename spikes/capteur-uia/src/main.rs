//! Spike capteur UIA — mesure, pas feature.
//!
//! Répond aux deux points `[SPIKE]` du design de la spec 002 :
//!   (a) quelle stratégie d'abonnement tient le budget CPU ;
//!   (b) quels paramètres de walker sont soutenables.
//!
//! Les deux stratégies tournent dans la MÊME session, l'une après l'autre, sur
//! la même tâche répétée — sinon les chiffres ne seraient pas comparables.
//!
//! Aucun contenu n'est écrit : on ne conserve que des rôles, des longueurs et
//! des empreintes. Ce binaire observe pour mesurer, il ne capture pas pour
//! garder.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use uiautomation::events::{
    CustomEventHandlerFn, CustomFocusChangedEventHandlerFn, UIEventHandler, UIEventType,
    UIFocusChangedEventHandler,
};
use uiautomation::types::TreeScope;
use uiautomation::{UIAutomation, UIElement};

/// Les événements qui traduisent un changement d'état, par opposition à la
/// simple navigation. Ce sont eux que la couverture mesure.
const EVENEMENTS_ETAT: &[UIEventType] = &[
    UIEventType::Invoke_Invoked,
    UIEventType::Text_TextChanged,
    UIEventType::SelectionItem_ElementSelected,
];

/// Budgets du walker à éprouver (point [SPIKE] b).
const WALKER_PROFONDEUR_MAX: usize = 12;
const WALKER_NOEUDS_MAX: usize = 1500;

#[derive(Clone, Debug, Serialize)]
struct Observation {
    occurrence: usize,
    /// `role|nom` — la signature de ciblage dont on mesure la stabilité.
    signature: String,
    role: String,
    /// Longueur seulement : le nom lui-même ne sort jamais d'ici.
    longueur_nom: usize,
    resolu: bool,
    etat: bool,
    ms_depuis_debut: u128,
}

#[derive(Clone, Debug, Serialize)]
struct MesureWalker {
    noeuds: usize,
    profondeur: usize,
    duree_ms: u128,
    tronque: bool,
}

#[derive(Debug, Default)]
struct Collecte {
    observations: Vec<Observation>,
    walker: Vec<MesureWalker>,
}

#[derive(Clone, Debug, Serialize)]
struct EchantillonCharge {
    seconde: u64,
    cpu_pct: f32,
    ram_mo: f64,
}

#[derive(Debug, Serialize)]
struct ResultatPhase {
    strategie: String,
    occurrences: usize,
    observations: usize,
    actions_etat: usize,
    actions_etat_resolues: usize,
    actions_etat_declarees: usize,
    stabilite_signature_pct: f64,
    couverture_etat_pct: f64,
    cpu_p95_fenetres_30s: f64,
    cpu_max_fenetre_30s: f64,
    ram_max_mo: f64,
    walker_noeuds_p50: usize,
    walker_noeuds_p95: usize,
    walker_profondeur_max: usize,
    walker_duree_p95_ms: u128,
    walker_tronques_pct: f64,
    echantillons: Vec<EchantillonCharge>,
}

#[derive(Debug, Serialize)]
struct Verdict {
    genere_le: String,
    application_cible: String,
    phases: Vec<ResultatPhase>,
}

// ---------------------------------------------------------------------------
// Utilitaires de mesure
// ---------------------------------------------------------------------------

fn percentile(mut v: Vec<f64>, p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let i = ((v.len() as f64 - 1.0) * p).round() as usize;
    v[i.min(v.len() - 1)]
}

/// Moyennes par fenêtre de 30 s, puis percentile sur ces fenêtres — c'est la
/// définition de R7.1, pas un percentile sur les échantillons bruts.
fn fenetres_30s(ech: &[EchantillonCharge]) -> Vec<f64> {
    let mut par_fenetre: BTreeMap<u64, Vec<f32>> = BTreeMap::new();
    for e in ech {
        par_fenetre.entry(e.seconde / 30).or_default().push(e.cpu_pct);
    }
    par_fenetre
        .values()
        .map(|v| f64::from(v.iter().sum::<f32>()) / v.len() as f64)
        .collect()
}

/// Stabilité : part des signatures d'actions d'état communes à TOUTES les
/// occurrences. Une signature qui n'apparaît que dans certaines répétitions
/// n'est pas un point d'ancrage fiable pour le ciblage.
fn stabilite(obs: &[Observation], occurrences: usize) -> f64 {
    if occurrences < 2 {
        return 0.0;
    }
    let mut par_occ: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    for o in obs.iter().filter(|o| o.etat && o.resolu) {
        par_occ.entry(o.occurrence).or_default().insert(o.signature.clone());
    }
    if par_occ.len() < 2 {
        return 0.0;
    }
    let mut ensembles = par_occ.values();
    let Some(premier) = ensembles.next() else {
        return 0.0;
    };
    let mut communes: BTreeSet<String> = premier.clone();
    let mut union: BTreeSet<String> = premier.clone();
    for s in ensembles {
        communes = communes.intersection(s).cloned().collect();
        union = union.union(s).cloned().collect();
    }
    if union.is_empty() {
        return 0.0;
    }
    communes.len() as f64 * 100.0 / union.len() as f64
}

// ---------------------------------------------------------------------------
// Walker
// ---------------------------------------------------------------------------

fn mesurer_walker(automation: &UIAutomation, racine: &UIElement) -> MesureWalker {
    let debut = Instant::now();
    let mut noeuds = 0usize;
    let mut profondeur_max = 0usize;
    let mut tronque = false;

    if let Ok(walker) = automation.create_tree_walker() {
        let mut pile: Vec<(UIElement, usize)> = vec![(racine.clone(), 0)];
        while let Some((el, prof)) = pile.pop() {
            noeuds += 1;
            profondeur_max = profondeur_max.max(prof);
            if noeuds >= WALKER_NOEUDS_MAX || prof >= WALKER_PROFONDEUR_MAX {
                tronque = true;
                continue;
            }
            if let Ok(mut enfant) = walker.get_first_child(&el) {
                loop {
                    pile.push((enfant.clone(), prof + 1));
                    match walker.get_next_sibling(&enfant) {
                        Ok(suivant) => enfant = suivant,
                        Err(_) => break,
                    }
                }
            }
        }
    }

    MesureWalker {
        noeuds,
        profondeur: profondeur_max,
        duree_ms: debut.elapsed().as_millis(),
        tronque,
    }
}

// ---------------------------------------------------------------------------
// Phase
// ---------------------------------------------------------------------------

fn signature(el: &UIElement) -> (String, String, bool) {
    let role = el
        .get_control_type()
        .map(|c| format!("{c:?}"))
        .unwrap_or_else(|_| "Inconnu".into());
    let nom = el.get_name().unwrap_or_default();
    let resolu = !nom.trim().is_empty() && role != "Inconnu";
    (role.clone(), format!("{role}|{nom}"), resolu)
}

fn lire_ligne(invite: &str) -> String {
    print!("{invite}");
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    let _ = std::io::stdin().lock().read_line(&mut s);
    s.trim().to_string()
}

fn executer_phase(
    strategie: &str,
    occurrences_voulues: usize,
    auto_secondes: Option<u64>,
) -> Result<ResultatPhase, Box<dyn std::error::Error>> {
    let automation = UIAutomation::new()?;
    let racine = automation.get_root_element()?;

    let collecte = Arc::new(Mutex::new(Collecte::default()));
    let occurrence = Arc::new(AtomicU64::new(1));
    let debut = Instant::now();

    // --- abonnements -------------------------------------------------------
    let mut poignees: Vec<(UIEventType, UIEventHandler)> = Vec::new();

    for &ev in EVENEMENTS_ETAT.iter().chain(&[UIEventType::StructureChanged]) {
        let c = Arc::clone(&collecte);
        let occ = Arc::clone(&occurrence);
        let handler: UIEventHandler = (Box::new(move |sender: &UIElement, kind: UIEventType| {
            let (role, sig, resolu) = signature(sender);
            let nom_len = sender.get_name().unwrap_or_default().chars().count();
            if let Ok(mut g) = c.lock() {
                g.observations.push(Observation {
                    occurrence: occ.load(Ordering::Relaxed) as usize,
                    signature: sig,
                    role,
                    longueur_nom: nom_len,
                    resolu,
                    etat: EVENEMENTS_ETAT.contains(&kind),
                    ms_depuis_debut: debut.elapsed().as_millis(),
                });
            }
            Ok(())
        }) as Box<CustomEventHandlerFn>)
            .into();

        // C'est ICI que les deux strategies different : la portee de l'abonnement.
        let portee = if strategie == "globale" {
            TreeScope::Descendants
        } else {
            TreeScope::Element
        };
        automation.add_automation_event_handler(ev, &racine, portee, None, &handler)?;
        poignees.push((ev, handler));
    }

    // En strategie « focus », on suit le focus et on mesure le conteneur actif.
    let focus_handler: Option<UIFocusChangedEventHandler> = if strategie == "focus" {
        let c = Arc::clone(&collecte);
        let occ = Arc::clone(&occurrence);
        let auto2 = UIAutomation::new()?;
        let h: UIFocusChangedEventHandler = (Box::new(move |sender: &UIElement| {
            let (role, sig, resolu) = signature(sender);
            let nom_len = sender.get_name().unwrap_or_default().chars().count();
            let m = mesurer_walker(&auto2, sender);
            if let Ok(mut g) = c.lock() {
                g.walker.push(m);
                g.observations.push(Observation {
                    occurrence: occ.load(Ordering::Relaxed) as usize,
                    signature: sig,
                    role,
                    longueur_nom: nom_len,
                    resolu,
                    etat: false,
                    ms_depuis_debut: debut.elapsed().as_millis(),
                });
            }
            Ok(())
        }) as Box<CustomFocusChangedEventHandlerFn>)
            .into();
        automation.add_focus_changed_event_handler(None, &h)?;
        Some(h)
    } else {
        None
    };

    // --- echantillonnage 1 Hz ---------------------------------------------
    let echantillons = Arc::new(Mutex::new(Vec::<EchantillonCharge>::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let ech2 = Arc::clone(&echantillons);
    let stop2 = Arc::clone(&stop);
    let pid = Pid::from_u32(std::process::id());
    let sampler = std::thread::spawn(move || {
        let mut sys = System::new();
        // Premier rafraichissement : sysinfo a besoin d une reference pour le CPU.
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        std::thread::sleep(Duration::from_millis(200));
        let mut seconde = 0u64;
        while !stop2.load(Ordering::Relaxed) {
            sys.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[pid]),
                true,
                ProcessRefreshKind::nothing().with_cpu().with_memory(),
            );
            if let Some(p) = sys.process(pid) {
                if let Ok(mut g) = ech2.lock() {
                    g.push(EchantillonCharge {
                        seconde,
                        cpu_pct: p.cpu_usage(),
                        ram_mo: p.memory() as f64 / 1_048_576.0,
                    });
                }
            }
            seconde += 1;
            std::thread::sleep(Duration::from_secs(1));
        }
    });

    // --- deroulement pilote par l'operateur --------------------------------
    println!("\n─────────────────────────────────────────────────────────────");
    println!("  PHASE « {strategie} »  —  {occurrences_voulues} occurrences");
    println!("─────────────────────────────────────────────────────────────");
    println!("  Faites votre tache normalement, du debut a la fin.");
    println!("  Puis revenez ici et appuyez sur Entree.\n");

    let mut declarees_total = 0usize;
    for n in 1..=occurrences_voulues {
        occurrence.store(n as u64, Ordering::Relaxed);
        if let Some(s) = auto_secondes {
            println!("  Occurrence {n}/{occurrences_voulues} — mode auto, {s} s...");
            std::thread::sleep(Duration::from_secs(s));
            declarees_total += 1;
        } else {
            lire_ligne(&format!(
                "  Occurrence {n}/{occurrences_voulues} — Entree quand c'est fini : "
            ));
            let rep =
                lire_ligne("    Combien d'actions modifiant un etat (clics, saisies, envois) ? ");
            declarees_total += rep.parse::<usize>().unwrap_or(0);
        }
    }

    // --- arret --------------------------------------------------------------
    stop.store(true, Ordering::Relaxed);
    let _ = sampler.join();
    for (ev, h) in &poignees {
        let _ = automation.remove_automation_event_handler(*ev, &racine, h);
    }
    if let Some(h) = &focus_handler {
        let _ = automation.remove_focus_changed_event_handler(h);
    }

    // --- agregation ---------------------------------------------------------
    let g = collecte.lock().map_err(|_| "collecte empoisonnee")?;
    let ech = echantillons.lock().map_err(|_| "echantillons empoisonnes")?.clone();

    let actions_etat: Vec<&Observation> = g.observations.iter().filter(|o| o.etat).collect();
    let resolues = actions_etat.iter().filter(|o| o.resolu).count();

    let fen = fenetres_30s(&ech);
    let noeuds: Vec<f64> = g.walker.iter().map(|w| w.noeuds as f64).collect();
    let durees: Vec<f64> = g.walker.iter().map(|w| w.duree_ms as f64).collect();

    Ok(ResultatPhase {
        strategie: strategie.to_string(),
        occurrences: occurrences_voulues,
        observations: g.observations.len(),
        actions_etat: actions_etat.len(),
        actions_etat_resolues: resolues,
        actions_etat_declarees: declarees_total,
        stabilite_signature_pct: stabilite(&g.observations, occurrences_voulues),
        couverture_etat_pct: if declarees_total == 0 {
            0.0
        } else {
            (actions_etat.len() as f64 * 100.0 / declarees_total as f64).min(100.0)
        },
        cpu_p95_fenetres_30s: percentile(fen.clone(), 0.95),
        cpu_max_fenetre_30s: fen.iter().cloned().fold(0.0, f64::max),
        ram_max_mo: ech.iter().map(|e| e.ram_mo).fold(0.0, f64::max),
        walker_noeuds_p50: percentile(noeuds.clone(), 0.50) as usize,
        walker_noeuds_p95: percentile(noeuds, 0.95) as usize,
        walker_profondeur_max: g.walker.iter().map(|w| w.profondeur).max().unwrap_or(0),
        walker_duree_p95_ms: percentile(durees, 0.95) as u128,
        walker_tronques_pct: if g.walker.is_empty() {
            0.0
        } else {
            g.walker.iter().filter(|w| w.tronque).count() as f64 * 100.0 / g.walker.len() as f64
        },
        echantillons: ech,
    })
}

// ---------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let occurrences: usize = args
        .iter()
        .position(|a| a == "--occurrences")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    println!("Spike capteur UIA — mesure des deux strategies d'abonnement.");
    println!("Aucun contenu n'est enregistre : roles, longueurs et compteurs seulement.\n");

    let auto: Option<u64> = args
        .iter()
        .position(|a| a == "--auto-seconds")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok());

    let cible = if auto.is_some() {
        "(mode auto)".to_string()
    } else {
        lire_ligne("Application cible (ex: Salesforce Lightning, Gmail) : ")
    };

    let mut phases = Vec::new();
    for strategie in ["globale", "focus"] {
        phases.push(executer_phase(strategie, occurrences, auto)?);
        if strategie == "globale" && auto.is_none() {
            println!("\n  Phase 1 terminee. Respirez.");
            lire_ligne("  Entree pour enchainer sur la strategie « focus » : ");
        }
    }

    let verdict = Verdict {
        genere_le: format!("{:?}", std::time::SystemTime::now()),
        application_cible: cible,
        phases,
    };

    std::fs::create_dir_all("resultats")?;
    let json = serde_json::to_string_pretty(&verdict)?;
    std::fs::write("resultats/spike.json", &json)?;

    println!("\n═══════════════════════════════════════════════════════════");
    for p in &verdict.phases {
        println!(
            "  {:8}  stabilite {:5.1} %   couverture {:5.1} %   CPU p95 {:5.2} %   RAM max {:6.1} Mo",
            p.strategie,
            p.stabilite_signature_pct,
            p.couverture_etat_pct,
            p.cpu_p95_fenetres_30s,
            p.ram_max_mo
        );
        println!(
            "            walker : noeuds p50 {} / p95 {}, profondeur max {}, duree p95 {} ms, tronques {:.0} %",
            p.walker_noeuds_p50,
            p.walker_noeuds_p95,
            p.walker_profondeur_max,
            p.walker_duree_p95_ms,
            p.walker_tronques_pct
        );
    }
    println!("═══════════════════════════════════════════════════════════");
    println!("\nresultats/spike.json ecrit.");
    println!("Depuis la racine du depot, lancez : node scripts/remplir-verdict.mjs");
    Ok(())
}
