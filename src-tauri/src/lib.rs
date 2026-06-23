mod commands;
mod sync_executor;
mod views;

use std::sync::Mutex;
use syncnet::identity::Identity;
use syncstore::Db;

#[tauri::command]
fn ping() -> &'static str {
    "pong"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    // Initialize database
    let db_path = syncplatform::app_db_path();
    let db = Db::open(&db_path).expect("failed to open database");

    // Initialize or load identity
    let app_dir = db_path.parent().expect("db must have parent dir");
    let identity = Identity::load_or_generate(app_dir).expect("failed to load/generate identity");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(Mutex::new(db))
        .manage(Mutex::new(identity))
        .invoke_handler(tauri::generate_handler![
            ping,
            commands::list_profiles,
            commands::get_profile,
            commands::create_profile,
            commands::update_profile,
            commands::delete_profile,
            commands::list_peers,
            commands::pair_peer,
            commands::unpair_peer,
            commands::start_sync,
            commands::list_pending_deletions,
            commands::confirm_deletion,
            commands::reject_deletion,
            commands::get_sync_status,
            commands::get_drift_summary,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
