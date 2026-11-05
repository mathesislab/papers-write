//! The documents: one per window, as the tabs of macOS (windows of one tabbing identifier merge into tabs, and a
//! tab dragged out is a window). A window shows a file, or is untitled until it is saved. The page asks which
//! (`document`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::files;

/// The windows' tabbing identifier: the windows of the app merge into tabs of one window.
const TABBING: &str = "papers-write";

/// What the windows show, by label: the file, or none for an untitled one.
#[derive(Default)]
pub struct Documents {
    paths: Mutex<HashMap<String, Option<PathBuf>>>,
    /// the number of the next window
    next: AtomicUsize,
}

/// The document of a window, for its page.
#[derive(Serialize)]
pub struct Document {
    /// the file's path; none while the window is untitled
    path: Option<String>,
}

/// The file a window shows, or none when it is untitled.
#[tauri::command]
pub fn document(window: tauri::WebviewWindow) -> Document {
    let paths = window.state::<Documents>().paths.lock().unwrap().get(window.label()).cloned().flatten();
    Document { path: paths.map(|p| p.to_string_lossy().into_owned()) }
}

/// The name of a window: its file's, else "Untitled".
fn title(path: Option<&Path>) -> String {
    path.and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".into())
}

/// A window made: for the file, else untitled.
pub fn make(app: &AppHandle, path: Option<PathBuf>) -> tauri::Result<()> {
    let state = app.state::<Documents>();
    let label = format!("doc-{}", state.next.fetch_add(1, Ordering::Relaxed) + 1);
    state.paths.lock().unwrap().insert(label.clone(), path.clone());
    let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("/".into()))
        .title(title(path.as_deref()))
        .inner_size(900.0, 700.0)
        .tabbing_identifier(TABBING)
        .build();
    if let Err(e) = built {
        state.paths.lock().unwrap().remove(&label);
        return Err(e);
    }
    if let Some(path) = path {
        let _ = files::watch(app, &label, &path);
    }
    Ok(())
}

/// A window shows `path` from now on: its title, its watch, and its page told (`opened`) to read it.
pub fn show(app: &AppHandle, label: &str, path: &Path) {
    app.state::<Documents>().paths.lock().unwrap().insert(label.to_string(), Some(path.to_path_buf()));
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.set_title(&title(Some(path)));
    }
    if let Err(e) = files::watch(app, label, path) {
        eprintln!("{e}");
    }
    let _ = app.emit_to(EventTarget::webview(label), "opened", ());
}

/// A window gone: what was kept of it goes.
pub fn closed(app: &AppHandle, label: &str) {
    app.state::<Documents>().paths.lock().unwrap().remove(label);
    files::unwatch(app, label);
}
