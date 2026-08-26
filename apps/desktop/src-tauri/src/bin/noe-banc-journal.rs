//! Banc du kill-test du journal (spec 002, R3.2). **Hors production.**
//!
//! Ce binaire n'a qu'un rôle : être tué. Le test d'intégration le lance en mode
//! `ecrire`, attend qu'il annonce `PRET`, le tue sans ménagement, puis le
//! relance en mode `reprendre` pour vérifier que l'épisode orphelin est bien
//! détecté et clos avec son `gap`.
//!
//! Fabriquer le fichier à la main aurait testé la fonction de reprise ; ça
//! n'aurait rien dit de la panne. Un writer qui perd tout son tampon, ou qui
//! laisse un fichier verrouillé, ne se voit qu'en tuant un vrai processus.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(noe_desktop_lib::harnais_journal(&args));
}
