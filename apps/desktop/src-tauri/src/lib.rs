//! Coquille Tauri de Noe — barre d'état, menu, hotkeys de bornage.
//!
//! Spec 002, tâche 1. Ce fichier ne contient QUE du câblage : la logique de
//! bornage vit dans [`etat`], la persistance dans [`config`], et les deux se
//! testent sans écran. Tout ce qui décide vraiment quelque chose est ailleurs,
//! là où la CI peut l'atteindre.

mod config;
mod etat;
mod horloge;
mod moteur;
mod source;

use std::sync::Mutex;

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Manager, Runtime, State,
};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

use config::Config;
use etat::{EtatTray, Session};
use horloge::{Horloge, HorlogeReelle};
use moteur::Moteur;

/// Ctrl+Alt+D commence, Ctrl+Alt+F clôt. D comme début, F comme fin.
///
/// Des fonctions et non des constantes : `Shortcut::new` n'est pas `const`.
fn raccourci_debut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyD)
}

fn raccourci_fin() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyF)
}

const ID_PAUSE: &str = "pause";
const ID_PANIQUE: &str = "panique";
const ID_DOSSIER: &str = "dossier";
const ID_QUITTER: &str = "quitter";
/// Les entrées du sous-menu portent l'identifiant `tache:<slug>`.
const PREFIXE_TACHE: &str = "tache:";

struct Etat {
    session: Mutex<Session>,
    config: Mutex<Config>,
    /// Une seule horloge pour toute la vie du processus : le monotone n'a de
    /// sens que rapporte a une meme origine.
    horloge: std::sync::Arc<dyn Horloge>,
    /// Le moteur n'existe QUE pendant un episode. C'est la meme garantie que
    /// `Session` porte cote etat : hors episode, il n'y a rien ou ecrire.
    moteur: Mutex<Option<Moteur>>,
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
            *e.moteur.lock().expect("moteur empoisonne") =
                Some(Moteur::ouvrir(e.horloge.clone(), "poste"));
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
    let bilan = {
        let mut m = e.moteur.lock().expect("moteur empoisonne");
        m.take().map(|mut moteur| {
            moteur.clore();
            (moteur.journal().len(), moteur.unresolved())
        })
    };

    match resultat {
        Ok(ep) => {
            let (entrees, unresolved) = bilan.unwrap_or((0, 0));
            notifier(
                app,
                "Noe a borne l'episode",
                &format!(
                    "{} — tache « {} » · {entrees} entrees, {unresolved} non resolues.                      Assemblage et grade : tache 8.",
                    ep.id, ep.task_slug
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

    let en_pause = {
        let e: State<Etat> = app.state();
        let s = e.session.lock().expect("session empoisonnee");
        s.en_pause()
    };

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
        &[&taches, &pause, &panique, &separateur, &dossier, &quitter],
    )
}

fn sur_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        ID_PAUSE => {
            let e: State<Etat> = app.state();
            let en_pause = {
                let mut s = e.session.lock().expect("session empoisonnee");
                s.basculer_pause()
            };
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
            }
        }
    }
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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

            app.manage(Etat {
                session: Mutex::new(Session::nouvelle()),
                config: Mutex::new(cfg.clone()),
                horloge: std::sync::Arc::new(HorlogeReelle::new()),
                moteur: Mutex::new(None),
            });

            let menu = construire_menu(&handle, &cfg)?;
            TrayIconBuilder::with_id("principal")
                .menu(&menu)
                .tooltip(EtatTray::Pause.infobulle())
                .on_menu_event(|app, evenement| sur_menu(app, evenement.id().as_ref()))
                .build(app)?;
            appliquer_tray(&handle, EtatTray::Pause);

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
                "[noe] pret — tache active : {} · raccourcis : {}",
                cfg.tache_active.as_deref().unwrap_or("aucune"),
                if obtenus.is_empty() {
                    "AUCUN".to_string()
                } else {
                    obtenus.join(", ")
                }
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erreur au lancement de la coquille Noe");
}
