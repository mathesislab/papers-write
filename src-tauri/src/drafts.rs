//! The text of a file typed and not yet saved (Settings ▸ Editor ▸ Save automatically off), kept in the app's cache
//! folder (`unsaved/`) as it is typed, so that it is not lost when the app quits or stops: the file's tab shows it
//! again when it opens, as not saved. A draft is one JSON file per document, named after a hash of its path; it
//! goes when the text is saved, or discarded (Don't Save, Reload).
//!
//! A draft says what the file held when the typing started (`base`, a hash): when the file has changed on disk since,
//! the editor shows the draft and asks which to keep.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize)]
struct Stored {
    /// the file's path (two paths could share a hash)
    path: String,
    text: String,
    /// the hash of the file's text the draft was typed over
    base: String,
}

/// A draft of a file, for its page.
#[derive(Serialize)]
pub struct Draft {
    text: String,
    /// the file holds what the draft was typed over: it has not changed on disk since
    current: bool,
}

/// FNV-1a, 64 bits: a hash that stays the same from one build to the next (the draft's name, its base).
fn hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_cache_dir().map_err(|e| e.to_string())?.join("unsaved"))
}

fn file_of(app: &AppHandle, path: &str) -> Result<PathBuf, String> {
    Ok(dir(app)?.join(format!("{}.json", hash(path.as_bytes()))))
}

fn read(app: &AppHandle, path: &str) -> Option<Stored> {
    let stored: Stored = serde_json::from_slice(&fs::read(file_of(app, path).ok()?).ok()?).ok()?;
    (stored.path == path).then_some(stored)
}

/// The draft of the file at `path`, if there is one.
#[tauri::command]
pub fn draft_read(app: AppHandle, path: String) -> Option<Draft> {
    let stored = read(&app, &path)?;
    let now = fs::read(Path::new(&path)).ok();
    let current = now.is_some_and(|t| hash(&t) == stored.base);
    Some(Draft { text: stored.text, current })
}

/// The draft of the file at `path` kept: `text` as typed, over `base`, the file's text as last read or saved.
#[tauri::command]
pub fn draft_write(app: AppHandle, path: String, text: String, base: String) -> Result<(), String> {
    let file = file_of(&app, &path)?;
    fs::create_dir_all(dir(&app)?).map_err(|e| e.to_string())?;
    let stored = Stored { base: hash(base.as_bytes()), path, text };
    let json = serde_json::to_vec(&stored).map_err(|e| e.to_string())?;
    // written whole, then put in place: never half a draft
    let part = file.with_extension("part");
    fs::write(&part, json).map_err(|e| e.to_string())?;
    fs::rename(&part, &file).map_err(|e| e.to_string())
}

/// The draft of the file at `path` gone (saved, or discarded).
#[tauri::command]
pub fn draft_drop(app: AppHandle, path: String) {
    drop_draft(&app, Path::new(&path));
}

pub fn drop_draft(app: &AppHandle, path: &Path) {
    if let Ok(file) = file_of(app, &path.to_string_lossy()) {
        let _ = fs::remove_file(file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_fnv1a() {
        assert_eq!(hash(b""), "cbf29ce484222325");
        assert_eq!(hash(b"a"), "af63dc4c8601ec8c");
    }
}
