//! Text files read and written, and watched for the changes made elsewhere (an editor, a sync client).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use notify::EventKind;
use tauri::{AppHandle, Emitter, EventTarget, Manager};

/// The files watched, by the label of the window that shows each: its subscription to the app's one watcher
/// (watch.rs), which watches the file's folder.
#[derive(Default)]
pub struct Watched(Mutex<HashMap<String, crate::watch::Subscription>>);

/// The text of a file: UTF-8, else an error that says so.
#[tauri::command]
pub fn read_text(path: String) -> Result<String, String> {
    let bytes = fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    String::from_utf8(bytes).map_err(|_| format!("{path}: not a UTF-8 text file"))
}

/// The bytes of a PDF, for the page to show with pdf.js where PDFKit is not (Linux).
#[tauri::command]
pub fn read_pdf(path: String) -> Result<tauri::ipc::Response, String> {
    if !Path::new(&path).extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        return Err(format!("{path}: not a PDF"));
    }
    fs::read(&path).map(tauri::ipc::Response::new).map_err(|e| format!("{path}: {e}"))
}

/// `text` written to the file, atomically: a file beside it, renamed. A file that is a link is written through it,
/// and one that was there keeps its permissions.
#[tauri::command]
pub fn write_text(path: String, text: String) -> Result<(), String> {
    write(Path::new(&path), &text).map_err(|e| format!("{path}: {e}"))
}

pub fn write(path: &Path, text: &str) -> std::io::Result<()> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let temporary = target.with_file_name(format!(".{name}.tmp"));
    let renamed = fs::write(&temporary, text)
        .and_then(|()| match fs::metadata(&target) {
            Ok(old) => fs::set_permissions(&temporary, old.permissions()),
            Err(_) => Ok(()),
        })
        .and_then(|()| fs::rename(&temporary, &target));
    if renamed.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    renamed
}

/// The file watched for the window `label`, that of an earlier watch given up: a change made to it goes to the
/// window's page as `file-changed`. The page's own writes come too, which `FileEditor.changedOnDisk` finds to be what
/// it wrote.
pub fn watch(app: &AppHandle, label: &str, path: &Path) -> Result<(), String> {
    watch_for(app, label, label, path, "file-changed")
}

/// `path` watched as `key` (a window's own file is watched as its label; other files of the window as keys of their
/// own), a change sent to the window `label` as the event `told`. A watch of the same key before is given up.
pub fn watch_for(app: &AppHandle, key: &str, label: &str, path: &Path, told: &'static str) -> Result<(), String> {
    let folder = path
        .parent()
        .and_then(|p| p.canonicalize().ok())
        .ok_or_else(|| format!("{}: no folder to watch", path.display()))?;
    // events name the file by its resolved folder
    let file: PathBuf = folder.join(path.file_name().unwrap_or_default());
    let (handle, target) = (app.clone(), label.to_string());
    let subscription = crate::watch::subscribe(&folder, false, move |event: &notify::Event| {
        if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)) && event.paths.contains(&file) {
            let _ = handle.emit_to(EventTarget::webview(target.clone()), told, ());
        }
    })?;
    app.state::<Watched>()
        .0
        .lock()
        .unwrap()
        .insert(key.to_string(), subscription);
    Ok(())
}

/// The watch for the window `label` given up.
pub fn unwatch(app: &AppHandle, label: &str) {
    app.state::<Watched>().0.lock().unwrap().remove(label);
}
