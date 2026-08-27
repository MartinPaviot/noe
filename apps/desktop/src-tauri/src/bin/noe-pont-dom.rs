//! L'hôte de native messaging : le seul morceau que Chrome lance lui-même.
//!
//! Il ne décide de rien. Il lit le protocole de Chrome sur son entrée standard —
//! une longueur sur 4 octets en petit-boutiste, puis autant d'octets de JSON — et
//! recopie chaque message, tel quel, dans le tuyau nommé de l'application.
//!
//! **Il est volontairement stupide.** Toute la logique — traduction, suivi de
//! numérotation, détection des trous — vit dans `pont.rs`, côté application, où
//! elle se teste sans navigateur. Un hôte qui interpréterait serait un troisième
//! endroit où la vérité pourrait diverger.
//!
//! Il n'écrit **rien** sur le disque, n'ouvre aucune connexion réseau, et ne
//! répond rien à Chrome. Si le tuyau est absent — l'application n'est pas lancée,
//! ou aucun épisode n'est ouvert — il jette le message et continue : ce n'est pas
//! sa place de décider qu'un épisode devrait exister.

use std::io::{Read, Write};

/// La borne du protocole de Chrome : 64 Mo côté navigateur, mais une observation
/// fait quelques centaines d'octets. Au-delà d'un mégaoctet, quelque chose ne va
/// pas et on préfère s'arrêter que d'allouer sur la foi d'un entier.
const TAILLE_MAX: u32 = 1024 * 1024;

fn main() {
    let mut entree = std::io::stdin().lock();
    let mut tuyau: Option<std::fs::File> = None;

    loop {
        let mut entete = [0u8; 4];
        if entree.read_exact(&mut entete).is_err() {
            // Chrome a fermé : l'onglet est parti, ou le service worker s'est
            // arrêté. C'est le mode de fin normal.
            return;
        }
        let taille = u32::from_le_bytes(entete);
        if taille == 0 || taille > TAILLE_MAX {
            eprintln!("[noe-pont] longueur refusee : {taille}");
            return;
        }

        let mut charge = vec![0u8; taille as usize];
        if entree.read_exact(&mut charge).is_err() {
            return;
        }

        // Le tuyau est rouvert à la demande : l'application peut avoir démarré
        // après Chrome, ou l'épisode s'être ouvert entre deux messages.
        if tuyau.is_none() {
            tuyau = std::fs::OpenOptions::new()
                .write(true)
                .open(noe_desktop_lib::pont::TUYAU)
                .ok();
        }

        let Some(t) = tuyau.as_mut() else {
            // Aucun épisode ouvert : le message est jeté, et c'est correct. Le
            // capteur observe en permanence ; c'est l'application qui décide
            // quand elle écoute.
            continue;
        };

        // Une observation par ligne : le serveur lit en lignes, et le JSON de
        // Chrome n'en contient jamais.
        let mut ligne = charge;
        ligne.push(b'\n');
        if t.write_all(&ligne).is_err() || t.flush().is_err() {
            // L'application s'est arrêtée, ou l'épisode s'est clos et le tuyau
            // avec lui. On oublie la poignée et on retentera au message suivant.
            tuyau = None;
        }
    }
}
