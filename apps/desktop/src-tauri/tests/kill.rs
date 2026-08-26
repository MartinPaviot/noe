//! Le kill-test automatisé exigé par la tâche 4 (spec 002, R3.2).
//!
//! « Process tué mi-capture → reprise → orphelin clôturé avec gap crash. »
//!
//! Le processus est **réellement** tué. Écrire un journal incomplet à la main
//! aurait testé la fonction de reprise et rien d'autre : ni la perte du tampon,
//! ni un fichier resté verrouillé, ni un marqueur mal posé ne se manifestent
//! sans une vraie mort de processus.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const BANC: &str = env!("CARGO_BIN_EXE_noe-banc-journal");

/// Ce que le banc écrit avant de se faire tuer.
const ENTREES: u64 = 250;
/// Vidage tous les 100 : 200 atteignent le disque, 50 restent au tampon.
const ATTENDUES_SUR_DISQUE: usize = 200;

fn racine(nom: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("noe-kill-{nom}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Attend la ligne `PRET` — jamais un `sleep` arbitraire.
///
/// Dormir « assez longtemps » rendrait le test lent quand tout va bien et
/// instable quand la machine est chargée. On attend le signal que le banc émet.
fn attendre_pret(enfant: &mut Child) -> String {
    let sortie = enfant.stdout.take().expect("stdout du banc");
    let mut lecteur = BufReader::new(sortie);
    let debut = Instant::now();
    let mut ligne = String::new();

    while debut.elapsed() < Duration::from_secs(30) {
        ligne.clear();
        match lecteur.read_line(&mut ligne) {
            Ok(0) => break,
            Ok(_) if ligne.starts_with("PRET") => return ligne.trim().to_string(),
            Ok(_) => continue,
            Err(e) => panic!("lecture du banc : {e}"),
        }
    }
    panic!("le banc n a jamais annonce PRET");
}

#[test]
fn un_process_tue_en_pleine_capture_laisse_un_orphelin_clos_a_la_reprise() {
    let r = racine("nominal");

    let mut enfant = Command::new(BANC)
        .args([
            "ecrire",
            r.to_str().unwrap(),
            "ep-tue",
            &ENTREES.to_string(),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("lancement du banc");

    let pret = attendre_pret(&mut enfant);
    assert!(
        pret.contains(&format!("ecrites={ATTENDUES_SUR_DISQUE}")),
        "R3.1 : le vidage se fait tous les 100, obtenu « {pret} »"
    );
    assert!(
        pret.contains("en_attente=50"),
        "50 entrees doivent rester au tampon, obtenu « {pret} »"
    );

    // La mort brutale. Pas de signal, pas de destructeur, pas de fsync.
    enfant.kill().expect("kill");
    let _ = enfant.wait();

    // Le marqueur doit avoir survecu : c'est lui qui rend l'orphelin trouvable.
    assert!(
        r.join("ep-tue").join(".ouvert").exists(),
        "sans marqueur, l orphelin serait introuvable au demarrage suivant"
    );

    // La reprise, dans un processus NEUF — comme au redemarrage de l'app.
    let sortie = Command::new(BANC)
        .args(["reprendre", r.to_str().unwrap()])
        .output()
        .expect("reprise");
    assert!(sortie.status.success(), "reprise : {sortie:?}");

    let json = String::from_utf8_lossy(&sortie.stdout);
    let resume: serde_json::Value = serde_json::from_str(json.trim()).expect(&json);
    let orphelins = resume.as_array().expect("un tableau");

    assert_eq!(orphelins.len(), 1, "un seul orphelin attendu : {json}");
    let o = &orphelins[0];
    assert_eq!(o["episode_id"], "ep-tue");
    assert_eq!(
        o["entrees"], ATTENDUES_SUR_DISQUE,
        "seules les entrees videes survivent, et c'est le contrat de R3.1"
    );
    assert_eq!(
        o["ligne_tronquee"], false,
        "un vidage par blocs ne laisse pas de ligne coupee"
    );
    assert_eq!(o["gap"]["cause"], "crash", "R3.2 : gap{{cause:\"crash\"}}");
    assert_eq!(
        o["gap"]["seq"],
        ATTENDUES_SUR_DISQUE + 1,
        "R3.1 : le seq du gap suit le dernier seq sain"
    );

    // Le marqueur est retire : une seconde reprise ne doit RIEN retrouver,
    // sinon chaque demarrage rajouterait un gap au meme episode.
    assert!(!r.join("ep-tue").join(".ouvert").exists());
    let seconde = Command::new(BANC)
        .args(["reprendre", r.to_str().unwrap()])
        .output()
        .expect("seconde reprise");
    let json2 = String::from_utf8_lossy(&seconde.stdout);
    assert_eq!(
        json2.trim(),
        "[]",
        "la reprise doit etre idempotente : {json2}"
    );
}

#[test]
fn deux_episodes_tues_sont_tous_deux_repris() {
    let r = racine("deux");
    let mut enfants: Vec<Child> = Vec::new();

    for id in ["ep-a", "ep-b"] {
        let mut e = Command::new(BANC)
            .args(["ecrire", r.to_str().unwrap(), id, "120"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("lancement");
        attendre_pret(&mut e);
        enfants.push(e);
    }
    for mut e in enfants {
        let _ = e.kill();
        let _ = e.wait();
    }

    let sortie = Command::new(BANC)
        .args(["reprendre", r.to_str().unwrap()])
        .output()
        .expect("reprise");
    let json = String::from_utf8_lossy(&sortie.stdout);
    let resume: serde_json::Value = serde_json::from_str(json.trim()).expect(&json);
    let orphelins = resume.as_array().expect("un tableau");

    assert_eq!(orphelins.len(), 2, "les deux orphelins : {json}");
    for o in orphelins {
        assert_eq!(o["gap"]["cause"], "crash");
        assert_eq!(
            o["entrees"], 100,
            "vidage a 100, les 20 restants sont perdus"
        );
    }
}
