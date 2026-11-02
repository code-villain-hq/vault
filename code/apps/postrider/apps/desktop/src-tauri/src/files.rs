//! A mail's file, opened: the web app reads it from the server and hands
//! it over; it's saved in Downloads, as a browser would, and opened with
//! the app the Mac opens it with.

use std::path::{Path, PathBuf};

use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

/// Saves the file in the request's body under its name (`x-file-name`,
/// percent-encoded) in Downloads, beside any of that name, and opens it.
#[tauri::command]
pub async fn open_file(app: AppHandle, request: Request<'_>) -> Result<(), String> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err("The file didn't come through.".into());
    };
    let name = request
        .headers()
        .get("x-file-name")
        .and_then(|value| value.to_str().ok())
        .map(decode)
        .unwrap_or_default();
    let downloads = app
        .path()
        .download_dir()
        .map_err(|e| format!("Couldn't find Downloads: {e}"))?;
    let path = free_path(&downloads, &safe_name(&name));
    std::fs::write(&path, bytes).map_err(|e| format!("Couldn't save it in Downloads: {e}"))?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("Saved in Downloads, but couldn't open it: {e}"))
}

/// A name that stays in its folder: no path, nothing hidden, something.
fn safe_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == ':' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let name = name.trim().trim_start_matches('.');
    if name.is_empty() {
        "Attachment".into()
    } else {
        name.to_string()
    }
}

/// `name` in `folder`, or "name 2", "name 3"… when it's taken.
fn free_path(folder: &Path, name: &str) -> PathBuf {
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem, format!(".{extension}")),
        _ => (name, String::new()),
    };
    let mut path = folder.join(name);
    let mut n = 2;
    while path.exists() {
        path = folder.join(format!("{stem} {n}{extension}"));
        n += 1;
    }
    path
}

/// Undoes `encodeURIComponent`.
fn decode(encoded: &str) -> String {
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                decoded.push(byte);
                i += 3;
            }
            (byte, _) => {
                decoded.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_comes_back_whole_and_safe() {
        assert_eq!(decode("Rechnung%20M%C3%A4rz.pdf"), "Rechnung März.pdf");
        assert_eq!(safe_name("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(safe_name(".hidden"), "hidden");
        assert_eq!(safe_name(""), "Attachment");
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let folder = std::env::temp_dir().join(format!("postrider-files-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("Invoice.pdf"), b"").unwrap();
        assert_eq!(
            free_path(&folder, "Invoice.pdf"),
            folder.join("Invoice 2.pdf")
        );
        assert_eq!(free_path(&folder, "Notes"), folder.join("Notes"));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
