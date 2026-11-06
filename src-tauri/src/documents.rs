//! The documents: one per window, as the tabs of macOS (windows of one tabbing identifier merge into tabs, and a
//! tab dragged out is a window). A window shows a file, or is untitled until it is saved. The page asks which
//! (`document`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde::Serialize;
use tauri::{
    AppHandle, CloseRequestApi, Emitter, EventTarget, Manager, Webview, WebviewUrl,
    WebviewWindowBuilder, Window,
};
use crate::sheet::{self, Level};
use tauri_plugin_dialog::DialogExt;

use write_core::review::package;

use crate::files;
use crate::package as review_files;

/// The windows' tabbing identifier: the windows of the app merge into tabs of one window.
pub(crate) const TABBING: &str = "papers-write";

/// What the windows show, by label: the file, or none for an untitled one.
#[derive(Default)]
pub struct Documents {
    paths: Mutex<HashMap<String, Option<PathBuf>>>,
    /// the untitled windows with no text in them, which a file opened takes over
    blank: Mutex<HashSet<String>>,
    /// the number of the next window
    next: AtomicUsize,
    /// the app has finished launching: before, files handed over by macOS wait in `pending`
    ready: AtomicBool,
    pending: Mutex<Vec<PathBuf>>,
    /// the window that last had the keys: the one in front (a tab chosen makes its window key)
    focused: Mutex<Option<String>>,
    /// the dialog before quitting is up
    asking_quit: AtomicBool,
}

/// The document of a window, for its page.
#[derive(Serialize)]
pub struct Document {
    /// the file's path; none while the window is untitled
    path: Option<String>,
}

/// The file a window shows, or none when it is untitled.
#[tauri::command]
pub fn document(window: Webview) -> Document {
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

/// The file a window shows, or none when it is untitled.
pub fn path_of(app: &AppHandle, label: &str) -> Option<PathBuf> {
    app.state::<Documents>()
        .paths
        .lock()
        .unwrap()
        .get(label)
        .cloned()
        .flatten()
}

/// The name of a window: its file's (a document to review: "<name> — review for <who sent it>"), else "Untitled".
pub(crate) fn title(path: Option<&Path>) -> String {
    if let Some(sent) = path.and_then(package::of_unpacked) {
        return format!("{} — review for {}", sent.document, sent.from.name);
    }
    path.and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".into())
}

/// The app in the Dock and ⌘Tab while a window is open; in the menu bar only otherwise (macOS).
pub(crate) fn regular(app: &AppHandle, regular: bool) {
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy::{Accessory, Regular};
        let _ = app.set_activation_policy(if regular { Regular } else { Accessory });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, regular);
}

/// Whether the file is a PDF (by its extension): its window is the page of PDFs, not the editor's.
pub fn is_pdf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// A window made: for the file, else untitled. Its label.
pub fn make(app: &AppHandle, path: Option<PathBuf>) -> tauri::Result<String> {
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
    // a new window joins a document's tabs, not the Settings window
    let tabs = crate::settings::get().tabs;
    let joined = front(app).filter(|w| tabs && w.label().starts_with("doc-"));
    let built = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App(
            if path.as_deref().is_some_and(is_pdf) {
                "/pdf"
            } else {
                "/"
            }
            .into(),
        ),
    )
    .title(title(path.as_deref()))
    .inner_size(900.0, 700.0)
    // off: an identifier of its own, which the system's preference for tabs does not merge with others
    .tabbing_identifier(if tabs { TABBING } else { &label })
    .build();
    let window = match built {
        Ok(window) => window,
        Err(e) => {
            closed(app, &label);
            return Err(e);
        }
    };
    match joined {
        Some(to) => crate::tabs::join(&window.as_ref().window(), &to.window()),
        None => {
            let _ = window.set_focus();
        }
    }
    if let Some(path) = path {
        let _ = files::watch(app, &label, &path);
    }
    Ok(label)
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
    let mut focused = state.focused.lock().unwrap();
    if focused.as_deref() == Some(label) {
        *focused = None;
    }
    drop(focused);
    files::unwatch(app, label);
    crate::refs::unload(app, label);
    crate::review::unload(app, label);
    crate::git::closed(app, label);
    crate::sidebar::closed(app, label);
    // the window may still be among the windows as this is told
    if app.webview_windows().keys().all(|l| l == label) {
        regular(app, false);
    }
}

/// Whether the untitled window has no text in it, as its page says each time that changes.
#[tauri::command]
pub fn set_blank(window: Webview, blank: bool) {
    let state = window.state::<Documents>();
    let mut set = state.blank.lock().unwrap();
    if blank {
        set.insert(window.label().to_string());
    } else {
        set.remove(window.label());
    }
}

/// A window that got the keys: the one in front from now on.
pub fn focused(app: &AppHandle, label: &str) {
    *app.state::<Documents>().focused.lock().unwrap() = Some(label.to_string());
    crate::review::focused(app, label);
}

/// The window in front: AppKit's key window (on the main thread, where the menu's events come); else the last that
/// got the keys (tabs say wrongly whether they have them, and a window made from another thread may never say).
pub fn front(app: &AppHandle) -> Option<Webview> {
    if let Some(page) = crate::tabs::key_window(app).and_then(|label| app.get_webview(&label)) {
        return Some(page);
    }
    let last = app.state::<Documents>().focused.lock().unwrap().clone();
    last.and_then(|label| app.get_webview(&label)).or_else(|| {
        app.webviews()
            .into_values()
            .find(|w| w.window().is_focused().unwrap_or(false))
    })
}

/// The files the windows show.
pub fn open_paths(app: &AppHandle) -> Vec<PathBuf> {
    app.state::<Documents>().paths.lock().unwrap().values().flatten().cloned().collect()
}

/// The window that shows `path`, if one does.
pub fn label_of(app: &AppHandle, path: &Path) -> Option<String> {
    let path = path.canonicalize().ok()?;
    app.state::<Documents>().paths.lock().unwrap().iter().find(|(_, p)| p.as_deref() == Some(path.as_path())).map(|(l, _)| l.clone())
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
    // a review file is a package: its document to review, or a review sent back
    if review_files::is_review(&path) {
        review_files::open(app, path);
        return Ok(());
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
    // a PDF has a page of its own, so no window of text shows it
    let blank = if is_pdf(&path) {
        HashSet::new()
    } else {
        state.blank.lock().unwrap().clone()
    };
    let reuse = front(app)
        .filter(|w| blank.contains(w.label()))
        .or_else(|| blank.iter().find_map(|label| app.get_webview(label)));
    match reuse {
        Some(window) => {
            show(app, window.label(), &path);
            let _ = window.set_focus();
            Ok(())
        }
        None => make(app, Some(path)).map(|_| ()).map_err(|e| e.to_string()),
    }
}

/// Files opened, one after the other; what could not be is told in a dialog.
/// Files handed over by macOS (Open With, the Dock, a double click): opened, or kept until the app is ready, as
/// they may come while it is still launching, before any window can be made.
pub fn opened(app: &AppHandle, paths: Vec<PathBuf>) {
    let state = app.state::<Documents>();
    if !state.ready.load(Ordering::SeqCst) {
        state.pending.lock().unwrap().extend(paths);
        return;
    }
    open_files(app, &paths);
}

/// The app launched: the files handed over until now, or else a window of untitled text.
pub fn ready(app: &AppHandle) {
    let state = app.state::<Documents>();
    state.ready.store(true, Ordering::SeqCst);
    let pending = std::mem::take(&mut *state.pending.lock().unwrap());
    if !pending.is_empty() {
        open_files(app, &pending);
    } else if app.webview_windows().is_empty()
        && let Err(e) = make(app, None)
    {
        eprintln!("{e}");
    }
}

pub fn open_files(app: &AppHandle, paths: &[PathBuf]) {
    for path in paths {
        if let Err(e) = open_file(app, path) {
            sheet::tell(app, None, "Open", e, Level::Error);
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
    window: Webview,
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

/// How many windows have untitled text that was never saved: those without a file that are not blank.
fn unsaved_count(paths: &HashMap<String, Option<PathBuf>>, blank: &HashSet<String>) -> usize {
    paths.iter().filter(|(label, path)| path.is_none() && !blank.contains(*label)).count()
}

/// What the dialog says before quitting with `n` windows of text never saved.
fn quit_message(n: usize) -> String {
    match n {
        1 => "1 window has text that was never saved.".into(),
        n => format!("{n} windows have text that was never saved."),
    }
}

/// The app quits (⌘Q, the menu bar icon, the Dock icon). With text in an untitled window that was never saved it
/// asks once first: Quit and discard, or Cancel.
pub fn quit(app: &AppHandle) {
    if !confirm_quit(app) {
        app.exit(0);
    }
}

/// Asks before quitting when untitled text would be lost; true when the dialog is up (it quits on "Quit and
/// discard"), false when there is nothing to ask. One dialog at a time.
pub fn confirm_quit(app: &AppHandle) -> bool {
    let state = app.state::<Documents>();
    let n = unsaved_count(&state.paths.lock().unwrap(), &state.blank.lock().unwrap());
    if n == 0 {
        return false;
    }
    if state.asking_quit.swap(true, Ordering::SeqCst) {
        return true;
    }
    let handle = app.clone();
    // a sheet on the window in front
    sheet::ask_then(app, None, "Quit", format!("{} Quit and discard it?", quit_message(n)), Level::Warning, &["Quit and discard", "Cancel"], move |button| {
        handle.state::<Documents>().asking_quit.store(false, Ordering::SeqCst);
        if button.as_deref() == Some("Quit and discard") {
            handle.exit(0);
        }
    });
    true
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
    let app = window.app_handle().clone();
    let webview = app.get_webview(window.label());
    sheet::ask_then(
        &app,
        webview.as_ref(),
        "Close",
        "This window has text that was never saved. Close it and discard the text?",
        Level::Warning,
        &["Discard", "Cancel"],
        move |button| {
            if button.as_deref() == Some("Discard") {
                let _ = window.destroy();
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_untitled_windows_with_text_count_for_quitting() {
        let paths: HashMap<String, Option<PathBuf>> = [
            ("doc-1".to_string(), None),
            ("doc-2".to_string(), None),
            ("doc-3".to_string(), None),
            ("doc-4".to_string(), Some(PathBuf::from("/tmp/paper.md"))),
        ]
        .into();
        let blank: HashSet<String> = ["doc-1".to_string()].into();
        assert_eq!(unsaved_count(&paths, &blank), 2);
        assert_eq!(unsaved_count(&paths, &["doc-1", "doc-2", "doc-3"].map(String::from).into()), 0);
        assert_eq!(quit_message(1), "1 window has text that was never saved.");
        assert_eq!(quit_message(2), "2 windows have text that was never saved.");
    }
}
