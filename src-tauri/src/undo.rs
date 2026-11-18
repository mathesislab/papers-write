//! The undo history of each document, kept in the app's cache folder (`undo/`) as it is typed, so that a tab whose
//! page is unloaded (tabs.rs: a tab not shown for a while) or the app quit and opened again, can still undo what was
//! done, as Sublime keeps it. One JSON file per document, named after a hash of its path, with a hash of the text the
//! history ends on (`text`): the history is given back only to that text. It goes when the tab is closed.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize)]
pub struct Kept {
    /// the file's path (two paths could share a hash)
    path: String,
    /// the hash of the text the history ends on (the editor's: it alone compares it)
    text: String,
    /// the editor's history (CodeMirror's `historyField`, as JSON)
    history: serde_json::Value,
}

fn hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_cache_dir().map_err(|e| e.to_string())?.join("undo"))
}

fn file_of(app: &AppHandle, path: &str) -> Result<PathBuf, String> {
    Ok(dir(app)?.join(format!("{}.json", hash(path.as_bytes()))))
}

/// The undo history kept for the file at `path`, with the hash of the text it ends on.
#[tauri::command]
pub fn history_read(app: AppHandle, path: String) -> Option<Kept> {
    let kept: Kept = serde_json::from_slice(&fs::read(file_of(&app, &path).ok()?).ok()?).ok()?;
    (kept.path == path).then_some(kept)
}

/// The undo history of the file at `path` kept: `history`, ending on the text whose hash is `text`.
#[tauri::command]
pub fn history_write(app: AppHandle, path: String, text: String, history: serde_json::Value) -> Result<(), String> {
    let file = file_of(&app, &path)?;
    fs::create_dir_all(dir(&app)?).map_err(|e| e.to_string())?;
    let json = serde_json::to_vec(&Kept { path, text, history }).map_err(|e| e.to_string())?;
    let part = file.with_extension("part");
    fs::write(&part, json).map_err(|e| e.to_string())?;
    fs::rename(&part, &file).map_err(|e| e.to_string())
}

/// The undo history of the file at `path` gone (its tab closed).
pub fn drop_history(app: &AppHandle, path: &Path) {
    if let Ok(file) = file_of(app, &path.to_string_lossy()) {
        let _ = fs::remove_file(file);
    }
}
