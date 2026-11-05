//! Text files read and written, and watched for the changes made elsewhere (an editor, a sync client).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, EventTarget, Manager};

/// The files watched, by the label of the window that shows each: the watcher of the file's folder.
#[derive(Default)]
pub struct Watched(Mutex<HashMap<String, RecommendedWatcher>>);

/// The text of a file: UTF-8, else an error that says so.
#[tauri::command]
pub fn read_text(path: String) -> Result<String, String> {
    let bytes = fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    String::from_utf8(bytes).map_err(|_| format!("{path}: not a UTF-8 text file"))
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
    let folder = path
        .parent()
        .and_then(|p| p.canonicalize().ok())
        .ok_or_else(|| format!("{}: no folder to watch", path.display()))?;
    // events name the file by its resolved folder
    let file: PathBuf = folder.join(path.file_name().unwrap_or_default());
    let (handle, target) = (app.clone(), label.to_string());
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event
            && matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
            )
            && event.paths.contains(&file)
        {
            let _ = handle.emit_to(EventTarget::webview(target.clone()), "file-changed", ());
        }
    })
    .map_err(|e| format!("{}: not watched ({e})", path.display()))?;
    watcher
        .watch(&folder, RecursiveMode::NonRecursive)
        .map_err(|e| format!("{}: not watched ({e})", path.display()))?;
    app.state::<Watched>()
        .0
        .lock()
        .unwrap()
        .insert(label.to_string(), watcher);
    Ok(())
}

/// The watch for the window `label` given up.
pub fn unwatch(app: &AppHandle, label: &str) {
    app.state::<Watched>().0.lock().unwrap().remove(label);
}
