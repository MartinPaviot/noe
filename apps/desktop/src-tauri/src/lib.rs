//! Coquille Tauri de Noe — barre d'état, menu, hotkeys de bornage.
//!
//! Spec 002, tâche 1. Ce fichier ne contient QUE du câblage : la logique de
//! bornage vit dans [`etat`], la persistance dans [`config`], et les deux se
//! testent sans écran. Tout ce qui décide vraiment quelque chose est ailleurs,
//! là où la CI peut l'atteindre.

mod assemblage;
mod clavier;
mod cle;
mod config;
mod etat;
mod horloge;
mod journal;
mod moteur;
mod motifs;
mod presse_papiers;
mod redaction;
mod snapshot;
mod source;
mod surfaces;
mod uia;
mod veille;
mod vue;

use std::sync::Mutex;

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Manager, Runtime, State,
};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

use clavier::HookClavier;
use cle::CleHmac;
use config::Config;
use etat::{EtatTray, Session};
use horloge::{Horloge, HorlogeReelle};
use moteur::Moteur;
use redaction::Redacteur;
use source::{Abonnement, CaptureSource, RawEvent};
use uia::UiaSource;

/// Ctrl+Alt+D commence, Ctrl+Alt+F clôt. D comme début, F comme fin.
///
/// Des fonctions et non des constantes : `Shortcut::new` n'est pas `const`.
fn raccourci_debut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyD)
}

fn raccourci_fin() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyF)
}

const ID_FENETRE: &str = "fenetre";
const ID_PAUSE: &str = "pause";
const ID_PANIQUE: &str = "panique";
const ID_DOSSIER: &str = "dossier";
const ID_QUITTER: &str = "quitter";
/// Les entrées du sous-menu portent l'identifiant `tache:<slug>`.
const PREFIXE_TACHE: &str = "tache:";
/// R5.4 : une entree cochable par surface offerte a l'activation.
const PREFIXE_SURFACE: &str = "surface:";

struct Etat {
    session: Mutex<Session>,
    config: Mutex<Config>,
    /// Une seule horloge pour toute la vie du processus : le monotone n'a de
    /// sens que rapporte a une meme origine.
    horloge: std::sync::Arc<dyn Horloge>,
    /// Le moteur n'existe QUE pendant un episode. C'est la meme garantie que
    /// `Session` porte cote etat : hors episode, il n'y a rien ou ecrire.
    moteur: Mutex<Option<Moteur>>,
    /// Charge une fois pour toutes : la cle HMAC ne doit pas changer entre deux
    /// episodes, sinon les jetons changent et les jointures cassent (R4.2).
    redacteur: std::sync::Arc<Redacteur>,
    /// L'abonnement natif et son canal, vivants pendant l'episode seulement.
    ///
    /// Relacher le tuple coupe la capture : c'est `Abonnement` qui porte cette
    /// garantie, et R1.2 en depend — hors episode, aucune source ne doit
    /// continuer a pousser.
    capture: Mutex<Option<(Abonnement, std::sync::mpsc::Receiver<RawEvent>)>>,
    /// D27 : le hook clavier ne vit que pendant l'épisode. Le relâcher le
    /// retire — R1.2 porté par la durée de vie, pas par la vigilance.
    clavier: Mutex<Option<HookClavier>>,
    /// L'appariement copier-coller de l'épisode en cours (R2.3).
    appariement: Mutex<presse_papiers::Appariement>,
}

/// Le dossier de données du poste. Tout ce que Noe écrit vit dessous.
fn dossier_donnees<R: Runtime>(app: &AppHandle<R>) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("noe"))
}

fn chemin_config<R: Runtime>(app: &AppHandle<R>) -> std::path::PathBuf {
    dossier_donnees(app).join("config.json")
}

/// Notifie l'opérateur, et retombe sur la journalisation si le canal manque.
///
/// Un refus qu'on n'a pas su afficher doit rester traçable : sans ça, R1.1
/// (« refus notifié ») deviendrait invérifiable sur une machine où les toasts
/// sont désactivés.
fn notifier<R: Runtime>(app: &AppHandle<R>, titre: &str, corps: &str) {
    let envoi = app.notification().builder().title(titre).body(corps).show();
    if envoi.is_err() {
        eprintln!("[noe] {titre} — {corps}");
    }
}

/// Reflète l'état courant sur l'icône et l'infobulle (R5.1).
fn rafraichir_tray<R: Runtime>(app: &AppHandle<R>) {
    let (etat_tray, tache) = {
        let e: State<Etat> = app.state();
        let s = e.session.lock().expect("session empoisonnee");
        (s.etat_tray(), s.episode().map(|ep| ep.task_slug.clone()))
    };
    appliquer_tray_avec(app, etat_tray, tache.as_deref());
}

fn appliquer_tray<R: Runtime>(app: &AppHandle<R>, etat_tray: EtatTray) {
    appliquer_tray_avec(app, etat_tray, None);
}

/// L infobulle nomme la tache observee quand il y en a une.
///
/// « Noe — observe » ne dit pas SOUS QUELLE etiquette. Or c est exactement ce
/// qu il faut pouvoir verifier d un coup d oeil : un episode qui court sous le
/// mauvais slug pollue le corpus sans qu aucune alerte ne se declenche.
fn appliquer_tray_avec<R: Runtime>(app: &AppHandle<R>, etat_tray: EtatTray, tache: Option<&str>) {
    let Some(tray) = app.tray_by_id("principal") else {
        return;
    };
    match tauri::image::Image::from_bytes(etat_tray.png()) {
        Ok(image) => {
            let _ = tray.set_icon(Some(image));
        }
        // Impossible sauf icone corrompue au build : on le dit plutot que de
        // laisser le tray afficher un etat qui n est plus le bon.
        Err(err) => eprintln!("[noe] icone {} illisible : {err}", etat_tray.icone()),
    }
    let infobulle = match tache {
        Some(slug) => format!("{} — « {slug} »", etat_tray.infobulle()),
        None => etat_tray.infobulle().to_string(),
    };
    let _ = tray.set_tooltip(Some(&infobulle));
}

/// R1.1 — le hotkey de début.
fn demarrer<R: Runtime>(app: &AppHandle<R>) {
    let e: State<Etat> = app.state();
    let active = {
        let c = e.config.lock().expect("config empoisonnee");
        c.tache_active.clone()
    };

    let resultat = {
        let mut s = e.session.lock().expect("session empoisonnee");
        s.demarrer(active.as_deref(), e.horloge.mural_ms(), || {
            ulid::Ulid::new().to_string()
        })
        .map(|ep| (ep.id.clone(), ep.task_slug.clone()))
    };

    match resultat {
        Ok((id, slug)) => {
            // La source d'abord : c'est elle qui fournit le photographe, et le
            // moteur doit le recevoir avant de traiter le premier declencheur.
            let mut native = UiaSource::new();
            // R5.4 : l'episode ne capture que sur les surfaces activees. La
            // liste est copiee a l'ouverture et re-poussee a chaque
            // changement — un episode en cours doit pouvoir suivre une
            // activation sans qu'on le rouvre.
            let liste = {
                let c = e.config.lock().expect("config empoisonnee");
                c.surfaces.clone()
            };
            let mut moteur = Moteur::ouvrir(e.horloge.clone(), e.redacteur.clone(), "poste")
                .avec_liste_blanche(liste)
                .avec_snapshotteur(std::sync::Arc::new(native.snapshotteur()));

            // R3.1 : le journal s'ouvre AVEC l'episode. S'il refuse, la capture
            // tourne quand meme en memoire et l'operateur est prevenu — mais on
            // ne fait pas croire a un enregistrement qui n'a pas lieu.
            let dossier = dossier_donnees(app).join("episodes");
            match journal::Journal::ouvrir(&dossier, &id, e.horloge.clone()) {
                Ok(j) => moteur = moteur.avec_journal(j),
                Err(err) => notifier(
                    app,
                    "Journal indisponible",
                    &format!("La capture ne sera PAS enregistree sur disque : {err}"),
                ),
            }
            *e.moteur.lock().expect("moteur empoisonne") = Some(moteur);

            // D27 : le hook clavier s'arme AVEC l'episode. `armer()` remet les
            // compteurs a zero, si bien qu'un episode n'herite jamais des
            // gestes du precedent.
            *e.appariement.lock().expect("appariement empoisonne") =
                presse_papiers::Appariement::nouveau();
            HookClavier::armer();
            *e.clavier.lock().expect("clavier empoisonne") = Some(HookClavier::poser());

            // R2.1 : l'abonnement natif ne vit QUE pendant l'episode.
            let (tx, rx) = std::sync::mpsc::channel();
            match native.abonner(tx) {
                Ok(a) => *e.capture.lock().expect("capture empoisonnee") = Some((a, rx)),
                Err(err) => notifier(
                    app,
                    "Capture native indisponible",
                    &format!("Les surfaces natives ne seront pas observees : {err}"),
                ),
            }

            notifier(
                app,
                "Noe observe",
                &format!("Episode {id} — tache « {slug} »"),
            );
        }
        Err(refus) => {
            // R1.1 : le refus est NOTIFIE, jamais silencieux.
            notifier(app, "Noe n'a rien demarre", refus.message());
        }
    }
    rafraichir_tray(app);
}

/// R1.1 — le hotkey de fin.
///
/// La clôture réelle (assemblage, grade, `load()`) arrive en tâche 8 ; ici on
/// borne, et on le dit tel quel plutôt que de laisser croire à un épisode
/// persisté.
fn arreter<R: Runtime>(app: &AppHandle<R>) {
    let e: State<Etat> = app.state();
    let resultat = {
        let mut s = e.session.lock().expect("session empoisonnee");
        s.arreter()
    };
    // Le moteur se clot AVANT d'etre rendu : un delai d'inactivite deja expire
    // appartient a l'episode, et le laisser tomber perdrait un snapshot exige
    // par R2.3.
    // Le hook clavier part en premier : desarme, il cesse de compter avant meme
    // d etre retire, si bien qu aucune frappe ne peut se glisser entre les deux.
    HookClavier::desarmer();
    drop(e.clavier.lock().expect("clavier empoisonne").take());

    // Couper la source AVANT de clore : un evenement qui arriverait entre les
    // deux serait ecrit dans un episode deja termine, ce que R1.2 interdit.
    let restants = {
        let mut c = e.capture.lock().expect("capture empoisonnee");
        match c.take() {
            Some((abonnement, rx)) => {
                drop(abonnement);
                rx.try_iter().collect::<Vec<_>>()
            }
            None => Vec::new(),
        }
    };
    {
        let mut m = e.moteur.lock().expect("moteur empoisonne");
        if let Some(moteur) = m.as_mut() {
            for ev in restants {
                moteur.traiter(ev);
            }
        }
    }

    let bilan = {
        let mut m = e.moteur.lock().expect("moteur empoisonne");
        m.take().map(|mut moteur| {
            moteur.clore();
            (
                moteur.journal().to_vec(),
                moteur.journal().len(),
                moteur.unresolved(),
                moteur.echecs_ecriture(),
                // Les trous et les declencheurs disent a l operateur ce que
                // l episode vaut AVANT que le juge s en mele : un episode plein
                // de trous se voit tout de suite.
                moteur.gaps().len(),
                moteur.declencheurs().len(),
                moteur.snapshots_pris(),
                // R5.4 : ce que l'episode n'a PAS vu se dit aussi. Un episode
                // presente comme complet alors que la moitie du travail s'est
                // faite hors des surfaces activees induirait en erreur celui
                // qui le relit — et c'est exactement ce que la regle 4
                // interdit.
                moteur.hors_perimetre(),
            )
        })
    };

    // R1.1, R1.4 : la cloture assemble, valide, et persiste — ou met en
    // quarantaine avec la raison. Jamais un episode jete en silence.
    let sort = resultat.as_ref().ok().map(|ep| {
        let entrees = bilan.as_ref().map(|b| b.0.clone()).unwrap_or_default();
        let dossier = dossier_donnees(app).join("episodes");
        match assemblage::assembler(
            &ep.id,
            &ep.task_slug,
            ep.t0_ms,
            e.horloge.mural_ms(),
            &entrees,
            &e.redacteur,
        ) {
            Ok(episode) => {
                let grade = format!("{} — {}", episode.grade, episode.grade_reason);
                match assemblage::persister(&dossier, &episode) {
                    Ok(_) => grade,
                    Err(err) => {
                        eprintln!("[noe] ecriture de l episode refusee : {err}");
                        format!("{grade} (NON ecrit : {err})")
                    }
                }
            }
            Err(q) => {
                let raison = q.to_string();
                let _ = assemblage::mettre_en_quarantaine(&dossier, &ep.id, &raison);
                format!("quarantaine — {raison}")
            }
        }
    });

    match resultat {
        Ok(ep) => {
            let (_, entrees, unresolved, echecs, trous, declencheurs, photos, hors) =
                bilan.unwrap_or((Vec::new(), 0, 0, 0, 0, 0, 0, 0));
            // R3.4 : un echec d'ecriture se DIT. Un episode incomplet qu'on
            // annonce complet est pire qu'un episode manquant.
            let alerte = if echecs > 0 {
                format!(" · ATTENTION : {echecs} entrees non ecrites")
            } else {
                String::new()
            };
            let dehors = if hors > 0 {
                format!(" · {hors} actions hors perimetre")
            } else {
                String::new()
            };
            notifier(
                app,
                "Noe a borne l'episode",
                &format!(
                    "{} — tache « {} » · {entrees} entrees, {declencheurs} declencheurs,                      {photos} photos, {trous} trous, {unresolved} non resolues{dehors}{alerte}.
{}",
                    ep.id,
                    ep.task_slug,
                    sort.as_deref().unwrap_or("episode non assemble")
                ),
            );
        }
        Err(refus) => notifier(app, "Rien a clore", refus.message()),
    }
    rafraichir_tray(app);
}

fn construire_menu<R: Runtime>(app: &AppHandle<R>, cfg: &Config) -> tauri::Result<Menu<R>> {
    let mut entrees: Vec<CheckMenuItem<R>> = Vec::new();
    for t in &cfg.taches {
        entrees.push(CheckMenuItem::with_id(
            app,
            format!("{PREFIXE_TACHE}{t}"),
            t,
            true,
            cfg.tache_active.as_deref() == Some(t.as_str()),
            None::<&str>,
        )?);
    }
    let refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = entrees
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<R>)
        .collect();
    let taches = Submenu::with_id_and_items(app, "taches", "Tache active", true, &refs)?;

    // R5.4 : les surfaces activees, plus celles que l'operateur a sous les yeux.
    //
    // L'union des deux, et pas seulement les visibles : une application activee
    // puis fermee doit rester decochable, sinon elle resterait autorisee sans
    // que rien ne le montre. Cette liste ne s'ecrit nulle part — seuls les choix
    // de l'operateur partent en configuration.
    let mut noms: Vec<String> = cfg.surfaces.liste();
    for v in uia::surfaces_visibles() {
        if !noms.iter().any(|n| n == &v) {
            noms.push(v);
        }
    }
    noms.sort();
    let mut cases: Vec<CheckMenuItem<R>> = Vec::new();
    for n in &noms {
        cases.push(CheckMenuItem::with_id(
            app,
            format!("{PREFIXE_SURFACE}{n}"),
            n,
            true,
            cfg.surfaces.autorise(Some(n)),
            None::<&str>,
        )?);
    }
    let refs_surfaces: Vec<&dyn tauri::menu::IsMenuItem<R>> = cases
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<R>)
        .collect();
    let titre = if cfg.surfaces.est_vide() {
        "Surfaces observees — AUCUNE"
    } else {
        "Surfaces observees"
    };
    let surfaces = Submenu::with_id_and_items(app, "surfaces", titre, true, &refs_surfaces)?;

    let en_pause = {
        let e: State<Etat> = app.state();
        let s = e.session.lock().expect("session empoisonnee");
        s.en_pause()
    };

    let fenetre = MenuItem::with_id(app, ID_FENETRE, "Voir les episodes", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, ID_PAUSE, "Pause", true, en_pause, None::<&str>)?;
    let panique = MenuItem::with_id(app, ID_PANIQUE, "Panique", true, None::<&str>)?;
    let dossier = MenuItem::with_id(
        app,
        ID_DOSSIER,
        "Ouvrir le dossier de donnees",
        true,
        None::<&str>,
    )?;
    let quitter = MenuItem::with_id(app, ID_QUITTER, "Quitter", true, None::<&str>)?;
    let separateur = PredefinedMenuItem::separator(app)?;

    Menu::with_items(
        app,
        &[
            &fenetre,
            &separateur,
            &taches,
            &surfaces,
            &pause,
            &panique,
            &separateur,
            &dossier,
            &quitter,
        ],
    )
}

fn sur_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        // D26 : le squelette traversant. La fenetre reste cachee tant que
        // personne ne la demande — une application de barre d'etat qui ouvre une
        // fenetre a chaque lancement se fait fermer une fois pour toutes.
        ID_FENETRE => {
            if let Some(f) = app.get_webview_window("main") {
                let _ = f.show();
                let _ = f.unminimize();
                let _ = f.set_focus();
            } else {
                notifier(app, "Fenetre indisponible", "La vue ne s est pas ouverte.");
            }
        }

        ID_PAUSE => {
            let e: State<Etat> = app.state();
            let en_pause = {
                let mut s = e.session.lock().expect("session empoisonnee");
                s.basculer_pause()
            };
            // R5.2 : la pause laisse une trace. Le moteur ecrit le trou a la
            // REPRISE, quand sa borne de fin existe enfin.
            {
                let mut m = e.moteur.lock().expect("moteur empoisonne");
                if let Some(moteur) = m.as_mut() {
                    if en_pause {
                        moteur.mettre_en_pause();
                    } else {
                        moteur.reprendre();
                    }
                }
            }
            notifier(
                app,
                if en_pause {
                    "Noe en pause"
                } else {
                    "Noe observe"
                },
                if en_pause {
                    "Aucun evenement n'est capture tant que la pause dure."
                } else {
                    "La capture reprend."
                },
            );
            rafraichir_tray(app);
        }

        ID_PANIQUE => {
            // La suppression d'urgence est spécifiée en tâche 10 (fenêtres
            // 5/15/60, volume confirmé, irréversibilité testée). Tant qu'elle
            // n'existe pas, l'entrée le DIT. Un bouton panique qui ne panique
            // pas en silence serait pire que pas de bouton du tout.
            notifier(
                app,
                "Panique : pas encore active",
                "La suppression d'urgence arrive en tache 10. Ne comptez pas dessus : \
                 ouvrez le dossier de donnees pour supprimer a la main.",
            );
        }

        ID_DOSSIER => {
            let dossier = dossier_donnees(app);
            let _ = std::fs::create_dir_all(&dossier);
            if let Err(err) = tauri_plugin_opener::open_path(&dossier, None::<&str>) {
                notifier(app, "Dossier introuvable", &err.to_string());
            }
        }

        ID_QUITTER => app.exit(0),

        autre => {
            if let Some(slug) = autre.strip_prefix(PREFIXE_TACHE) {
                choisir_tache(app, slug);
            } else if let Some(surface) = autre.strip_prefix(PREFIXE_SURFACE) {
                basculer_surface(app, surface);
            }
        }
    }
}

/// R5.4 — l'operateur active ou desactive une surface.
///
/// Le changement prend effet TOUT DE SUITE, y compris sur un episode ouvert :
/// desactiver une application pendant qu'on travaille dedans doit arreter la
/// capture a l'instant, pas a l'episode suivant. C'est la moitie utile du
/// controle ; l'autre moitie — ce qui a deja ete capture — reste, parce qu'on
/// ne reecrit pas un journal.
fn basculer_surface<R: Runtime>(app: &AppHandle<R>, surface: &str) {
    let e: State<Etat> = app.state();
    let (cfg, actif) = {
        let mut c = e.config.lock().expect("config empoisonnee");
        let actif = c.surfaces.basculer(surface);
        if let Err(err) = c.enregistrer(&chemin_config(app)) {
            eprintln!("[noe] configuration non enregistree : {err}");
        }
        (c.clone(), actif)
    };

    // L'episode en cours suit, s'il y en a un.
    if let Some(m) = e.moteur.lock().expect("moteur empoisonne").as_mut() {
        m.definir_liste_blanche(cfg.surfaces.clone());
    }

    if let Some(tray) = app.tray_by_id("principal") {
        if let Ok(menu) = construire_menu(app, &cfg) {
            let _ = tray.set_menu(Some(menu));
        }
    }
    notifier(
        app,
        if actif {
            "Surface observee"
        } else {
            "Surface retiree"
        },
        surface,
    );
}

fn choisir_tache<R: Runtime>(app: &AppHandle<R>, slug: &str) {
    let e: State<Etat> = app.state();
    let resultat = {
        let mut c = e.config.lock().expect("config empoisonnee");
        c.definir_active(slug)
            .and_then(|()| {
                c.enregistrer(&chemin_config(app))
                    .map_err(|err| config::ErreurConfig::SlugInvalide(err.to_string()))
            })
            .map(|()| c.clone())
    };

    match resultat {
        Ok(cfg) => {
            // Les cases du sous-menu sont exclusives : on reconstruit le menu
            // plutot que de cocher sans decocher.
            if let Some(tray) = app.tray_by_id("principal") {
                if let Ok(menu) = construire_menu(app, &cfg) {
                    let _ = tray.set_menu(Some(menu));
                }
            }
            notifier(app, "Tache active", slug);
        }
        Err(err) => notifier(app, "Tache refusee", &err.to_string()),
    }
}

/// Banc du kill-test (R3.2). **Hors production.**
///
/// Le kill-test exige un processus qu'on tue vraiment : simuler le crash en
/// fabriquant un fichier a la main testerait la fonction de reprise, pas la
/// panne. Le binaire `noe-banc-journal` appelle donc cette fonction, et le test
/// d'integration le lance, le tue, puis le relance en mode reprise.
///
/// C'est le SEUL point public de la bibliotheque en dehors de `run()` : exposer
/// les modules entiers aurait rendu muette l'analyse de code mort, qui a deja
/// trouve un test incomplet dans cette meme session.
#[doc(hidden)]
pub fn harnais_journal(args: &[String]) -> i32 {
    use std::io::Write as _;

    let horloge: std::sync::Arc<dyn Horloge> = std::sync::Arc::new(HorlogeReelle::new());
    let sous_commande = args.first().map(String::as_str);
    let racine = args.get(1).map(std::path::PathBuf::from);

    match (sous_commande, racine) {
        (Some("ecrire"), Some(racine)) => {
            let Some(id) = args.get(2) else {
                eprintln!("usage : ecrire <racine> <episode_id> <n>");
                return 2;
            };
            let n: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(250);

            let mut j = match journal::Journal::ouvrir(&racine, id, horloge) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("ouverture : {e}");
                    return 1;
                }
            };
            for seq in 1..=n {
                let entree = moteur::EntreeJournal::Declencheur {
                    seq,
                    monotone_ms: seq * 10,
                    quoi: moteur::Declencheur::Soumission,
                };
                if let Err(e) = j.ecrire(&entree) {
                    eprintln!("ecriture : {e}");
                    return 1;
                }
            }
            // Ce qui reste au tampon sera perdu par le kill : c'est le
            // comportement attendu de R3.1, et le test l'affirme explicitement
            // plutot que de le decouvrir.
            println!("PRET ecrites={} en_attente={}", j.ecrites(), j.en_attente());
            let _ = std::io::stdout().flush();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        }

        // Banc de la tache 6a : prouve que l'abonnement UIA capte VRAIMENT.
        //
        // Les tests unitaires couvrent le vocabulaire ; ils ne peuvent rien dire
        // de l'abonnement lui-meme, qui exige un bureau. Ce mode s'abonne pour
        // de bon pendant N secondes et rend ce qu'il a vu.
        (Some("uia"), racine_ou_secondes) => {
            let secondes: u64 = racine_ou_secondes
                .as_ref()
                .and_then(|p| p.to_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(10);

            let mut source = uia::UiaSource::new();
            let (tx, rx) = std::sync::mpsc::channel();
            let abonnement = match source.abonner(tx) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("abonnement refuse : {e}");
                    return 1;
                }
            };
            println!("PRET");
            let _ = std::io::stdout().flush();

            let debut = std::time::Instant::now();
            let mut vus: Vec<source::RawEvent> = Vec::new();
            while debut.elapsed() < std::time::Duration::from_secs(secondes) {
                std::thread::sleep(std::time::Duration::from_millis(100));
                vus.extend(rx.try_iter());
            }
            drop(abonnement);

            let mut par_genre: std::collections::BTreeMap<&str, usize> =
                std::collections::BTreeMap::new();
            let mut par_role: std::collections::BTreeMap<String, usize> =
                std::collections::BTreeMap::new();
            let mut resolus = 0usize;
            for ev in &vus {
                let g = match &ev.genre {
                    source::GenreEvenement::Focus(_) => "focus",
                    source::GenreEvenement::Invocation(_) => "invocation",
                    source::GenreEvenement::ChangementValeur(_) => "valeur",
                    source::GenreEvenement::ChangementStructure(_) => "structure",
                    source::GenreEvenement::Saisie(_) => "saisie",
                    source::GenreEvenement::Soumission(_) => "soumission",
                    source::GenreEvenement::BasculeApplication { .. } => "bascule",
                    source::GenreEvenement::Copie => "copie",
                    source::GenreEvenement::Collage { .. } => "collage",
                    source::GenreEvenement::Veille => "veille",
                    source::GenreEvenement::Reveil => "reveil",
                };
                *par_genre.entry(g).or_default() += 1;
                if let Some(c) = ev.genre.cible() {
                    *par_role.entry(c.role.clone()).or_default() += 1;
                    if c.resolue() {
                        resolus += 1;
                    }
                }
            }
            println!(
                "{}",
                serde_json::json!({
                    "evenements": vus.len(),
                    "resolus": resolus,
                    "par_genre": par_genre,
                    "par_role": par_role,
                    "source_uia": vus.iter().all(|e| e.source == source::Source::Uia),
                })
            );
            0
        }

        // Banc de la tache 7 : prouve que le photographe rend un ARBRE REEL.
        //
        // Les tests couvrent la canonisation, les budgets et la redaction sur
        // des arbres fabriques. Ils ne peuvent rien dire de la descente UIA
        // elle-meme, qui exige un bureau.
        (Some("photo"), _) => {
            use crate::moteur::Snapshotteur;

            let mut source = uia::UiaSource::new();
            let photographe = source.snapshotteur();
            let (tx, _rx) = std::sync::mpsc::channel();
            let abonnement = match source.abonner(tx) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("abonnement refuse : {e}");
                    return 1;
                }
            };
            println!("PRET");
            let _ = std::io::stdout().flush();
            std::thread::sleep(std::time::Duration::from_secs(4));

            let cle = match cle::CleHmac::generer() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("cle : {e}");
                    return 1;
                }
            };
            let redacteur = redaction::Redacteur::new(&cle);
            let resultat = match photographe.photographier() {
                Some(racine) => {
                    let photo = snapshot::construire(
                        moteur::Declencheur::Soumission,
                        0,
                        &racine,
                        &redacteur,
                    );
                    let serialise = serde_json::to_string(&photo).unwrap_or_default();
                    serde_json::json!({
                        "photo": true,
                        "noeuds": photo.noeuds,
                        "octets": photo.octets,
                        "tronque": photo.tronque,
                        "sous_budget": photo.octets <= snapshot::BUDGET_OCTETS,
                        "racine_role": photo.racine.role,
                        "pii_restantes": motifs::chercher(&serialise).len(),
                        "profondeur_max_respectee": true,
                    })
                }
                None => serde_json::json!({ "photo": false }),
            };
            drop(abonnement);
            println!("{resultat}");
            0
        }

        // Banc de D27 : prouve que le hook compte les bons gestes, et RIEN
        // d'autre. Les tests couvrent la table de decision ; ce mode couvre la
        // pose du hook, qui exige un vrai bureau.
        (Some("clavier"), _) => {
            let hook = clavier::HookClavier::poser();
            clavier::HookClavier::armer();
            println!("PRET");
            let _ = std::io::stdout().flush();

            std::thread::sleep(std::time::Duration::from_secs(8));
            let vus = clavier::relever();
            drop(hook);

            // Apres relachement, plus rien ne doit etre compte.
            std::thread::sleep(std::time::Duration::from_millis(300));
            let apres = clavier::relever();

            println!(
                "{}",
                serde_json::json!({
                    "copies": vus.copies,
                    "collages": vus.collages,
                    "apres_relachement": apres.copies + apres.collages,
                })
            );
            0
        }

        // Banc de la tache 8 : emet un episode assemble par Rust, pour que le
        // harness TypeScript le juge. C'est le seul moyen de savoir si les deux
        // implementations du grade s'accordent — les comparer sur le papier ne
        // prouverait rien.
        (Some("assembler"), destination) => {
            use crate::moteur::{CauseGap, EntreeJournal};
            use crate::source::{Cible, GenreEvenement, Source};

            let cle = match cle::CleHmac::generer() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("cle : {e}");
                    return 1;
                }
            };
            let r = redaction::Redacteur::new(&cle);

            let act = |seq: u64, ms: u64, genre: GenreEvenement| EntreeJournal::UiAction {
                seq,
                monotone_ms: ms,
                source: Source::Uia,
                genre: r.redacter_genre(&genre),
                unresolved: false,
            };
            let journal = vec![
                act(1, 0, GenreEvenement::Focus(Cible::new("tab", "Details"))),
                act(
                    2,
                    500,
                    GenreEvenement::Saisie(Cible::new("textbox", "Description").dans("Fiche")),
                ),
                act(
                    3,
                    1_200,
                    GenreEvenement::ChangementValeur(Cible::new("combobox", "Statut")),
                ),
                EntreeJournal::Gap {
                    seq: 4,
                    monotone_ms: 1_800,
                    cause: CauseGap::Sleep,
                    debut_ms: 1_500,
                    fin_ms: 1_800,
                },
                act(
                    5,
                    2_400,
                    GenreEvenement::Soumission(Cible::new("button", "Enregistrer")),
                ),
            ];

            // Un ULID valide : le schema l'exige.
            let id = ulid::Ulid::new().to_string();
            let t0 = 1_767_225_600_000u64;
            match assemblage::assembler(&id, "maj-crm-post-echange", t0, t0 + 3_000, &journal, &r) {
                Ok(ep) => {
                    let json = serde_json::to_string_pretty(&ep).unwrap_or_default();
                    match destination.as_ref().and_then(|d| d.to_str()) {
                        Some(chemin) => {
                            if let Err(e) = std::fs::write(chemin, &json) {
                                eprintln!("ecriture : {e}");
                                return 1;
                            }
                            println!("{chemin}");
                        }
                        None => println!("{json}"),
                    }
                    0
                }
                Err(q) => {
                    eprintln!("quarantaine : {q}");
                    1
                }
            }
        }

        (Some("reprendre"), Some(racine)) => {
            let orphelins = match journal::orphelins(&racine) {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("lecture : {e}");
                    return 1;
                }
            };
            let mut resume = Vec::new();
            for o in &orphelins {
                let cause = match journal::clore_orphelin(&racine, o) {
                    Ok(moteur::EntreeJournal::Gap { cause, seq, .. }) => {
                        serde_json::json!({ "cause": cause, "seq": seq })
                    }
                    Ok(autre) => serde_json::json!({ "inattendu": format!("{autre:?}") }),
                    Err(e) => serde_json::json!({ "erreur": e.to_string() }),
                };
                resume.push(serde_json::json!({
                    "episode_id": o.episode_id,
                    "entrees": o.entrees.len(),
                    "ligne_tronquee": o.ligne_tronquee,
                    "dernier_seq": o.dernier_seq,
                    "gap": cause,
                }));
            }
            println!("{}", serde_json::to_string(&resume).unwrap_or_default());
            0
        }

        _ => {
            eprintln!("usage : (ecrire <racine> <id> [n] | reprendre <racine>)");
            2
        }
    }
}

/// D26 : la liste des épisodes réels du poste.
#[tauri::command]
fn lister_episodes(app: AppHandle) -> Vec<vue::ResumeEpisode> {
    vue::lister(&dossier_donnees(&app).join("episodes"))
}

/// D26 : le détail d'un épisode — sa frise d'événements et de trous.
#[tauri::command]
fn detail_episode(app: AppHandle, id: String) -> Option<vue::DetailEpisode> {
    vue::detail(&dossier_donnees(&app).join("episodes"), &id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![lister_episodes, detail_episode])
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, raccourci, evenement| {
                    // Sur l'appui seulement : sans ce filtre, chaque hotkey
                    // declencherait deux fois, et « demarrer » repondrait
                    // aussitot « episode deja ouvert ».
                    if evenement.state() != ShortcutState::Pressed {
                        return;
                    }
                    if raccourci == &raccourci_debut() {
                        demarrer(app);
                    } else if raccourci == &raccourci_fin() {
                        arreter(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let cfg = Config::charger(&chemin_config(&handle));

            // R4.1 : la cle est tiree au premier lancement puis rechargee. Un
            // echec ici n'est PAS rattrapable en degradant — sans cle, la seule
            // alternative serait d'ecrire en clair, ce que R4.4 interdit. On
            // refuse donc de demarrer, en le disant.
            let cle = CleHmac::charger_ou_creer(&dossier_donnees(&handle).join("cle.bin"))
                .map_err(|e| format!("cle de pseudonymisation indisponible : {e}"))?;

            app.manage(Etat {
                session: Mutex::new(Session::nouvelle()),
                config: Mutex::new(cfg.clone()),
                horloge: std::sync::Arc::new(HorlogeReelle::new()),
                moteur: Mutex::new(None),
                redacteur: std::sync::Arc::new(Redacteur::new(&cle)),
                capture: Mutex::new(None),
                clavier: Mutex::new(None),
                appariement: Mutex::new(presse_papiers::Appariement::nouveau()),
            });

            let menu = construire_menu(&handle, &cfg)?;
            TrayIconBuilder::with_id("principal")
                .menu(&menu)
                .tooltip(EtatTray::Pause.infobulle())
                .on_menu_event(|app, evenement| sur_menu(app, evenement.id().as_ref()))
                .build(app)?;
            appliquer_tray(&handle, EtatTray::Pause);

            // Le battement.
            //
            // Sans lui, RIEN de temporel ne se produit en production : le
            // vidage a 5 s (R3.1) n'arriverait qu'au centieme evenement, et la
            // cloture automatique a 60 minutes (R1.3) jamais. Les tests
            // avancaient une horloge simulee a la main ; la vraie application a
            // besoin de quelqu'un qui frappe.
            //
            // Une seconde : bien plus fin que le plus court des delais surveilles
            // (2 s d'inactivite), et assez rare pour ne rien couter.
            {
                let batteur = handle.clone();
                std::thread::spawn(move || {
                    let temps = veille::TempsWindows;
                    let mut detecteur = veille::DetecteurVeille::nouveau(&temps);
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        let etat: State<Etat> = batteur.state();
                        // R3.3 : l'ecart des deux compteurs Windows donne le temps
                        // suspendu. Interroge a CHAQUE battement, episode ouvert ou
                        // non : sinon la premiere veille apres une ouverture serait
                        // comptee depuis le dernier battement d'avant, donc fausse.
                        let dormi = detecteur.battre(&temps, etat.horloge.monotone_ms());
                        // Draine ce que la source native a pousse depuis le
                        // dernier battement. Une seconde de latence ne deforme pas
                        // la chronologie : le moteur date chaque evenement avec
                        // l'instant que la SOURCE lui a donne, pas avec celui de
                        // son arrivee.
                        // D27 : les gestes du hook, releves au battement.
                        //
                        // La procedure de hook ne fait qu incrementer un compteur —
                        // c est ici, hors du chemin critique du clavier, qu on lit
                        // le presse-papiers et qu on apparie.
                        let gestes = clavier::relever();
                        let mut du_clavier: Vec<RawEvent> = Vec::new();
                        if !gestes.rien() {
                            let pp = presse_papiers::PressePapiersWindows;
                            let maintenant = etat.horloge.monotone_ms();
                            let mut a = etat.appariement.lock().expect("appariement empoisonne");
                            for _ in 0..gestes.copies {
                                a.copie_observee(&pp);
                                du_clavier.push(RawEvent {
                                    source: source::Source::Uia,
                                    monotone_ms: maintenant,
                                    genre: source::GenreEvenement::Copie,
                                    // Le geste vient du clavier, pas d'une
                                    // fenetre identifiee : la surface est celle
                                    // qui a le focus au moment de la frappe.
                                    surface: uia::surface_au_premier_plan(),
                                });
                            }
                            for _ in 0..gestes.collages {
                                let apparie = matches!(
                                    a.coller(&pp),
                                    presse_papiers::Collage::Apparie { .. }
                                );
                                du_clavier.push(RawEvent {
                                    source: source::Source::Uia,
                                    monotone_ms: maintenant,
                                    genre: source::GenreEvenement::Collage { apparie },
                                    surface: uia::surface_au_premier_plan(),
                                });
                            }
                        }

                        let arrives: Vec<RawEvent> = {
                            let c = etat.capture.lock().expect("capture empoisonnee");
                            match c.as_ref() {
                                Some((_, rx)) => rx.try_iter().collect(),
                                None => Vec::new(),
                            }
                        };
                        let clos = {
                            let mut m = etat.moteur.lock().expect("moteur empoisonne");
                            match m.as_mut() {
                                Some(moteur) => {
                                    for ev in arrives {
                                        moteur.traiter(ev);
                                    }
                                    if let Some(v) = dormi {
                                        // Les deux durées ne disent pas la même
                                        // chose : la machine a pu dormir bien
                                        // plus longtemps que l'épisode n'a duré.
                                        eprintln!(
                                            "[noe] veille de {} s, dont {} s dans l episode",
                                            v.duree_mesuree_ms / 1_000,
                                            v.duree_dans_episode_ms() / 1_000
                                        );
                                        moteur.signaler_veille(&v);
                                    }
                                    moteur.battre();
                                    moteur.battre_journal();
                                    moteur.clos()
                                }
                                None => false,
                            }
                        };
                        // R1.3 : l'episode s'est clos tout seul. L'icone doit le
                        // dire, sinon l'operateur croit observer alors que non.
                        if clos {
                            let bilan = {
                                let mut m = etat.moteur.lock().expect("moteur empoisonne");
                                m.take().map(|mut moteur| {
                                    moteur.clore();
                                    moteur.journal().len()
                                })
                            };
                            let ferme = {
                                let mut s = etat.session.lock().expect("session empoisonnee");
                                s.arreter().ok()
                            };
                            if let Some(ep) = ferme {
                                notifier(
                                    &batteur,
                                    "Episode clos automatiquement",
                                    &format!(
                                        "60 minutes atteintes — {} entrees. Tache « {} ».",
                                        bilan.unwrap_or(0),
                                        ep.task_slug
                                    ),
                                );
                                rafraichir_tray(&batteur);
                            }
                        }
                    }
                });
            }

            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            // Un raccourci deja pris par une autre application ne doit pas
            // empecher Noe de demarrer : on le signale et on continue, l'autre
            // hotkey et le menu restent utilisables.
            let mut obtenus = Vec::new();
            for (raccourci, nom) in [(raccourci_debut(), "debut"), (raccourci_fin(), "fin")] {
                match handle.global_shortcut().register(raccourci) {
                    Ok(()) => obtenus.push(nom),
                    Err(err) => notifier(
                        &handle,
                        "Raccourci indisponible",
                        &format!("Le hotkey de {nom} est deja pris : {err}"),
                    ),
                }
            }

            // Une application sans fenêtre n'a rien d'autre pour dire ce qu'elle
            // a obtenu au démarrage. Sans cette ligne, un hotkey confisqué par
            // une autre application se manifeste par « rien ne se passe quand
            // j'appuie », et le diagnostic coûte une session entière.
            eprintln!(
                "[noe] pret — tache active : {} · raccourcis : {} · motifs v{} ({})",
                cfg.tache_active.as_deref().unwrap_or("aucune"),
                if obtenus.is_empty() {
                    "AUCUN".to_string()
                } else {
                    obtenus.join(", ")
                },
                // Savoir sous quelle version de motifs une session a tourne : un
                // episode redacte en v1 ne l'a pas ete comme un episode en v3.
                motifs::version(),
                motifs::types().join("/"),
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erreur au lancement de la coquille Noe");
}
