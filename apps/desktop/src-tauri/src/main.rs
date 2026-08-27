// Empeche l'ouverture d'une console Windows en release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // R6.1 : `noe export` et `noe import` passent par le meme executable.
    //
    // `main` ne lisait PAS argv, si bien que le bras de reprise de
    // `harnais_journal` etait inatteignable depuis le binaire livre — le
    // kill-test validait une fonction que personne n'appelait. On lit donc,
    // maintenant, et une commande inconnue le dit au lieu d'ouvrir une fenetre.
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        std::process::exit(noe_desktop_lib::cli(&args));
    }
    noe_desktop_lib::run()
}
