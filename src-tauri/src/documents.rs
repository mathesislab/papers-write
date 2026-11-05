//! The documents: one per window, as the tabs of macOS (windows of one tabbing identifier merge into tabs, and a
//! tab dragged out is a window). A window shows a file, or is untitled until it is saved. The page asks which
//! (`document`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Serialize;
use tauri::{
    AppHandle, CloseRequestApi, Emitter, EventTarget, Manager, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, Window,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::files;

/// The windows' tabbing identifier: the windows of the app merge into tabs of one window.
const TABBING: &str = "papers-write";

/// What the windows show, by label: the file, or none for an untitled one.
#[derive(Default)]
pub struct Documents {
    paths: Mutex<HashMap<String, Option<PathBuf>>>,
    /// the untitled windows with no text in them, which a file opened takes over
    blank: Mutex<HashSet<String>>,
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
pub fn document(window: WebviewWindow) -> Document {
    let paths = window
        .state::<Documents>()
        .paths
        .lock()
        .unwrap()
        .get(window.label())
        .cloned()
        .flatten();
    Document {
        path: paths.map(|p| p.to_string_lossy().into_owned()),
    }
}

/// The name of a window: its file's, else "Untitled".
fn title(path: Option<&Path>) -> String {
    path.and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".into())
}

/// The app in the Dock and ⌘Tab while a window is open; in the menu bar only otherwise (macOS).
fn regular(app: &AppHandle, regular: bool) {
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy::{Accessory, Regular};
        let _ = app.set_activation_policy(if regular { Regular } else { Accessory });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, regular);
}

/// A window made: for the file, else untitled.
pub fn make(app: &AppHandle, path: Option<PathBuf>) -> tauri::Result<()> {
    let state = app.state::<Documents>();
    let label = format!("doc-{}", state.next.fetch_add(1, Ordering::Relaxed) + 1);
    state
        .paths
        .lock()
        .unwrap()
        .insert(label.clone(), path.clone());
    if path.is_none() {
        state.blank.lock().unwrap().insert(label.clone());
    }
    regular(app, true);
    let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("/".into()))
        .title(title(path.as_deref()))
        .inner_size(900.0, 700.0)
        .tabbing_identifier(TABBING)
        .build();
    let window = match built {
        Ok(window) => window,
        Err(e) => {
            closed(app, &label);
            return Err(e);
        }
    };
    let _ = window.set_focus();
    if let Some(path) = path {
        let _ = files::watch(app, &label, &path);
    }
    Ok(())
}

/// A window shows `path` from now on: its title, its watch, and its page told (`opened`) to read it.
pub fn show(app: &AppHandle, label: &str, path: &Path) {
    let state = app.state::<Documents>();
    state
        .paths
        .lock()
        .unwrap()
        .insert(label.to_string(), Some(path.to_path_buf()));
    state.blank.lock().unwrap().remove(label);
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
    let state = app.state::<Documents>();
    state.paths.lock().unwrap().remove(label);
    state.blank.lock().unwrap().remove(label);
    files::unwatch(app, label);
    // the window may still be among the windows as this is told
    if app.webview_windows().keys().all(|l| l == label) {
        regular(app, false);
    }
}

/// Whether the untitled window has no text in it, as its page says each time that changes.
#[tauri::command]
pub fn set_blank(window: WebviewWindow, blank: bool) {
    let state = window.state::<Documents>();
    let mut set = state.blank.lock().unwrap();
    if blank {
        set.insert(window.label().to_string());
    } else {
        set.remove(window.label());
    }
}

/// The window in front.
pub fn front(app: &AppHandle) -> Option<WebviewWindow> {
    app.webview_windows()
        .into_values()
        .find(|w| w.is_focused().unwrap_or(false))
}

/// A file opened: its window brought forward when it has one; else the untitled blank window in front (or any) shows
/// it; else a window of its own.
pub fn open_file(app: &AppHandle, path: &Path) -> Result<(), String> {
    let path = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !path.is_file() {
        return Err(format!("{}: not a file", path.display()));
    }
    let state = app.state::<Documents>();
    let open = state
        .paths
        .lock()
        .unwrap()
        .iter()
        .find(|(_, p)| p.as_deref() == Some(path.as_path()))
        .map(|(l, _)| l.clone());
    if let Some(window) = open.and_then(|label| app.get_webview_window(&label)) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let blank = state.blank.lock().unwrap().clone();
    let reuse = front(app)
        .filter(|w| blank.contains(w.label()))
        .or_else(|| blank.iter().find_map(|label| app.get_webview_window(label)));
    match reuse {
        Some(window) => {
            show(app, window.label(), &path);
            let _ = window.set_focus();
            Ok(())
        }
        None => make(app, Some(path)).map_err(|e| e.to_string()),
    }
}

/// Files opened, one after the other; what could not be is told in a dialog.
pub fn open_files(app: &AppHandle, paths: &[PathBuf]) {
    for path in paths {
        if let Err(e) = open_file(app, path) {
            app.dialog()
                .message(e)
                .title("Open")
                .kind(MessageDialogKind::Error)
                .show(|_| {});
        }
    }
}

/// Open…: the files to choose.
pub fn choose(app: &AppHandle) {
    // from the menu bar, with no window: the dialog is in front only for an app with a Dock icon
    regular(app, true);
    let handle = app.clone();
    app.dialog()
        .file()
        .set_title("Open")
        .pick_files(move |files| {
            let paths: Vec<PathBuf> = files
                .into_iter()
                .flatten()
                .filter_map(|f| f.into_path().ok())
                .collect();
            open_files(&handle, &paths);
        });
}

/// An untitled window's text written where the user says: the window then shows that file. The path, or none when
/// the dialog was cancelled.
#[tauri::command]
pub async fn save_untitled(
    app: AppHandle,
    window: WebviewWindow,
    text: String,
) -> Result<Option<String>, String> {
    let Some(file) = app
        .dialog()
        .file()
        .set_title("Save")
        .set_file_name("Untitled.md")
        .blocking_save_file()
        .and_then(|f| f.into_path().ok())
    else {
        return Ok(None);
    };
    let state = app.state::<Documents>();
    let taken = state.paths.lock().unwrap().values().flatten().any(|p| {
        p.canonicalize()
            .is_ok_and(|p| Some(p) == file.canonicalize().ok())
    });
    if taken {
        return Err(format!("{}: open in another window", file.display()));
    }
    files::write(&file, &text).map_err(|e| format!("{}: {e}", file.display()))?;
    let file = file.canonicalize().unwrap_or(file);
    show(&app, window.label(), &file);
    Ok(Some(file.to_string_lossy().into_owned()))
}

/// A window asked to close: an untitled one with text in it asks first whether to discard it.
pub fn closing(window: &Window, api: &CloseRequestApi) {
    let state = window.state::<Documents>();
    let untitled = state
        .paths
        .lock()
        .unwrap()
        .get(window.label())
        .is_some_and(|p| p.is_none());
    if !untitled || state.blank.lock().unwrap().contains(window.label()) {
        return;
    }
    api.prevent_close();
    let window = window.clone();
    window
        .dialog()
        .message("This window has text that was never saved. Close it and discard the text?")
        .title("Close")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Discard".into(),
            "Cancel".into(),
        ))
        .show(move |discard| {
            if discard {
                let _ = window.destroy();
            }
        });
}
