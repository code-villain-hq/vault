//! A mail's file, opened: the web app reads it from the server and hands
//! it over; it's saved in Downloads, as a browser would, marked as from
//! the internet, and opened with the app the Mac opens it with. A file
//! that would run something is shown in Finder instead.

use std::path::{Path, PathBuf};

use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

/// Saves the file in the request's body under its name (`x-file-name`,
/// percent-encoded) in Downloads, beside any of that name, and opens it,
/// or shows it in Finder when it would run something.
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
    // A browser's download carries the quarantine mark, so Gatekeeper
    // checks it before it runs; a file from a stranger's mail is no safer.
    let marked = quarantine(&path)
        .inspect_err(|e| tracing::warn!("Couldn't mark {} as downloaded: {e}", path.display()))
        .is_ok();
    if !marked || may_run(&path) {
        return app
            .opener()
            .reveal_item_in_dir(&path)
            .map_err(|e| format!("Saved in Downloads, but couldn't show it: {e}"));
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("Saved in Downloads, but couldn't open it: {e}"))
}

/// Marks `path` as downloaded by Postrider now, as Safari marks its own.
fn quarantine(path: &Path) -> std::io::Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    xattr::set(
        path,
        "com.apple.quarantine",
        format!("0083;{now:08x};Postrider;").as_bytes(),
    )
}

/// The kinds of file that run something, or open a link, when opened: one
/// click on them shouldn't be all it takes.
const RUNS: &[&str] = &[
    "action",
    "afploc",
    "app",
    "applescript",
    "bash",
    "command",
    "csh",
    "dylib",
    "fileloc",
    "fish",
    "ftploc",
    "inetloc",
    "jar",
    "kext",
    "ksh",
    "mailloc",
    "mobileconfig",
    "mpkg",
    "osax",
    "pkg",
    "pl",
    "plugin",
    "prefpane",
    "py",
    "rb",
    "saver",
    "scpt",
    "scptd",
    "sh",
    "shortcut",
    "terminal",
    "tool",
    "url",
    "webloc",
    "workflow",
    "xpc",
    "zsh",
];

/// Whether opening `path` would run something rather than show it.
fn may_run(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| RUNS.iter().any(|kind| kind.eq_ignore_ascii_case(extension)))
}

/// A name that stays in its folder and shows what it is: no path, nothing
/// hidden, no characters that turn the rest around, something.
fn safe_name(name: &str) -> String {
    let name: String = name
        .chars()
        .filter(|&c| !is_format(c))
        .map(|c| {
            if c == '/' || c == ':' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let name = name
        .trim()
        .trim_start_matches('.')
        .trim_end_matches(['.', ' ']);
    if name.is_empty() {
        "Attachment".into()
    } else {
        name.to_string()
    }
}

/// Unicode's format characters (Cf), which show as nothing: a right-to-left
/// override among them makes "invoice\u{202E}fdp.terminal" read as a PDF.
fn is_format(c: char) -> bool {
    matches!(
        c,
        '\u{AD}'
            | '\u{600}'..='\u{605}'
            | '\u{61C}'
            | '\u{6DD}'
            | '\u{70F}'
            | '\u{890}'..='\u{891}'
            | '\u{8E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
    )
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
    fn a_name_cant_hide_what_it_is() {
        assert_eq!(
            safe_name("invoice\u{202E}fdp.terminal"),
            "invoicefdp.terminal"
        );
        assert_eq!(
            safe_name("\u{2067}report\u{200F}.pdf\u{2069}\u{FEFF}"),
            "report.pdf"
        );
        assert_eq!(safe_name("run.command. . "), "run.command");
        assert_eq!(safe_name("\u{202E}"), "Attachment");
        assert_eq!(safe_name("Rechnung März.pdf"), "Rechnung März.pdf");
    }

    #[test]
    fn a_file_that_runs_something_isnt_opened() {
        for name in [
            "a.app",
            "b.command",
            "c.terminal",
            "d.tool",
            "e.sh",
            "f.zsh",
            "g.jar",
            "h.pkg",
            "i.mpkg",
            "j.scpt",
            "k.applescript",
            "l.workflow",
            "m.webloc",
            "n.inetloc",
            "o.fileloc",
            "p.url",
            "Q.COMMAND",
            "r.Terminal",
        ] {
            assert!(may_run(Path::new(name)), "{name}");
        }
        for name in [
            "invoice.pdf",
            "photo.jpeg",
            "notes.txt",
            "report.docx",
            "plain",
            "app",
        ] {
            assert!(!may_run(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn a_saved_file_is_marked_as_downloaded() {
        let folder =
            std::env::temp_dir().join(format!("postrider-quarantine-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("Invoice.pdf");
        std::fs::write(&path, b"").unwrap();
        quarantine(&path).unwrap();
        let mark = xattr::get(&path, "com.apple.quarantine").unwrap().unwrap();
        let mark = String::from_utf8(mark).unwrap();
        assert!(mark.starts_with("0083;"), "{mark}");
        assert!(mark.ends_with(";Postrider;"), "{mark}");
        std::fs::remove_dir_all(&folder).unwrap();
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
