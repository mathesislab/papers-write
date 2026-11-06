//! The sidebar of a document window: the files of its project (`write_core::files`), and the project searched and
//! text replaced in it (`write_core::search`). Positions are UTF-16 code units, as CodeMirror counts.
//!
//! A match opened goes to its window as `go-to`. A window that is to load the document first (made for it, or a blank
//! one taken over) is not listening yet: the match waits here until its page asks (`pending_go_to`), as reviews
//! received wait for the log (`review_received`).

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, Manager, Webview};
use write_core::files::{self, Node};
use write_core::search::{self, FileHits, Query};

use crate::documents;

/// The most matches a search finds.
const MOST: usize = 2000;

/// The matches to go to, by the label of the window that has yet to load its document.
#[derive(Default)]
pub struct GoTos(Mutex<HashMap<String, GoTo>>);

/// A match to select in the editor: `go-to`, `pending_go_to`.
#[derive(Clone, Copy, Serialize)]
pub struct GoTo {
    at: usize,
    end: usize,
}

/// Replace all in a window's text: `replace-all`.
#[derive(Clone, Serialize)]
struct ReplaceAll {
    query: Query,
    with: String,
}

/// The files of the project at `root`, as a tree; `write.toml` left out.
#[tauri::command]
pub async fn project_files(root: String) -> Result<Vec<Node>, String> {
    tauri::async_runtime::spawn_blocking(move || files::tree(Path::new(&root), &[write_core::project::MANIFEST]))
        .await
        .map_err(|e| e.to_string())
}

/// The matches of a query in the files of the project at `root`.
#[tauri::command]
pub async fn search_project(root: String, query: Query) -> Result<Vec<FileHits>, String> {
    tauri::async_runtime::spawn_blocking(move || search::search(Path::new(&root), &query, MOST))
        .await
        .map_err(|e| e.to_string())?
}

/// The matches of a query replaced in the files `paths`; how many. A file a window shows is replaced in the window's
/// text (`replace-all`), where it can be undone and is saved as the window saves; its matches are counted on disk.
/// The others are written. The files that could not be are told in the error, with how many were replaced in the rest.
#[tauri::command]
pub async fn replace_in_project(app: AppHandle, query: Query, with: String, paths: Vec<String>) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (mut count, mut failed) = (0, Vec::new());
        for path in &paths {
            match replace_in(&app, Path::new(path), &query, &with) {
                Ok(n) => count += n,
                Err(e) => failed.push(format!("{path}: {e}")),
            }
        }
        if failed.is_empty() {
            Ok(count)
        } else {
            Err(format!("{count} replaced; not in:\n{}", failed.join("\n")))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

fn replace_in(app: &AppHandle, path: &Path, query: &Query, with: &str) -> Result<usize, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let (replaced, count) = search::replace(&text, query, with)?;
    match documents::label_of(app, path) {
        Some(label) => {
            let all = ReplaceAll { query: query.clone(), with: with.to_string() };
            app.emit_to(EventTarget::webview(label), "replace-all", all).map_err(|e| e.to_string())?;
        }
        None if count > 0 => crate::files::write(path, &replaced).map_err(|e| e.to_string())?,
        None => {}
    }
    Ok(count)
}

/// A file opened (as File ▸ Open does) with the match at `at..end` selected. A PDF is opened only.
#[tauri::command]
pub fn open_at(app: AppHandle, path: String, at: usize, end: usize) -> Result<(), String> {
    let path = Path::new(&path);
    let shown = documents::label_of(&app, path);
    documents::open_file(&app, path)?;
    if documents::is_pdf(path) {
        return Ok(());
    }
    let go = GoTo { at, end };
    match (shown, documents::label_of(&app, path)) {
        (Some(label), _) => app.emit_to(EventTarget::webview(label), "go-to", go).map_err(|e| e.to_string()),
        (None, Some(label)) => {
            app.state::<GoTos>().0.lock().unwrap().insert(label, go);
            Ok(())
        }
        (None, None) => Ok(()),
    }
}

/// The match to select once the window's document has loaded, if one waits.
#[tauri::command]
pub fn pending_go_to(window: Webview) -> Option<GoTo> {
    window.state::<GoTos>().0.lock().unwrap().remove(window.label())
}

/// A window gone: its match, if one was waiting, goes.
pub fn closed(app: &AppHandle, label: &str) {
    app.state::<GoTos>().0.lock().unwrap().remove(label);
}

/// A new, empty file `name` in `folder`, opened; its path. One already there is an error.
#[tauri::command]
pub fn new_markdown_file(app: AppHandle, folder: String, name: String) -> Result<String, String> {
    let path: PathBuf = Path::new(&folder).join(&name);
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    documents::open_files(&app, std::slice::from_ref(&path));
    Ok(path.to_string_lossy().into_owned())
}

/// A file or folder moved to the Trash. A window that shows a file of it keeps its text: the file is gone from disk.
#[tauri::command]
pub fn trash_path(path: String) -> Result<(), String> {
    crate::trash::trash(Path::new(&path))
}
