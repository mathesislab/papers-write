//! The windows and their tabs. A window (`win-N`) holds pages, one per tab (a document's `doc-N`, a history's
//! `history-N`), as children filling it. A window shows one project at a time: its pages are grouped in sets, one
//! per project, the one shown first; the tab in front of it is shown, every other page hidden but alive (text, undo,
//! scroll and find stay). A tile of the project bar switches the window to that project (its tabs as left, or as
//! saved; else its main document), or brings forward the window that shows it; Open in New Window gives it a window
//! of its own. A file opens in its project's set, the window switched to it; a file outside any project in the set
//! shown in front. With Settings ▸ Open files as tabs off, each file has a window of its own.
//!
//! What a project's tabs are (its files, the one in front) is saved as they change, and so are the windows and their
//! projects (`state.rs`), except while the app quits: the windows open then open again at the next start, each
//! project with its tabs. A window closed by hand is not; its projects keep their tabs for the next time.
//!
//! The pages draw the tabs of their set themselves (`tab_strip`), told `tabs-changed` when a tab comes, goes or is
//! renamed, and `tab-shown` (true or false) when theirs comes to the front or leaves it. Which window is in front is
//! AppKit's to say (its key window), else the last focused.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, Webview, WebviewBuilder, WebviewUrl,
    Window, WindowBuilder,
};

use crate::state::{Layout, SavedWindow};

/// The windows' tabs, by window label.
#[derive(Default)]
pub struct Tabs {
    windows: Mutex<HashMap<String, Win>>,
    /// the windows in the order they were made, for saving them so
    order: Mutex<Vec<String>>,
    /// the number of the next window
    next: AtomicUsize,
    /// the window that last had the keys
    focused: Mutex<Option<String>>,
    /// the app is quitting: its windows close, but stay saved as open
    quitting: AtomicBool,
    /// the windows are being opened again at the start: nothing is saved meanwhile
    restoring: AtomicBool,
}

/// A window's sets of tabs, the one shown first.
#[derive(Default)]
struct Win {
    sets: Vec<Set>,
}

/// The tabs of one project in a window (none: files outside any project): their pages in order, the one in front.
#[derive(Default)]
struct Set {
    project: Option<PathBuf>,
    pages: Vec<String>,
    front: Option<String>,
    /// the names of pages that are not documents (a history)
    titles: HashMap<String, String>,
}

/// A tab of the strip.
#[derive(Serialize)]
pub struct Tab {
    label: String,
    /// the file's name, Untitled, or the page's own (a history)
    title: String,
    /// the file's path, for the tooltip
    path: Option<String>,
}

impl Win {
    fn set_of(&self, page: &str) -> Option<usize> {
        self.sets.iter().position(|s| s.pages.iter().any(|p| p == page))
    }

    fn set_for(&self, project: Option<&Path>) -> Option<usize> {
        self.sets.iter().position(|s| s.project.as_deref() == project)
    }

    /// The set `i` shown first.
    fn raise(&mut self, i: usize) {
        let set = self.sets.remove(i);
        self.sets.insert(0, set);
    }

    fn pages(&self) -> impl Iterator<Item = &String> {
        self.sets.iter().flat_map(|s| s.pages.iter())
    }
}

fn with_tabs<T>(app: &AppHandle, f: impl FnOnce(&mut HashMap<String, Win>) -> T) -> T {
    let state = app.state::<Tabs>();
    let mut windows = state.windows.lock().unwrap();
    f(&mut windows)
}

/// The window a page is in.
pub fn window_of(app: &AppHandle, page: &str) -> Option<String> {
    with_tabs(app, |w| w.iter().find(|(_, win)| win.set_of(page).is_some()).map(|(l, _)| l.clone()))
}

/// The window that holds a project's tabs, if one does.
pub fn window_of_project(app: &AppHandle, root: &Path) -> Option<String> {
    with_tabs(app, |w| w.iter().find(|(_, win)| win.set_for(Some(root)).is_some()).map(|(l, _)| l.clone()))
}

/// The page in front of a window: the tab in front of the set it shows.
fn front_of(app: &AppHandle, window: &str) -> Option<String> {
    with_tabs(app, |w| w.get(window)?.sets.first()?.front.clone())
}

/// A new window, empty.
fn make_window(app: &AppHandle) -> tauri::Result<Window> {
    let state = app.state::<Tabs>();
    let label = format!("win-{}", state.next.fetch_add(1, Ordering::Relaxed) + 1);
    // a little below and right of the window in front, as macOS cascades windows
    let at = front_window(app).and_then(|w| app.get_window(&w)).and_then(|w| {
        let scale = w.scale_factor().ok()?;
        let p = w.outer_position().ok()?.to_logical::<f64>(scale);
        Some((p.x + 24.0, p.y + 24.0))
    });
    let mut builder = WindowBuilder::new(app, &label)
        .title("Papers Write")
        .inner_size(1000.0, 720.0)
        .min_inner_size(520.0, 360.0)
        // never merged into AppKit's tabs: the pages draw their own
        .tabbing_identifier(&label);
    builder = match at {
        Some((x, y)) => builder.position(x, y),
        None => builder.center(),
    };
    let window = builder.build()?;
    state.windows.lock().unwrap().insert(label.clone(), Win::default());
    state.order.lock().unwrap().push(label);
    Ok(window)
}

/// The window a page of `project` goes in, and that the window shows that project's set (made when new): the
/// window that holds the project; else the window in front, switched to it; else a new window. With files in
/// windows of their own, always a new window. A file outside any project goes in the set the window in front shows.
fn place(app: &AppHandle, project: Option<&Path>) -> tauri::Result<Window> {
    let own = !crate::settings::get().tabs;
    let held = if own { None } else { project.and_then(|p| window_of_project(app, p)) };
    let label = match held {
        Some(label) => label,
        None => match (own, front_window(app)) {
            (false, Some(front)) => front,
            _ => make_window(app)?.label().to_string(),
        },
    };
    with_tabs(app, |w| {
        let win = w.entry(label.clone()).or_default();
        // a loose file joins whatever the window shows
        let i = match project {
            None if !win.sets.is_empty() => 0,
            _ => match win.set_for(project) {
                Some(i) => i,
                None => {
                    win.sets.insert(0, Set { project: project.map(Path::to_path_buf), ..Set::default() });
                    0
                }
            },
        };
        win.raise(i);
    });
    app.get_window(&label).ok_or(tauri::Error::WindowNotFound)
}

/// A page made at `url` with `label`, as a tab of `path`'s project (`place`), after the tab in front, and brought to
/// the front.
pub fn add(app: &AppHandle, label: &str, url: &str, path: Option<&Path>) -> tauri::Result<Webview> {
    let project = path.and_then(write_core::project::find);
    let window = place(app, project.as_deref())?;
    add_to(app, &window, label, url, None)
}

/// A page that is not a document (a history), named `title`, as a tab beside the page `beside`.
pub fn add_beside(app: &AppHandle, label: &str, url: &str, beside: &str, title: String) -> tauri::Result<Webview> {
    let window = match window_of(app, beside).and_then(|w| app.get_window(&w)) {
        Some(window) => {
            with_tabs(app, |w| {
                if let Some(win) = w.get_mut(window.label())
                    && let Some(i) = win.set_of(beside)
                {
                    win.raise(i);
                }
            });
            window
        }
        None => place(app, None)?,
    };
    add_to(app, &window, label, url, Some(title))
}

/// The page made as a tab of the set `window` shows first.
fn add_to(app: &AppHandle, window: &Window, label: &str, url: &str, title: Option<String>) -> tauri::Result<Webview> {
    let size = window.inner_size()?.to_logical::<f64>(window.scale_factor()?);
    let page = window.add_child(
        WebviewBuilder::new(label, WebviewUrl::App(url.into())).auto_resize(),
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(size.width, size.height),
    )?;
    with_tabs(app, |w| {
        let win = w.entry(window.label().to_string()).or_default();
        if win.sets.is_empty() {
            win.sets.push(Set::default());
        }
        let set = &mut win.sets[0];
        let at = set.front.as_ref().and_then(|f| set.pages.iter().position(|p| p == f)).map_or(set.pages.len(), |i| i + 1);
        set.pages.insert(at, label.to_string());
        if let Some(title) = title {
            set.titles.insert(label.to_string(), title);
        }
    });
    select(app, label);
    Ok(page)
}

/// A tab brought to the front: its set shown by its window, its page shown and every other page of the window
/// hidden, the window titled after it and brought forward.
pub fn select(app: &AppHandle, page: &str) {
    let Some(window_label) = window_of(app, page) else {
        return;
    };
    let pages: Vec<String> = with_tabs(app, |w| {
        let Some(win) = w.get_mut(&window_label) else {
            return Vec::new();
        };
        if let Some(i) = win.set_of(page) {
            win.raise(i);
        }
        win.sets[0].front = Some(page.to_string());
        win.pages().cloned().collect()
    });
    for other in &pages {
        if let Some(webview) = app.get_webview(other) {
            if other == page {
                let _ = webview.show();
            } else {
                let _ = webview.hide();
            }
            let _ = app.emit_to(EventTarget::webview(other), "tab-shown", other == page);
        }
    }
    if let Some(window) = app.get_window(&window_label) {
        let _ = window.set_title(&title_of(app, page));
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    if let Some(webview) = app.get_webview(page) {
        let _ = webview.set_focus();
    }
    *app.state::<Tabs>().focused.lock().unwrap() = Some(window_label.clone());
    crate::documents::focused(app, page);
    changed(app, &window_label);
}

/// A tab's name: its file's, or the page's own.
fn title_of(app: &AppHandle, page: &str) -> String {
    let own = with_tabs(app, |w| w.values().flat_map(|win| win.sets.iter()).find_map(|s| s.titles.get(page).cloned()));
    own.unwrap_or_else(|| crate::documents::title(crate::documents::path_of(app, page).as_deref()))
}

/// A page shows another file (an untitled one saved, a blank one taking a file): its window's title and tabs. A set
/// of loose files with this tab alone becomes the set of the file's project, unless a window holds that one already.
pub fn renamed(app: &AppHandle, page: &str) {
    let Some(window_label) = window_of(app, page) else {
        return;
    };
    let project = crate::documents::path_of(app, page).and_then(|p| write_core::project::find(&p));
    if let Some(project) = project {
        with_tabs(app, |w| {
            let taken = w.values().any(|win| win.set_for(Some(&project)).is_some());
            if let Some(win) = w.get_mut(&window_label)
                && let Some(i) = win.set_of(page)
                && win.sets[i].project.is_none()
                && win.sets[i].pages.len() == 1
                && !taken
            {
                win.sets[i].project = Some(project);
            }
        });
    }
    if front_of(app, &window_label).as_deref() == Some(page)
        && let Some(window) = app.get_window(&window_label)
    {
        let _ = window.set_title(&title_of(app, page));
    }
    changed(app, &window_label);
}

/// The pages of a window told to ask for their tabs again; what is kept of the tabs saved.
fn changed(app: &AppHandle, window: &str) {
    for page in pages_of(app, window) {
        let _ = app.emit_to(EventTarget::webview(&page), "tabs-changed", ());
    }
    save(app);
}

/// A tab closed (⌘W, its close button): a page with untitled text asks first; then it goes.
pub fn close(app: &AppHandle, page: &str) {
    if !crate::documents::asks_before_closing(app, page) {
        remove(app, page);
    }
}

/// A page and its tab gone (asked already): what was kept of it goes; the neighbour comes to the front; a set
/// emptied goes, the window showing the project shown before; the window closes with its last tab.
pub fn remove(app: &AppHandle, page: &str) {
    let Some(window_label) = window_of(app, page) else {
        return;
    };
    let (next, empty) = with_tabs(app, |w| {
        let Some(win) = w.get_mut(&window_label) else {
            return (None, false);
        };
        let Some(s) = win.set_of(page) else {
            return (None, false);
        };
        let set = &mut win.sets[s];
        let i = set.pages.iter().position(|p| p == page).unwrap_or(0);
        let was_front = set.front.as_deref() == Some(page);
        set.pages.retain(|p| p != page);
        set.titles.remove(page);
        if was_front {
            // the tab after it, else the one before
            set.front = set.pages.get(i).or_else(|| set.pages.last()).cloned();
        }
        let emptied = set.pages.is_empty();
        if emptied {
            win.sets.remove(s);
        }
        // the page shown went (its tab, or its set and so the project shown): the window shows what is first now
        let next = if s == 0 && (was_front || emptied) { win.sets.first().and_then(|s| s.front.clone()) } else { None };
        (next, win.sets.is_empty())
    });
    forget(app, page);
    if let Some(webview) = app.get_webview(page) {
        let _ = webview.close();
    }
    if empty {
        // closed by hand, tab by tab: it is not opened again at the next start
        let state = app.state::<Tabs>();
        state.windows.lock().unwrap().remove(&window_label);
        state.order.lock().unwrap().retain(|w| *w != window_label);
        if let Some(window) = app.get_window(&window_label) {
            let _ = window.destroy();
        }
        save(app);
        if !any(app) {
            crate::documents::regular(app, false);
        }
    } else if let Some(next) = next {
        select(app, &next);
    } else {
        changed(app, &window_label);
    }
}

/// What the app kept of a page: its document, its history, its spelling.
fn forget(app: &AppHandle, page: &str) {
    crate::documents::closed(app, page);
    crate::git::closed(app, page);
    crate::spelling::closed(app, page);
}

/// A window gone (its close button, after asking about untitled text; the app quitting): its pages and what was
/// kept of them. Closed by hand, it is not opened again at the next start (its projects keep their tabs).
pub fn window_closed(app: &AppHandle, window: &str) {
    let state = app.state::<Tabs>();
    let win = state.windows.lock().unwrap().remove(window);
    let Some(win) = win else {
        return;
    };
    for page in win.pages() {
        forget(app, page);
    }
    if !state.quitting.load(Ordering::SeqCst) {
        state.order.lock().unwrap().retain(|w| w != window);
        save_windows(app);
    }
    let mut focused = state.focused.lock().unwrap();
    if focused.as_deref() == Some(window) {
        *focused = None;
    }
    drop(focused);
    if !any(app) {
        crate::documents::regular(app, false);
    }
}

/// The app quits: its windows close, but stay saved as open.
pub fn quitting(app: &AppHandle) {
    app.state::<Tabs>().quitting.store(true, Ordering::SeqCst);
}

/// The pages of a window, in order, every set's.
pub fn pages_of(app: &AppHandle, window: &str) -> Vec<String> {
    with_tabs(app, |w| w.get(window).map(|win| win.pages().cloned().collect()).unwrap_or_default())
}

/// Whether any window of tabs is open.
pub fn any(app: &AppHandle) -> bool {
    with_tabs(app, |w| !w.is_empty())
}

/// A window got the keys: it is the one in front, and its tab in front the page in front.
pub fn window_focused(app: &AppHandle, window: &str) {
    if !with_tabs(app, |w| w.contains_key(window)) {
        return;
    }
    *app.state::<Tabs>().focused.lock().unwrap() = Some(window.to_string());
    if let Some(page) = front_of(app, window) {
        crate::documents::focused(app, &page);
    }
}

/// The window of tabs in front: AppKit's key window (on the main thread, where the menu's events come), else the
/// last that got the keys.
fn front_window(app: &AppHandle) -> Option<String> {
    let ours = |w: &String| with_tabs(app, |t| t.contains_key(w));
    key_window(app).filter(ours).or_else(|| app.state::<Tabs>().focused.lock().unwrap().clone().filter(ours))
}

/// The page in front: the tab in front of the window in front.
pub fn front_page(app: &AppHandle) -> Option<Webview> {
    let window = front_window(app)?;
    app.get_webview(&front_of(app, &window)?)
}

/// The label of the key window, as AppKit says; none off the main thread or when no window of the app is key.
fn key_window(app: &AppHandle) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        let mtm = MainThreadMarker::new()?;
        let key = NSApplication::sharedApplication(mtm).keyWindow()?;
        let key = objc2::rc::Retained::as_ptr(&key) as usize;
        app.windows()
            .into_iter()
            .find(|(_, w)| w.ns_window().is_ok_and(|p| p as usize == key))
            .map(|(label, _)| label)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        None
    }
}

/// ⌘W: the tab in front of the window in front closed; a window that has no tabs (Settings) closes.
pub fn close_front(app: &AppHandle) {
    match key_window(app) {
        Some(window) if !with_tabs(app, |w| w.contains_key(&window)) => {
            if let Some(window) = app.get_window(&window) {
                let _ = window.close();
            }
        }
        _ => {
            if let Some(page) = front_page(app) {
                close(app, page.label());
            }
        }
    }
}

// --- projects: switching a window, opening one in a window of its own, saving and restoring

/// Where a project not shown by any window goes.
pub enum Target {
    /// the window in front (a new one when there is none)
    Front,
    /// a window of its own
    New,
    /// that window
    Window(String),
}

/// A project shown (a tile clicked, File ▸ Open Project…, Open in New Window): the window that holds it brought
/// forward and switched to it; else the window `into` switched to it, its tabs as saved, else its main document.
pub fn show_project(app: &AppHandle, root: &Path, into: Target) -> Result<(), String> {
    if let Some(window) = window_of_project(app, root) {
        let page = with_tabs(app, |w| {
            let win = w.get_mut(&window)?;
            let i = win.set_for(Some(root))?;
            win.raise(i);
            win.sets[0].front.clone()
        });
        if let Some(page) = page {
            select(app, &page);
        }
        return Ok(());
    }
    let window = match into {
        Target::Window(label) if with_tabs(app, |w| w.contains_key(&label)) => label,
        Target::Front if let Some(front) = front_window(app) => front,
        _ => make_window(app).map_err(|e| e.to_string())?.label().to_string(),
    };
    with_tabs(app, |w| {
        let win = w.entry(window.clone()).or_default();
        win.sets.insert(0, Set { project: Some(root.to_path_buf()), ..Set::default() });
    });
    restore_project(app, root)
}

/// A project's tabs opened again in the set its window shows (made by the caller), as saved: the files still there,
/// the one in front; none saved, or none left, its main document.
fn restore_project(app: &AppHandle, root: &Path) -> Result<(), String> {
    let layout = crate::projects::layout(app, root);
    let files: Vec<PathBuf> = layout.files.iter().filter(|f| f.is_file()).cloned().collect();
    let files = if files.is_empty() {
        let main = write_core::project::Project::read(root).ok().and_then(|p| p.main());
        main.into_iter().collect()
    } else {
        files
    };
    if files.is_empty() {
        return Err(format!("{}: no main document (Project Info…)", root.display()));
    }
    for file in &files {
        if crate::documents::label_of(app, file).is_none() {
            crate::documents::make(app, Some(file.clone())).map_err(|e| e.to_string())?;
        }
    }
    let front = layout.front.filter(|f| files.contains(f)).unwrap_or_else(|| files[0].clone());
    if let Some(label) = crate::documents::label_of(app, &front) {
        select(app, &label);
    }
    Ok(())
}

/// The windows open as the app last quit, opened again, each project with its tabs, the one shown first in front;
/// whether any was.
pub fn restore(app: &AppHandle) -> bool {
    let windows = crate::projects::saved_windows(app);
    let state = app.state::<Tabs>();
    state.restoring.store(true, Ordering::SeqCst);
    let mut any_made = false;
    for saved in windows {
        let projects: Vec<&PathBuf> = saved.projects.iter().filter(|p| p.is_dir() && window_of_project(app, p).is_none()).collect();
        let Some((first, rest)) = projects.split_first() else {
            continue;
        };
        // the first in a new window, the others behind it in the same window, the first shown last
        if show_project(app, first, Target::New).is_err() {
            continue;
        }
        any_made = true;
        let Some(window) = window_of_project(app, first) else {
            continue;
        };
        for project in rest.iter().rev() {
            let _ = show_project(app, project, Target::Window(window.clone()));
        }
        let _ = show_project(app, first, Target::Window(window));
    }
    state.restoring.store(false, Ordering::SeqCst);
    save(app);
    any_made
}

/// What is kept of the tabs saved: each project's tabs, and the windows with their projects.
fn save(app: &AppHandle) {
    let state = app.state::<Tabs>();
    if state.restoring.load(Ordering::SeqCst) || state.quitting.load(Ordering::SeqCst) {
        return;
    }
    let layouts: Vec<(PathBuf, Layout)> = with_tabs(app, |w| {
        w.values()
            .flat_map(|win| win.sets.iter())
            .filter_map(|set| {
                let root = set.project.clone()?;
                let path = |page: &String| crate::documents::path_of(app, page);
                let files = set.pages.iter().filter_map(path).collect();
                Some((root, Layout { files, front: set.front.as_ref().and_then(path) }))
            })
            .collect()
    });
    crate::projects::save_layouts(app, layouts);
    save_windows(app);
}

/// The windows saved, in the order they were made, each with its projects, the one shown first.
fn save_windows(app: &AppHandle) {
    let state = app.state::<Tabs>();
    if state.restoring.load(Ordering::SeqCst) {
        return;
    }
    let order = state.order.lock().unwrap().clone();
    let windows: Vec<SavedWindow> = with_tabs(app, |w| {
        order
            .iter()
            .filter_map(|label| w.get(label))
            .map(|win| SavedWindow { projects: win.sets.iter().filter_map(|s| s.project.clone()).collect() })
            .filter(|s| !s.projects.is_empty())
            .collect()
    });
    crate::projects::save_windows(app, windows);
}

// --- the pages' commands

/// The tabs of the asking page's set, in order.
#[tauri::command]
pub fn tab_strip(page: Webview) -> Vec<Tab> {
    let app = page.app_handle();
    let pages = with_tabs(app, |w| {
        w.values().find_map(|win| win.set_of(page.label()).map(|i| win.sets[i].pages.clone())).unwrap_or_default()
    });
    pages
        .into_iter()
        .map(|label| Tab {
            title: title_of(app, &label),
            path: crate::documents::path_of(app, &label).map(|p| p.display().to_string()),
            label,
        })
        .collect()
}

/// A tab chosen.
#[tauri::command]
pub fn select_tab(app: AppHandle, label: String) {
    select(&app, &label);
}

/// A tab's close button.
#[tauri::command]
pub fn close_tab(app: AppHandle, label: String) {
    close(&app, &label);
}
