//! Postrider's desktop app. It shows the web app (apps/web), opens links in
//! the browser and a mail's files from Downloads (`files`), and runs
//! Postrider's server for this Mac (`this_mac`), which the web app asks for
//! when it runs here.

// Don't open a console window next to the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod files;
mod this_mac;

use tauri::{Manager, RunEvent, State};
use this_mac::{Place, ThisMac};
use tracing_subscriber::EnvFilter;

/// Where Postrider runs on this Mac; waits while it starts.
#[tauri::command]
async fn this_mac(this_mac: State<'_, ThisMac>) -> Result<Option<Place>, String> {
    this_mac.place().await
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "postrider_desktop=info,postrider_server=info".into()),
        )
        .init();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(ThisMac::new(app.path().app_data_dir()?));
            app.state::<ThisMac>().resume();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![this_mac, files::open_file])
        .build(tauri::generate_context!())
        .expect("Postrider failed to start");
    app.run(|app, event| {
        if let RunEvent::Exit = event {
            tauri::async_runtime::block_on(app.state::<ThisMac>().stop());
        }
    });
}
