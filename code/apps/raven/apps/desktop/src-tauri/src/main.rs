//! Raven's desktop app. It shows the web app (apps/web) and adds links
//! opened in the browser, the session token kept in the system keychain, and
//! `raven://` links, which bring a sign-in in the browser back to the app;
//! the web app calls them when it runs here. It also runs Raven itself for
//! the workspaces on this Mac (`this_mac`).

// Don't open a console window next to the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod this_mac;

use raven_server::telemetry::{Capture, Recorder};
use tauri::{Manager, RunEvent, State};
use this_mac::{Place, Recording, ThisMac};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Keychain entries are named after the app, one per server.
const KEYCHAIN_SERVICE: &str = "Raven";

fn entry(server: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, server).map_err(|e| e.to_string())
}

/// The session token for a server, if signed in to it.
#[tauri::command]
fn session_token(server: String) -> Result<Option<String>, String> {
    match entry(&server)?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
fn save_session_token(server: String, token: String) -> Result<(), String> {
    entry(&server)?
        .set_password(&token)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn forget_session_token(server: String) -> Result<(), String> {
    match entry(&server)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// The workspaces on this Mac, once someone has started here; waits while
/// Raven starts.
#[tauri::command]
async fn this_mac(this_mac: State<'_, ThisMac>) -> Result<Option<Place>, String> {
    this_mac.place().await
}

/// "Start on this Mac", or a further workspace once started.
#[tauri::command]
async fn start_on_this_mac(
    this_mac: State<'_, ThisMac>,
    name: String,
    workspace_name: String,
) -> Result<Place, String> {
    this_mac.start_here(&name, &workspace_name).await
}

/// The Mac account's name, suggested as yours.
#[tauri::command]
fn mac_user_name() -> String {
    this_mac::account_name()
}

fn main() {
    // The app records its own calls and logs, as Raven's server does.
    let (recorder, records) = Recorder::new(env!("CARGO_PKG_VERSION"));
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer().with_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "raven_desktop=info,raven_server=info".into()),
            ),
        )
        .with(Capture::new(recorder.clone()))
        .init();
    let recording = Recording::new(recorder, records);
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(ThisMac::new(app.path().app_data_dir()?, recording));
            app.state::<ThisMac>().resume();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            session_token,
            save_session_token,
            forget_session_token,
            this_mac,
            start_on_this_mac,
            mac_user_name
        ])
        .build(tauri::generate_context!())
        .expect("Raven failed to start");
    app.run(|app, event| {
        if let RunEvent::Exit = event {
            tauri::async_runtime::block_on(app.state::<ThisMac>().stop());
        }
    });
}
