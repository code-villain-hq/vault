//! The accounts' passwords. They live in the macOS keychain, under
//! "Postrider", never in the database or a settings file: a mail account's
//! under its address, the calendar server's under its URL.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Keychain entries are named after the app.
const SERVICE: &str = "Postrider";

#[derive(Clone)]
pub enum Secrets {
    /// The macOS keychain: the desktop app's.
    Keychain,
    /// One password for everything, kept nowhere: the dev server's, for
    /// the stand-in mail and calendar servers (`POSTRIDER_DEV_PASSWORD`).
    Fixed(String),
    /// Held in memory, for tests.
    Memory(Arc<Mutex<HashMap<String, String>>>),
}

impl Secrets {
    pub fn memory() -> Self {
        Secrets::Memory(Arc::default())
    }

    /// The password kept for `key`, if there is one.
    pub async fn get(&self, key: &str) -> Result<Option<String>, String> {
        match self {
            Secrets::Keychain => {
                let key = key.to_string();
                blocking(move || match entry(&key)?.get_password() {
                    Ok(password) => Ok(Some(password)),
                    Err(keyring::Error::NoEntry) => Ok(None),
                    Err(e) => Err(e.to_string()),
                })
                .await
            }
            Secrets::Fixed(password) => Ok(Some(password.clone())),
            Secrets::Memory(map) => Ok(map.lock().expect("never poisoned").get(key).cloned()),
        }
    }

    pub async fn set(&self, key: &str, password: &str) -> Result<(), String> {
        match self {
            Secrets::Keychain => {
                let (key, password) = (key.to_string(), password.to_string());
                blocking(move || {
                    entry(&key)?
                        .set_password(&password)
                        .map_err(|e| e.to_string())
                })
                .await
            }
            Secrets::Fixed(_) => Ok(()),
            Secrets::Memory(map) => {
                map.lock()
                    .expect("never poisoned")
                    .insert(key.into(), password.into());
                Ok(())
            }
        }
    }

    pub async fn forget(&self, key: &str) -> Result<(), String> {
        match self {
            Secrets::Keychain => {
                let key = key.to_string();
                blocking(move || match entry(&key)?.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                    Err(e) => Err(e.to_string()),
                })
                .await
            }
            Secrets::Fixed(_) => Ok(()),
            Secrets::Memory(map) => {
                map.lock().expect("never poisoned").remove(key);
                Ok(())
            }
        }
    }
}

fn entry(key: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, key).map_err(|e| e.to_string())
}

/// The keychain may wait on the person (to allow access), so it's asked
/// off the async threads.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}
