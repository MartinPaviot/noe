//! Coquille Tauri de Noe. Session 0 : aucune commande exposee.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("erreur au lancement de la coquille Noe");
}
