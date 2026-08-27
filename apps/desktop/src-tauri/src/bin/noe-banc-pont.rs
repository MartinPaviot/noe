//! Banc du pont DOM. **Hors production.**
//!
//! Sert le tuyau nommé et imprime, en JSON, ce qui en sort réellement. Il existe
//! parce que la tâche 6b ne peut pas se prouver en test unitaire : ce qu'on veut
//! savoir, c'est si Chrome lance bien l'hôte, si le service worker se connecte,
//! et si un `change` venu d'une racine shadow imbriquée arrive jusqu'ici.
//!
//! Il n'écrit rien sur le disque et ne garde rien : il imprime et il sort.
//!
//! ```text
//! noe-banc-pont.exe <secondes>
//! ```

use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use noe_desktop_lib::pont;

fn main() {
    let secondes: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let actif = Arc::new(AtomicBool::new(true));
    let recu: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let a = actif.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(secondes));
        a.store(false, Ordering::SeqCst);
        // Une connexion factice reveille `ConnectNamedPipe`, qui bloque.
        let _ = std::fs::OpenOptions::new().write(true).open(pont::TUYAU);
    });

    eprintln!("[banc] tuyau {} — {secondes} s", pont::TUYAU);
    let _ = std::io::stderr().flush();

    // Une instance en ecoute EN PERMANENCE, et un fil par connexion : Chrome
    // redemarre l'hote a chaque relance du service worker, et le nouveau se
    // presente pendant que l'ancien tient encore la connexion. Servir une
    // connexion a la fois faisait taire la capture pour le reste de l'episode.
    let lignes = recu.clone();
    let a = actif.clone();
    let fil = std::thread::spawn(move || {
        while a.load(Ordering::SeqCst) {
            let Some(tuyau) = pont::banc_tuyau() else {
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            };
            let lignes = lignes.clone();
            let a2 = a.clone();
            std::thread::spawn(move || {
                for ligne in BufReader::new(tuyau).lines().map_while(Result::ok) {
                    if !a2.load(Ordering::SeqCst) {
                        return;
                    }
                    eprintln!("[banc] {ligne}");
                    lignes.lock().expect("lignes").push(ligne);
                }
            });
        }
    });
    let _ = fil.join();

    let lignes = recu.lock().expect("lignes").clone();
    let mut suiveur = pont::Suiveur::default();
    let mut bilan = pont::Bilan::default();
    let mut genres: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut noms: Vec<String> = Vec::new();

    for l in &lignes {
        if let Ok(o) = serde_json::from_str::<pont::Observation>(l) {
            *genres.entry(o.genre.clone()).or_default() += 1;
            let nom = format!("{}|{}", o.cible.role, o.cible.nom);
            if !noms.contains(&nom) {
                noms.push(nom);
            }
        }
        pont::traiter_ligne(l, &mut suiveur, &mut bilan, 0);
    }

    println!(
        "{}",
        serde_json::json!({
            "lignes": lignes.len(),
            "recues": bilan.recues,
            "genres_inconnus": bilan.genres_inconnus,
            "ruptures": bilan.ruptures,
            "genres": genres,
            "ancrages": noms,
        })
    );
}
