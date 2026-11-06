//! The windows and their tabs. A window (`win-N`) holds pages, one per tab (a document's `doc-N`, a history's
//! `history-N`), as children filling it; the tab in front is shown, the others hidden but alive (their text, undo,
//! scroll and find stay). A project's documents share its window; a file outside any project joins the window in
//! front; with Settings ▸ Open files as tabs off, each file has a window of its own. The pages draw the tabs of
//! their window themselves (`tab_strip`), told `tabs-changed` when a tab comes, goes or is renamed, and
//! `tab-shown` (true or false) when theirs comes to the front or leaves it. Which window is in front is AppKit's to
//! say (its key window), else the last focused.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, Webview, WebviewBuilder, WebviewUrl,
    Window, WindowBuilder,
};

/// The windows' tabs, by window label.
#[derive(Default)]
pub struct Tabs {
    windows: Mutex<HashMap<String, Group>>,
    /// the number of the next window
    next: AtomicUsize,
    /// the window that last had the keys
    focused: Mutex<Option<String>>,
}

/// A window's tabs: their pages in order, the one in front, the project the window is for (none: loose files).
#[derive(Default)]
struct Group {
    pages: Vec<String>,
    front: Option<String>,
    project: Option<PathBuf>,
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

/// The window a page is in.
pub fn window_of(app: &AppHandle, page: &str) -> Option<String> {
    let state = app.state::<Tabs>();
    let windows = state.windows.lock().unwrap();
    windows.iter().find(|(_, g)| g.pages.iter().any(|p| p == page)).map(|(w, _)| w.clone())
}

/// The window of a project, if one is open.
pub fn window_of_project(app: &AppHandle, root: &Path) -> Option<String> {
    let state = app.state::<Tabs>();
    let windows = state.windows.lock().unwrap();
    windows.iter().find(|(_, g)| g.project.as_deref() == Some(root)).map(|(w, _)| w.clone())
}

/// The page in front of a window.
fn front_of(app: &AppHandle, window: &str) -> Option<String> {
    app.state::<Tabs>().windows.lock().unwrap().get(window)?.front.clone()
}

/// The window a new page goes in: its project's (none open: a new one); for a file outside any project, or
/// untitled, the window in front; none (a new window) when files open in windows of their own.
fn window_for(app: &AppHandle, project: Option<&Path>) -> Option<String> {
    if !crate::settings::get().tabs {
        return None;
    }
    match project {
        Some(root) => window_of_project(app, root),
        None => front_window(app),
    }
}

/// A new window, for `project` (none: loose files).
fn make_window(app: &AppHandle, project: Option<PathBuf>) -> tauri::Result<Window> {
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
    state.windows.lock().unwrap().insert(label, Group { project, ..Group::default() });
    Ok(window)
}

/// A page made at `url` with `label`, as a tab of the window for `path` (its project's, the one in front, or a new
/// one), after the tab in front, and brought to the front.
pub fn add(app: &AppHandle, label: &str, url: &str, path: Option<&Path>) -> tauri::Result<Webview> {
    let project = path.and_then(write_core::project::find);
    let window = match window_for(app, project.as_deref()).and_then(|w| app.get_window(&w)) {
        Some(window) => window,
        None => make_window(app, project)?,
    };
    add_to(app, &window, label, url, None)
}

/// A page that is not a document (a history), named `title`, as a tab of the window of the page `beside`.
pub fn add_beside(app: &AppHandle, label: &str, url: &str, beside: &str, title: String) -> tauri::Result<Webview> {
    let window = match window_of(app, beside).and_then(|w| app.get_window(&w)) {
        Some(window) => window,
        None => make_window(app, None)?,
    };
    add_to(app, &window, label, url, Some(title))
}

fn add_to(app: &AppHandle, window: &Window, label: &str, url: &str, title: Option<String>) -> tauri::Result<Webview> {
    let size = window.inner_size()?.to_logical::<f64>(window.scale_factor()?);
    let page = window.add_child(
        WebviewBuilder::new(label, WebviewUrl::App(url.into())).auto_resize(),
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(size.width, size.height),
    )?;
    {
        let state = app.state::<Tabs>();
        let mut windows = state.windows.lock().unwrap();
        let group = windows.entry(window.label().to_string()).or_default();
        let at = group
            .front
            .as_ref()
            .and_then(|f| group.pages.iter().position(|p| p == f))
            .map_or(group.pages.len(), |i| i + 1);
        group.pages.insert(at, label.to_string());
        if let Some(title) = title {
            group.titles.insert(label.to_string(), title);
        }
    }
    select(app, label);
    Ok(page)
}

/// A tab brought to the front: its page shown, the others of its window hidden, the window titled after it and
/// brought forward.
pub fn select(app: &AppHandle, page: &str) {
    let Some(window_label) = window_of(app, page) else {
        return;
    };
    let pages = {
        let state = app.state::<Tabs>();
        let mut windows = state.windows.lock().unwrap();
        let Some(group) = windows.get_mut(&window_label) else {
            return;
        };
        group.front = Some(page.to_string());
        group.pages.clone()
    };
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
        let _ = window.set_title(&title_of(app, &window_label, page));
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

/// A window brought forward as it is, its tab in front kept.
pub fn bring_forward(app: &AppHandle, window: &str) {
    if let Some(page) = front_of(app, window) {
        select(app, &page);
    }
}

/// A tab's name: its file's, or the page's own.
fn title_of(app: &AppHandle, window: &str, page: &str) -> String {
    let own = app.state::<Tabs>().windows.lock().unwrap().get(window).and_then(|g| g.titles.get(page).cloned());
    own.unwrap_or_else(|| crate::documents::title(crate::documents::path_of(app, page).as_deref()))
}

/// A page shows another file (an untitled one saved, a blank one taking a file): its window's title and tabs. A
/// window of loose files with this tab alone becomes the window of the file's project.
pub fn renamed(app: &AppHandle, page: &str) {
    let Some(window_label) = window_of(app, page) else {
        return;
    };
    let project = crate::documents::path_of(app, page).and_then(|p| write_core::project::find(&p));
    if let Some(project) = project {
        let state = app.state::<Tabs>();
        let mut windows = state.windows.lock().unwrap();
        let taken = windows.values().any(|g| g.project.as_ref() == Some(&project));
        if let Some(group) = windows.get_mut(&window_label)
            && group.project.is_none()
            && group.pages.len() == 1
            && !taken
        {
            group.project = Some(project);
        }
    }
    if front_of(app, &window_label).as_deref() == Some(page)
        && let Some(window) = app.get_window(&window_label)
    {
        let _ = window.set_title(&title_of(app, &window_label, page));
    }
    changed(app, &window_label);
}

/// The pages of a window told to ask for their tabs again.
fn changed(app: &AppHandle, window: &str) {
    for page in pages_of(app, window) {
        let _ = app.emit_to(EventTarget::webview(&page), "tabs-changed", ());
    }
}

/// A tab closed (⌘W, its close button): a page with untitled text asks first; then it goes.
pub fn close(app: &AppHandle, page: &str) {
    if !crate::documents::asks_before_closing(app, page) {
        remove(app, page);
    }
}

/// A page and its tab gone (asked already): what was kept of it goes; the neighbour comes to the front; the window
/// closes with its last tab.
pub fn remove(app: &AppHandle, page: &str) {
    let Some(window_label) = window_of(app, page) else {
        return;
    };
    let (next, empty) = {
        let state = app.state::<Tabs>();
        let mut windows = state.windows.lock().unwrap();
        let Some(group) = windows.get_mut(&window_label) else {
            return;
        };
        let i = group.pages.iter().position(|p| p == page).unwrap_or(0);
        group.pages.retain(|p| p != page);
        group.titles.remove(page);
        let was_front = group.front.as_deref() == Some(page);
        if was_front {
            group.front = None;
        }
        // the tab after it, else the one before
        let next = if was_front { group.pages.get(i).or_else(|| group.pages.last()).cloned() } else { None };
        (next, group.pages.is_empty())
    };
    forget(app, page);
    if let Some(webview) = app.get_webview(page) {
        let _ = webview.close();
    }
    if empty {
        app.state::<Tabs>().windows.lock().unwrap().remove(&window_label);
        if let Some(window) = app.get_window(&window_label) {
            let _ = window.destroy();
        }
        if !any(app) {
            crate::documents::regular(app, false);
        }
    } else if let Some(next) = next {
        select(app, &next);
    } else {
        changed(app, &window_label);
    }
}

/// What the app kept of a page: its document, its history.
fn forget(app: &AppHandle, page: &str) {
    crate::documents::closed(app, page);
    crate::git::closed(app, page);
    crate::spelling::closed(app, page);
}

/// A window gone (its close button, after asking about untitled text; the app quitting): its pages and what was
/// kept of them.
pub fn window_closed(app: &AppHandle, window: &str) {
    let group = app.state::<Tabs>().windows.lock().unwrap().remove(window);
    for page in group.map(|g| g.pages).unwrap_or_default() {
        forget(app, &page);
    }
    let state = app.state::<Tabs>();
    let mut focused = state.focused.lock().unwrap();
    if focused.as_deref() == Some(window) {
        *focused = None;
    }
    drop(focused);
    if !any(app) {
        crate::documents::regular(app, false);
    }
}

/// The pages of a window, in order.
pub fn pages_of(app: &AppHandle, window: &str) -> Vec<String> {
    app.state::<Tabs>().windows.lock().unwrap().get(window).map(|g| g.pages.clone()).unwrap_or_default()
}

/// Whether any window of tabs is open.
pub fn any(app: &AppHandle) -> bool {
    !app.state::<Tabs>().windows.lock().unwrap().is_empty()
}

/// A window got the keys: it is the one in front, and its tab in front the page in front.
pub fn window_focused(app: &AppHandle, window: &str) {
    if !app.state::<Tabs>().windows.lock().unwrap().contains_key(window) {
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
    let ours = |w: &String| app.state::<Tabs>().windows.lock().unwrap().contains_key(w);
    key_window(app)
        .filter(ours)
        .or_else(|| app.state::<Tabs>().focused.lock().unwrap().clone().filter(ours))
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

/// The tabs of the asking page's window, in order.
#[tauri::command]
pub fn tab_strip(page: Webview) -> Vec<Tab> {
    let app = page.app_handle();
    let Some(window) = window_of(app, page.label()) else {
        return Vec::new();
    };
    pages_of(app, &window)
        .into_iter()
        .map(|label| Tab {
            title: title_of(app, &window, &label),
            path: crate::documents::path_of(app, &label).map(|p| p.display().to_string()),
            label,
        })
        .collect()
}

/// ⌘W: the tab in front of the window in front closed; a window that has no tabs (Settings) closes.
pub fn close_front(app: &AppHandle) {
    match key_window(app) {
        Some(window) if !app.state::<Tabs>().windows.lock().unwrap().contains_key(&window) => {
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
