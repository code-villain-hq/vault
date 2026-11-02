//! Postrider's desktop app. It shows the web app (apps/web), opens links in
//! the browser and a mail's files from Downloads (`files`), notifies of new
//! mail (`notify`), and runs Postrider's server for this Mac (`this_mac`),
//! which the web app asks for when it runs here.

// Don't open a console window next to the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod files;
mod this_mac;

use tauri::{AppHandle, Manager, RunEvent, State};
use tauri_plugin_notification::NotificationExt;
use this_mac::{Place, ThisMac};
use tracing_subscriber::EnvFilter;

/// Where Postrider runs on this Mac; waits while it starts.
#[tauri::command]
async fn this_mac(this_mac: State<'_, ThisMac>) -> Result<Option<Place>, String> {
    this_mac.place().await
}

/// A notification for new mail. Outside an app bundle, in `tauri dev`, the
/// plugin shows it: its own never show on macOS (`code/kit/notify`).
#[tauri::command]
fn notify(app: AppHandle, title: String, body: String) {
    if code_villain_notify::show(&title, &body) {
        return;
    }
    if let Err(error) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!("couldn't show a notification: {error}");
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "postrider_desktop=info,postrider_server=info".into()),
        )
        .init();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // New mail notifies: macOS asks once whether it may.
            code_villain_notify::ask();
            app.manage(ThisMac::new(app.path().app_data_dir()?));
            app.state::<ThisMac>().resume();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![this_mac, notify, files::open_file])
        .build(tauri::generate_context!())
        .expect("Postrider failed to start");
    app.run(|app, event| {
        if let RunEvent::Exit = event {
            tauri::async_runtime::block_on(app.state::<ThisMac>().stop());
        }
    });
}
