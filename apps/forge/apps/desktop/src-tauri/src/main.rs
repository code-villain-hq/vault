//! Forge's desktop app. It shows the web app (apps/web) and adds native
//! notifications, the dock badge, links opened in the browser, and the
//! session token kept in the system keychain; the web app calls them when it
//! runs here.

// Don't open a console window next to the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Keychain entries are named after the app, one per server.
const KEYCHAIN_SERVICE: &str = "Forge";

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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            session_token,
            save_session_token,
            forget_session_token
        ])
        .run(tauri::generate_context!())
        .expect("Forge failed to start");
}
