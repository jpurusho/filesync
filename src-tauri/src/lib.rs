mod commands;
pub mod network;
mod sync_executor;
mod sync_tracker;
mod views;

use network::SharedNetworkState;
use sync_tracker::SyncTracker;
use syncnet::identity::Identity;
use syncstore::Db;
use tauri::Manager;

#[tauri::command]
fn ping() -> &'static str {
    "pong"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    // Initialize database
    let db_path = syncplatform::app_db_path();

    // Create app directory if it doesn't exist
    let app_dir = db_path.parent().expect("db must have parent dir");
    std::fs::create_dir_all(app_dir).expect("failed to create app directory");

    let db = Db::open(&db_path).expect("failed to open database");

    // Initialize or load identity
    let identity = Identity::load_or_generate(app_dir).expect("failed to load/generate identity");

    // Create shared network state (filled async during setup)
    let shared_net = SharedNetworkState::new();

    // Create sync tracker for cancellation support
    let sync_tracker = SyncTracker::new();

    // Clone for async setup closure
    let identity_for_net = identity.clone();
    let db_for_net = db.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(db)
        .manage(identity)
        .manage(shared_net)
        .manage(sync_tracker)
        .setup(move |app| {
            let net_state: SharedNetworkState = app.state::<SharedNetworkState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                match network::start_network(&identity_for_net, &db_for_net).await {
                    Ok(state) => {
                        tracing::info!(
                            addr = %state.listen_addr,
                            "network services started"
                        );
                        net_state.set(state).await;
                    }
                    Err(e) => {
                        tracing::error!("failed to start network services: {e}");
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            commands::get_app_version,
            commands::list_profiles,
            commands::get_profile,
            commands::create_profile,
            commands::update_profile,
            commands::delete_profile,
            commands::list_peers,
            commands::pair_peer,
            commands::unpair_peer,
            commands::start_sync,
            commands::cancel_sync,
            commands::list_pending_deletions,
            commands::confirm_deletion,
            commands::reject_deletion,
            commands::get_sync_status,
            commands::get_drift_summary,
            commands::get_network_info,
            commands::list_discovered_peers,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
