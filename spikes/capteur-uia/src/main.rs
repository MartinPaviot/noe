//! Spike capteur UIA — mesure, pas feature.
//!
//! Répond aux deux points `[SPIKE]` du design de la spec 002 :
//!   (a) quelle stratégie d'abonnement tient le budget CPU ;
//!   (b) quels paramètres de walker sont soutenables.
//!
//! **Entièrement non interactif.** Aucune lecture du clavier : le binaire tourne
//! en arrière-plan et se pilote par un fichier de contrôle, ligne par ligne.
//! L'opérateur ne touche qu'à son application ; l'orchestration est écrite par
//! l'agent.
//!
//! Protocole du fichier de contrôle — une commande par ligne, ajoutée à la fin :
//!   `fait <n>`  clôt l'occurrence courante, `<n>` = actions d'état déclarées
//!   `stop`      termine la phase, écrit le JSON, sort
//!
//! Aucun contenu n'est écrit : on ne conserve que des rôles, des longueurs et des
//! compteurs. Ce binaire observe pour mesurer, il ne capture pas pour garder.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use regex::Regex;
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

/// Garde-fou : même si personne n'écrit `stop`, la phase se termine.
const MAX_MINUTES_DEFAUT: u64 = 120;

#[derive(Clone, Debug, Serialize)]
struct Observation {
    occurrence: usize,
    /// Signature enrichie : role | nom normalise | region | chemin de roles.
    signature: String,
    /// Signature nue : role | nom brut. Sert de temoin pour mesurer l apport.
    signature_nue: String,
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
    application_cible: String,
    occurrences: usize,
    observations: usize,
    actions_etat: usize,
    actions_etat_resolues: usize,
    actions_etat_declarees: usize,
    /// Stabilite POST-PIPELINE, signature enrichie. C est LE chiffre du critere (a).
    stabilite_signature_pct: f64,
    /// Temoin : meme calcul sur les noms BRUTS. Mesure l apport de l enrichissement.
    stabilite_nue_pct: f64,
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

// ---------------------------------------------------------------------------
// Mesures
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
/// occurrences. Une signature présente dans certaines répétitions seulement
/// n'est pas un point d'ancrage fiable pour le ciblage.
fn stabilite_par(obs: &[Observation], nue: bool) -> f64 {
    let mut par_occ: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    for o in obs.iter().filter(|o| o.etat && o.resolu) {
        let s = if nue { o.signature_nue.clone() } else { o.signature.clone() };
        par_occ.entry(o.occurrence).or_default().insert(s);
    }
    if par_occ.len() < 2 {
        return 0.0;
    }
    let mut it = par_occ.values();
    let Some(premier) = it.next() else { return 0.0 };
    let mut communes: BTreeSet<String> = premier.clone();
    let mut union: BTreeSet<String> = premier.clone();
    for s in it {
        communes = communes.intersection(s).cloned().collect();
        union = union.union(s).cloned().collect();
    }
    if union.is_empty() {
        return 0.0;
    }
    communes.len() as f64 * 100.0 / union.len() as f64
}

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
// Normalisation POST-PIPELINE du nom accessible (D18, correctif 1)
// ---------------------------------------------------------------------------

/// Motifs volatils : ce qui change d'une occurrence à l'autre sans que
/// l'élément change. Les comparer bruts, c'est mesurer le bruit.
static MOTIFS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();

fn motifs() -> &'static Vec<(Regex, &'static str)> {
    MOTIFS.get_or_init(|| {
        let m: Vec<(&str, &'static str)> = vec![
            // --- fragments de donnees : pseudonymises en tokens stables ------
            (r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}", "EMAIL"),
            (r"(?:\+33|0)[1-9](?:[ .-]?\d{2}){4}", "TEL"),
            // Identifiants Salesforce : 15 ou 18 caracteres alphanumeriques.
            (r"\b[a-zA-Z0-9]{18}\b", "ID18"),
            (r"\b[a-zA-Z0-9]{15}\b", "ID15"),
            // --- motifs volatils : effaces ----------------------------------
            // Horodatages relatifs, francais et anglais.
            (r"(?i)il y a \d+\s*\w+", "TEMPS"),
            (r"(?i)\d+\s*(minutes?|heures?|jours?|min|h|hours?|days?|ago)\b", "TEMPS"),
            (r"\d{1,2}[/:]\d{2}(?:[/:]\d{2,4})?", "TEMPS"),
            // Compteurs entre parentheses : « Fichiers (12) ».
            (r"\(\s*\d+\s*\)", "N"),
            // Toute sequence de 4 chiffres ou plus : ids, compteurs, montants.
            (r"\d{4,}", "N"),
        ];
        m.into_iter()
            .filter_map(|(p, t)| Regex::new(p).ok().map(|r| (r, t)))
            .collect()
    })
}

/**
 * Applique au nom accessible la normalisation que le produit appliquerait
 * AVANT tout usage — puis seulement on compare.
 *
 * Comparer des noms bruts revient à mesurer un objet qui n'existe nulle part
 * dans le système réel : le produit ne voit jamais un nom non normalisé.
 */
fn normaliser_nom(brut: &str) -> String {
    let mut s = brut.trim().to_string();
    for (re, jeton) in motifs() {
        s = re.replace_all(&s, *jeton).into_owned();
    }
    // Espaces effondres : « Piste   ouverte » et « Piste ouverte » sont le meme nom.
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Rôle d'un élément, ou « Inconnu ».
fn role_de(el: &UIElement) -> String {
    el.get_control_type()
        .map(|c| format!("{c:?}"))
        .unwrap_or_else(|_| "Inconnu".into())
}

/**
 * Ancrage structurel (D18, correctif 2) : la région porteuse et le chemin des
 * rôles ancestraux.
 *
 * Le nom seul est un ancrage trop maigre sur une SPA. La région dit *où* dans
 * l'application, le chemin de rôles dit *quoi* dans la structure — deux choses
 * qui bougent beaucoup moins qu'un libellé.
 */
fn contexte(automation: &UIAutomation, el: &UIElement) -> (String, String) {
    let mut region = String::new();
    let mut chemin: Vec<String> = Vec::new();

    if let Ok(walker) = automation.create_tree_walker() {
        let mut courant = el.clone();
        for _ in 0..PROFONDEUR_ANCRAGE {
            match walker.get_parent(&courant) {
                Ok(p) => {
                    let r = role_de(&p);
                    // Premiere region nommee rencontree en remontant.
                    if region.is_empty()
                        && matches!(r.as_str(), "Pane" | "Group" | "Document" | "Window" | "Custom")
                    {
                        let n = normaliser_nom(&p.get_name().unwrap_or_default());
                        if !n.is_empty() {
                            region = n;
                        }
                    }
                    chemin.push(r);
                    courant = p;
                }
                Err(_) => break,
            }
        }
    }

    (region, chemin.join(">"))
}

/// Nombre d'ancêtres remontés pour l'ancrage. Au-delà, le coût dépasse l'apport.
const PROFONDEUR_ANCRAGE: usize = 4;

/**
 * Remonte de l'élément au conteneur de haut niveau — le dernier ancêtre avant la
 * racine du bureau.
 *
 * Sert d'identité de fenêtre pour la stratégie « focus ». Le handle natif ne peut
 * pas jouer ce rôle : il vaut 0 sur les éléments internes de Chrome, ce qui a fait
 * échouer deux tentatives de réparation.
 */
fn conteneur_de(automation: &UIAutomation, el: &UIElement, racine: &UIElement) -> Option<UIElement> {
    let walker = automation.create_tree_walker().ok()?;
    let id_racine = racine.get_runtime_id().ok()?;
    let mut courant = el.clone();
    for _ in 0..25 {
        match walker.get_parent(&courant) {
            Ok(p) => {
                if p.get_runtime_id().ok().as_ref() == Some(&id_racine) {
                    return Some(courant);
                }
                courant = p;
            }
            Err(_) => break,
        }
    }
    Some(courant)
}

/**
 * Signature enrichie : `rôle | nom normalisé | région | chemin de rôles`.
 *
 * Retourne aussi la signature NUE (rôle + nom brut) pour que le rapport puisse
 * chiffrer ce que l'enrichissement a réellement apporté.
 */
fn signature(automation: &UIAutomation, el: &UIElement) -> (String, String, String, bool) {
    let role = role_de(el);
    let brut = el.get_name().unwrap_or_default();
    let nom = normaliser_nom(&brut);
    let (region, chemin) = contexte(automation, el);
    let resolu = !brut.trim().is_empty() && role != "Inconnu";
    (
        role.clone(),
        format!("{role}|{brut}"),
        format!("{role}|{nom}|{region}|{chemin}"),
        resolu,
    )
}

// ---------------------------------------------------------------------------
// Fichier de contrôle
// ---------------------------------------------------------------------------

/// Lit les lignes ajoutées depuis le dernier appel. Un simple curseur d'octets
/// suffit : le fichier ne fait que grossir.
fn lignes_nouvelles(chemin: &str, curseur: &mut u64) -> Vec<String> {
    let Ok(mut f) = std::fs::File::open(chemin) else {
        return Vec::new();
    };
    if f.seek(SeekFrom::Start(*curseur)).is_err() {
        return Vec::new();
    }
    let mut buf = String::new();
    if f.read_to_string(&mut buf).is_err() {
        return Vec::new();
    }
    *curseur += buf.len() as u64;
    buf.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

fn arg(args: &[String], nom: &str) -> Option<String> {
    args.iter().position(|a| a == nom).and_then(|i| args.get(i + 1)).cloned()
}

// ---------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let strategie = arg(&args, "--strategie").unwrap_or_else(|| "globale".into());
    if strategie != "globale" && strategie != "focus" {
        eprintln!("--strategie doit valoir « globale » ou « focus »");
        std::process::exit(2);
    }
    let controle = arg(&args, "--controle").unwrap_or_else(|| "controle.txt".into());
    let sortie =
        arg(&args, "--sortie").unwrap_or_else(|| format!("resultats/spike-{strategie}.json"));
    let cible = arg(&args, "--cible").unwrap_or_else(|| "(non renseignee)".into());
    let max_minutes: u64 = arg(&args, "--max-minutes")
        .and_then(|v| v.parse().ok())
        .unwrap_or(MAX_MINUTES_DEFAUT);

    // Le fichier de controle doit exister avant qu'on le suive.
    if !std::path::Path::new(&controle).exists() {
        std::fs::write(&controle, "")?;
    }
    let mut curseur = std::fs::metadata(&controle).map(|m| m.len()).unwrap_or(0);

    let automation = UIAutomation::new()?;
    let racine = automation.get_root_element()?;

    let collecte = Arc::new(Mutex::new(Collecte::default()));
    let occurrence = Arc::new(AtomicU64::new(1));
    let debut = Instant::now();

    // --- abonnements : c'est ICI que les deux strategies different ----------
    //
    // « globale » : un abonnement unique sur TOUT le bureau (Descendants).
    // « focus »   : un abonnement sur le SOUS-ARBRE de la fenetre active, refait
    //               quand la fenetre change. C'est la vraie difference — et c'est
    //               ce que la v1 ratait : TreeScope::Element sur la racine ne
    //               s'abonne qu'a la racine elle-meme, donc a rien.
    //
    // La (re)souscription vit dans la boucle principale, pas dans un handler :
    // tous les appels UIA restent ainsi sur le meme thread.
    let fabriquer = |c: Arc<Mutex<Collecte>>, occ: Arc<AtomicU64>, auto_sig: UIAutomation| -> UIEventHandler {
        (Box::new(move |sender: &UIElement, kind: UIEventType| {
            let (role, sig_nue, sig, resolu) = signature(&auto_sig, sender);
            let nom_len = sender.get_name().unwrap_or_default().chars().count();
            if let Ok(mut g) = c.lock() {
                g.observations.push(Observation {
                    occurrence: occ.load(Ordering::Relaxed) as usize,
                    signature: sig,
                    signature_nue: sig_nue,
                    role,
                    longueur_nom: nom_len,
                    resolu,
                    etat: EVENEMENTS_ETAT.contains(&kind),
                    ms_depuis_debut: debut.elapsed().as_millis(),
                });
            }
            Ok(())
        }) as Box<CustomEventHandlerFn>)
            .into()
    };

    // Abonnements courants : (evenement, handler, element sur lequel il porte).
    let mut poignees: Vec<(UIEventType, UIEventHandler, UIElement)> = Vec::new();

    if strategie == "globale" {
        for &ev in EVENEMENTS_ETAT.iter().chain(&[UIEventType::StructureChanged]) {
            let h = fabriquer(Arc::clone(&collecte), Arc::clone(&occurrence), automation.clone());
            automation.add_automation_event_handler(
                ev,
                &racine,
                TreeScope::Descendants,
                None,
                &h,
            )?;
            poignees.push((ev, h, racine.clone()));
        }
    }
    // Fenetre actuellement couverte par la strategie « focus ».
    let mut fenetre_courante = String::new();

    let focus_handler: Option<UIFocusChangedEventHandler> = if strategie == "focus" {
        let c = Arc::clone(&collecte);
        let occ = Arc::clone(&occurrence);
        let auto2 = UIAutomation::new()?;
        let h: UIFocusChangedEventHandler = (Box::new(move |sender: &UIElement| {
            let (role, sig_nue, sig, resolu) = signature(&auto2, sender);
            let nom_len = sender.get_name().unwrap_or_default().chars().count();
            let m = mesurer_walker(&auto2, sender);
            if let Ok(mut g) = c.lock() {
                g.walker.push(m);
                g.observations.push(Observation {
                    occurrence: occ.load(Ordering::Relaxed) as usize,
                    signature: sig,
                    signature_nue: sig_nue,
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

    // --- echantillonnage 1 Hz ----------------------------------------------
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

    println!("PRET strategie={strategie} controle={controle}");
    println!("commandes : « fait <n> » clot une occurrence · « stop » termine");

    // --- boucle de pilotage, sans aucune lecture clavier ---------------------
    let mut declarees_total = 0usize;
    let mut occurrences_closes = 0usize;
    let limite = Duration::from_secs(max_minutes * 60);

    let mut tours = 0u64;
    loop {
        // Strategie « focus » : suivre la fenetre active et y (re)poser les
        // abonnements. Verifie chaque seconde, pas a chaque tour de boucle.
        tours += 1;
        if strategie == "focus" && tours % 4 == 1 {
            if let Ok(focus) = automation.get_focused_element() {
                // Le handle natif vaut 0 sur les elements internes de Chrome : deux
                // tentatives ont echoue la-dessus. On identifie le conteneur par son
                // runtime_id, qui existe toujours.
                let identite = conteneur_de(&automation, &focus, &racine)
                    .and_then(|c| c.get_runtime_id().ok())
                    .map(|id| id.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("."))
                    .unwrap_or_default();
                if !identite.is_empty() && identite != fenetre_courante {
                    // Retirer les abonnements de la fenetre precedente.
                    for (ev, h, el) in &poignees {
                        let _ = automation.remove_automation_event_handler(*ev, el, h);
                    }
                    poignees.clear();

                    // Remonter au conteneur de haut niveau, puis s'y abonner.
                    let mut conteneur = focus.clone();
                    if let Ok(walker) = automation.create_tree_walker() {
                        for _ in 0..20 {
                            match walker.get_parent(&conteneur) {
                                Ok(p) => {
                                    let racine_atteinte = p
                                        .get_runtime_id()
                                        .ok()
                                        .zip(racine.get_runtime_id().ok())
                                        .map(|(a, b)| a == b)
                                        .unwrap_or(false);
                                    if racine_atteinte {
                                        break;
                                    }
                                    conteneur = p;
                                }
                                Err(_) => break,
                            }
                        }
                    }

                    for &ev in EVENEMENTS_ETAT.iter().chain(&[UIEventType::StructureChanged]) {
                        let h = fabriquer(Arc::clone(&collecte), Arc::clone(&occurrence), automation.clone());
                        if automation
                            .add_automation_event_handler(ev, &conteneur, TreeScope::Subtree, None, &h)
                            .is_ok()
                        {
                            poignees.push((ev, h, conteneur.clone()));
                        }
                    }
                    fenetre_courante = identite;
                    println!(
                        "  abonnement deplace sur « {} » ({} handlers)",
                        conteneur.get_name().unwrap_or_default().chars().take(40).collect::<String>(),
                        poignees.len()
                    );
                }
            }
        }

        for ligne in lignes_nouvelles(&controle, &mut curseur) {
            let mut mots = ligne.split_whitespace();
            match mots.next() {
                Some("fait") => {
                    let n: usize = mots.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                    declarees_total += n;
                    occurrences_closes += 1;
                    occurrence.store(occurrences_closes as u64 + 1, Ordering::Relaxed);
                    println!("OCCURRENCE {occurrences_closes} close, {n} actions declarees");
                }
                Some("stop") => {
                    println!("STOP recu");
                    stop.store(true, Ordering::Relaxed);
                }
                Some(autre) => println!("commande ignoree : {autre}"),
                None => {}
            }
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if debut.elapsed() > limite {
            println!("LIMITE de {max_minutes} min atteinte, arret automatique");
            stop.store(true, Ordering::Relaxed);
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    let _ = sampler.join();
    for (ev, h, el) in &poignees {
        let _ = automation.remove_automation_event_handler(*ev, el, h);
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

    let resultat = ResultatPhase {
        strategie: strategie.clone(),
        application_cible: cible,
        occurrences: occurrences_closes,
        observations: g.observations.len(),
        actions_etat: actions_etat.len(),
        actions_etat_resolues: resolues,
        actions_etat_declarees: declarees_total,
        stabilite_signature_pct: stabilite_par(&g.observations, false),
        stabilite_nue_pct: stabilite_par(&g.observations, true),
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
    };

    if let Some(parent) = std::path::Path::new(&sortie).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&sortie, serde_json::to_string_pretty(&resultat)?)?;

    println!(
        "TERMINE {} — occurrences {} · stabilite {:.1} % · couverture {:.1} % · CPU p95 {:.2} % · RAM max {:.1} Mo",
        strategie,
        resultat.occurrences,
        resultat.stabilite_signature_pct,
        resultat.couverture_etat_pct,
        resultat.cpu_p95_fenetres_30s,
        resultat.ram_max_mo
    );
    println!("SORTIE {sortie}");
    Ok(())
}
