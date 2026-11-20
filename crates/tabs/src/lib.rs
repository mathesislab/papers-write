//! The windows of the Papers apps (Papers Write, Papers Review) and their tabs. A window (`win-N`) holds pages, one
//! per tab, as children filling it. A window shows one project at a time: its pages are grouped in sets, one per
//! project, the one shown first; the page of the tab in front of it is above the others, which stay behind it,
//! alive and drawn (text, undo, scroll and find stay). A project's tile switches the window to that project (its tabs as left, or as saved, else
//! what the app opens it with), or brings forward the window that shows it; Open in New Window gives it a window of
//! its own. A file opens in its project's set, the window switched to it; a file outside any project in the set
//! shown in front. When the app says so (`Host::windows`), each project, or each page, has a window of its own.
//!
//! What a project's tabs are (its files, the one in front) is saved as they change, and so are the windows and their
//! projects, except while the app quits: the windows open then open again at the next start, each project with its
//! tabs. A window closed by hand is not; its projects keep their tabs for the next time.
//!
//! The pages draw the tabs of their set themselves (`tab_strip`; `Tabs.svelte` of `papers-write/project`), told
//! `tabs-changed` when a tab comes, goes or is renamed, and `tab-shown` (true or false) when theirs comes to the front
//! or leaves it. Which window is in front is AppKit's to say (its key window), else the last focused. A project's
//! sidebar (shown, its width, its tab) is the same in all its pages: asked with `sidebar`, changed with
//! `set_sidebar`, the others told `sidebar-changed` (`ProjectSidebar` of `papers-write/project`); so are the folders
//! open in its file tree (`folders`, `set_folders`, `folders-changed`), which are not saved. A page is shown once it
//! says it is drawn (`page_ready`, `ready()` of `papers-write/project`), the page before staying in view till then.
//!
//! What the pages are is the app's: it gives a `Host` (`Tabs::new`), which makes them, names them, says what file
//! each shows and keeps what is saved; it registers the commands of `commands`, and puts the items of `menu` in its
//! Window menu.

pub mod menu;
mod model;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, Webview, WebviewBuilder, WebviewUrl,
    Window, WindowBuilder,
};

use model::{Set, Win};

/// A project's tabs as saved: its files in order, the one in front.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub files: Vec<PathBuf>,
    pub front: Option<PathBuf>,
    /// the tabs that go with a file of the project (a text's preview), made again beside it
    pub companions: Vec<Companion>,
    /// the tab in front, when it is one of those (`front` its file then)
    pub front_companion: Option<Companion>,
}

/// A tab that goes with a file (made with `add_beside`; the host says its kind: `Host::companion_kind`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Companion {
    pub file: PathBuf,
    pub kind: String,
}

/// A window as saved: its projects, the one shown first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedWindow {
    pub projects: Vec<PathBuf>,
}

/// A project's sidebar, as all its pages show it: shown or not, its width (points), the tab it shows (the app's).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sidebar {
    pub shown: bool,
    pub width: f64,
    pub tab: Option<String>,
}

impl Default for Sidebar {
    fn default() -> Sidebar {
        Sidebar { shown: true, width: 240.0, tab: None }
    }
}

/// The projects' sidebars as kept, and the last one changed, which a project shown for the first time takes and then
/// keeps as its own (a project does not follow another's changes). Kept by the app (`Host::with_sidebars`); its fields
/// are those of the app's saved state (`#[serde(flatten)]`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sidebars {
    pub sidebars: std::collections::BTreeMap<PathBuf, Sidebar>,
    pub sidebar: Sidebar,
}

impl Sidebars {
    /// A project's sidebar (none: the pages of no project, which show the last one changed).
    pub fn of(&mut self, root: Option<&Path>) -> Sidebar {
        match root {
            Some(root) => self.sidebars.entry(root.to_path_buf()).or_insert_with(|| self.sidebar.clone()).clone(),
            None => self.sidebar.clone(),
        }
    }

    /// A project's sidebar changed.
    pub fn set(&mut self, root: Option<&Path>, sidebar: Sidebar) {
        if let Some(root) = root {
            self.sidebars.insert(root.to_path_buf(), sidebar.clone());
        }
        self.sidebar = sidebar;
    }
}

/// How pages share windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Windows {
    /// a window shows one project at a time, switched between them
    Shared,
    /// each project in a window of its own
    PerProject,
    /// each page in a window of its own
    PerPage,
}

/// How a new window looks: its title before a tab names it, its size and its least size (points).
pub struct WindowStyle {
    pub title: &'static str,
    pub size: (f64, f64),
    pub least: (f64, f64),
}

/// What the app says of its pages, and does as their tabs come and go.
/// When the pages of tabs are unloaded (`Host::unload_policy`), checked once a minute: a tab unseen for `idle`, the
/// tabs beyond the `most` recently seen, and while the app takes more than `memory`, the least recently seen.
pub struct Unloading {
    /// none: however long a tab goes unseen
    pub idle: Option<std::time::Duration>,
    pub most: usize,
    /// bytes, the app's own processes and its pages' (none: no limit; Linux only)
    pub memory: Option<u64>,
}

pub trait Host: Send + Sync + 'static {
    fn style(&self) -> WindowStyle;

    /// The file a page shows (none: untitled, or not a file).
    fn path(&self, app: &AppHandle, page: &str) -> Option<PathBuf>;

    /// A tab's name.
    fn title(&self, app: &AppHandle, page: &str) -> String;

    /// A window's title, `page` the tab in front, named `tab`.
    fn window_title(&self, _app: &AppHandle, _page: &str, tab: &str) -> String {
        tab.to_string()
    }

    /// Whether a page has changes not saved: its tab says so, and its window while it is in front, as the platform
    /// does (macOS: the dot in the window's close button; elsewhere a star after the title).
    fn edited(&self, _app: &AppHandle, _page: &str) -> bool {
        false
    }

    /// The project a file is of.
    fn project_of(&self, app: &AppHandle, file: &Path) -> Option<PathBuf>;

    /// How pages share windows.
    fn windows(&self, app: &AppHandle) -> Windows;

    /// A page made for `file` of the project of `root` (none: the project with no file open), as a tab (`add`); its
    /// label.
    fn open(&self, app: &AppHandle, root: &Path, file: Option<&Path>) -> Result<String, String>;

    /// What a project opens with when it has no tabs saved, or none left: a file, or a page without one (none).
    fn first_file(&self, app: &AppHandle, root: &Path) -> Result<Option<PathBuf>, String>;

    /// Whether closing `page` waits for an answer (text never saved, say); the app `remove`s it once given.
    fn asks_before_closing(&self, _app: &AppHandle, _page: &str) -> bool {
        false
    }

    /// A page came to the front of the window in front.
    fn focused(&self, _app: &AppHandle, _page: &str) {}

    /// A page added to `window`.
    fn added(&self, _app: &AppHandle, _window: &Window, _page: &str) {}

    /// A page gone: what the app kept of it.
    fn closed(&self, _app: &AppHandle, _page: &str) {}

    /// A window switched to the project of `root` (none: loose files).
    fn shown(&self, _app: &AppHandle, _window: &str, _root: Option<&Path>) {}

    /// A window gone (closed, or the app quitting), its pages forgotten already.
    fn window_closed(&self, _app: &AppHandle, _window: &str) {}

    /// The last window of tabs gone.
    fn none_left(&self, _app: &AppHandle) {}

    /// What kind of tab `page` is when it goes with another's file (made with `add_beside`: a preview), to be saved
    /// and made again (`open_companion`); none: it is not kept.
    fn companion_kind(&self, _app: &AppHandle, _page: &str) -> Option<String> {
        None
    }

    /// The tab of kind `kind` made beside the page `beside` (as the project's tabs are restored); its label.
    fn open_companion(&self, _app: &AppHandle, _beside: &str, _kind: &str) -> Result<String, String> {
        Err("no such tab".into())
    }

    /** When the pages of tabs are unloaded (`Unloading`); none: never. */
    fn unload_policy(&self, _app: &AppHandle) -> Option<Unloading> {
        None
    }

    /** Whether every tab is loaded at the start (one at a time, `load_later`), or only the tab in front of each
     *  window (and the one it goes with): the others then as they come to the front. */
    fn load_all_at_start(&self, _app: &AppHandle) -> bool {
        true
    }

    /** The colour of a window and of its pages before they are drawn (`dark`: the window's theme is, the system's
     *  unless the app chose one), so that a window opening shows it rather than an empty or half-drawn area; none: the
     *  platform's. The app calls `repaint` when what it gives changes. */
    fn background(&self, _app: &AppHandle, _dark: bool) -> Option<tauri::window::Color> {
        None
    }

    /// A project's tabs as last saved.
    fn layout(&self, app: &AppHandle, root: &Path) -> Layout;

    /// The tabs of the projects open, as they are now.
    fn save_layouts(&self, app: &AppHandle, layouts: Vec<(PathBuf, Layout)>);

    /// The windows to open again at the start.
    fn saved_windows(&self, app: &AppHandle) -> Vec<SavedWindow>;

    /// The windows open now, to open again at the next start.
    fn save_windows(&self, app: &AppHandle, windows: Vec<SavedWindow>);

    /// The sidebars as kept, given to `f` to read or change; kept again after.
    fn with_sidebars(&self, app: &AppHandle, f: &mut dyn FnMut(&mut Sidebars));
}

/// The windows' tabs, by window label, and the app's `Host`: managed by the app (`app.manage(Tabs::new(host))`).
pub struct Tabs {
    host: Box<dyn Host>,
    windows: Mutex<HashMap<String, Win>>,
    /// the windows in the order they were made, for saving them so
    order: Mutex<Vec<String>>,
    /// the number of the next window
    next: AtomicUsize,
    /// the window that last had the keys
    focused: Mutex<Option<String>>,
    /// the app is quitting: its windows close, but stay saved as open
    quitting: AtomicBool,
    /// the windows are being opened again at the start: nothing is saved meanwhile (till the pages made empty are
    /// all loaded: `load_later`)
    restoring: AtomicBool,
    /// the tabs of the windows being made at the start (`restore`): their pages made empty, loaded one at a time after
    deferring: AtomicBool,
    /// the pages made empty at the start, each with the address it loads (`load_later`)
    later: Mutex<Vec<(String, String)>>,
    /// the page each tab made with `add_beside` goes with (a preview: its text's)
    beside: Mutex<HashMap<String, String>>,
    /// when each page was last in front (the tabs unseen for a while have their page unloaded: `unload_idle`)
    last_shown: Mutex<HashMap<String, std::time::Instant>>,
    /// the pages unloaded, each with the address it loads again when its tab comes to the front
    unloaded: Mutex<HashMap<String, String>>,
    /// the unloading's watch started
    unloading: AtomicBool,
    /// the address of the app's pages, from the first page made (what `WebviewUrl::App` joins to)
    base: Mutex<Option<tauri::Url>>,
    /// each window's tabs in the order they were last in front, and its tabs visited, to go back and forward to
    visits: Mutex<HashMap<String, Visits>>,
    /// Switch to Recent Tab going on (⌃Tab pressed again while ⌃ is held): the window, and how far down its recent
    /// tabs; the order of the recent tabs stays as it was till it ends (`recent_done`)
    cycling: Mutex<Option<(String, usize)>>,
    /// the presses of Switch to Recent Tab, for the end of one after a while without a press or ⌃ let go
    cycle_presses: AtomicUsize,
    /// when Switch to Recent Tab was last asked (a press said twice is left out)
    last_recent: Mutex<Option<std::time::Instant>>,
    /// a tab brought back by Back or Forward: not itself a visit
    stepping: AtomicBool,
    /// the folders open in the file tree of each project's sidebar (none: of the pages of no project), the same in
    /// all its pages; not saved: a project opens with its folders closed
    folders: Mutex<HashMap<Option<PathBuf>, Vec<String>>>,
    /// the pages drawn (`page_ready`), which may be shown: one that is not stays hidden, the page shown before it
    /// staying in view, so a tab never shows an empty page
    ready: Mutex<HashSet<String>>,
    /// the page above the others of each window: the one in view (the model's front may be a page still being drawn,
    /// or none yet, as a project comes)
    on_top: Mutex<HashMap<String, String>>,
}

impl Tabs {
    pub fn new(host: impl Host) -> Tabs {
        Tabs {
            host: Box::new(host),
            windows: Mutex::default(),
            order: Mutex::default(),
            next: AtomicUsize::new(0),
            focused: Mutex::default(),
            quitting: AtomicBool::new(false),
            restoring: AtomicBool::new(false),
            deferring: AtomicBool::new(false),
            later: Mutex::default(),
            beside: Mutex::default(),
            last_shown: Mutex::default(),
            unloaded: Mutex::default(),
            unloading: AtomicBool::new(false),
            base: Mutex::default(),
            visits: Mutex::default(),
            cycling: Mutex::default(),
            cycle_presses: AtomicUsize::new(0),
            last_recent: Mutex::default(),
            stepping: AtomicBool::new(false),
            folders: Mutex::default(),
            ready: Mutex::default(),
            on_top: Mutex::default(),
        }
    }
}

/// A window's tabs as visited: the most recently in front first, and those left (Back) and come back from (Forward).
#[derive(Default)]
struct Visits {
    recent: Vec<String>,
    back: Vec<String>,
    forward: Vec<String>,
}

/// A tab of the strip.
#[derive(Serialize)]
pub struct Tab {
    pub(crate) label: String,
    /// the file's name, Untitled, or the page's own (a history)
    title: String,
    /// the file's path, for the tooltip
    path: Option<String>,
    /// changes not saved
    edited: bool,
}

fn host(app: &AppHandle) -> &dyn Host {
    app.state::<Tabs>().inner().host.as_ref()
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

/// The project a window shows.
pub fn project_of_window(app: &AppHandle, window: &str) -> Option<PathBuf> {
    with_tabs(app, |w| w.get(window)?.shown().map(Path::to_path_buf))
}

/// The project whose tab a page is.
pub fn project_of_page(app: &AppHandle, page: &str) -> Option<PathBuf> {
    with_tabs(app, |w| w.values().find_map(|win| win.sets[win.set_of(page)?].project.clone()))
}

/// The pages of a project's tabs.
pub fn pages_of_project(app: &AppHandle, root: &Path) -> Vec<String> {
    with_tabs(app, |w| {
        w.values().find_map(|win| win.set_for(Some(root)).map(|i| win.sets[i].pages.clone())).unwrap_or_default()
    })
}

/// The projects with tabs in a window.
pub fn projects(app: &AppHandle) -> Vec<PathBuf> {
    with_tabs(app, |w| w.values().flat_map(|win| win.sets.iter().filter_map(|s| s.project.clone())).collect())
}

/// The pages of the tabs of a project, in every window (none: the pages of no project).
fn pages_of_set(app: &AppHandle, root: Option<&Path>) -> Vec<String> {
    with_tabs(app, |w| {
        w.values().flat_map(|win| win.sets.iter()).filter(|s| s.project.as_deref() == root).flat_map(|s| s.pages.clone()).collect()
    })
}

/// The windows of tabs, by label.
pub fn windows(app: &AppHandle) -> Vec<String> {
    with_tabs(app, |w| w.keys().cloned().collect())
}

/// The page in front of a window: the tab in front of the set it shows.
pub fn front_of(app: &AppHandle, window: &str) -> Option<String> {
    with_tabs(app, |w| w.get(window)?.sets.first()?.front.clone())
}

/// The page that shows `file`, if one does.
pub fn page_of(app: &AppHandle, file: &Path) -> Option<String> {
    let pages: Vec<String> = with_tabs(app, |w| w.values().flat_map(Win::pages).cloned().collect());
    pages.into_iter().find(|p| host(app).path(app, p).as_deref() == Some(file))
}

/// A tab's name: the page's own (a history), else the app's.
fn title_of(app: &AppHandle, page: &str) -> String {
    let own = with_tabs(app, |w| w.values().flat_map(|win| win.sets.iter()).find_map(|s| s.titles.get(page).cloned()));
    own.unwrap_or_else(|| host(app).title(app, page))
}

/// The title of the window of `page`, in front.
fn window_title(app: &AppHandle, page: &str) -> String {
    host(app).window_title(app, page, &title_of(app, page))
}

/// A window titled after its tab in front, `page`, and showing whether it has changes not saved as the platform does
/// (`Host::edited`).
fn titled(app: &AppHandle, window: &Window, page: &str) {
    let title = window_title(app, page);
    let edited = host(app).edited(app, page);
    #[cfg(target_os = "macos")]
    {
        let _ = window.set_title(&title);
        let shown = window.clone();
        let _ = window.run_on_main_thread(move || {
            // the window asked for again on the main thread: gone meanwhile, it has none
            if let Ok(ns) = shown.ns_window() {
                // SAFETY: the NSWindow of a window that is there, on the main thread
                let ns = unsafe { &*(ns as *const objc2_app_kit::NSWindow) };
                ns.setDocumentEdited(edited);
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = window.set_title(&if edited { format!("{title} *") } else { title });
}

/// A new window, empty.
fn make_window(app: &AppHandle) -> tauri::Result<Window> {
    let state = app.state::<Tabs>();
    let label = format!("win-{}", state.next.fetch_add(1, Ordering::Relaxed) + 1);
    let style = state.host.style();
    // a little below and right of the window in front, as macOS cascades windows
    let at = front_window(app).and_then(|w| app.get_window(&w)).and_then(|w| {
        let scale = w.scale_factor().ok()?;
        let p = w.outer_position().ok()?.to_logical::<f64>(scale);
        Some((p.x + 24.0, p.y + 24.0))
    });
    let mut builder = WindowBuilder::new(app, &label)
        .title(style.title)
        .inner_size(style.size.0, style.size.1)
        .min_inner_size(style.least.0, style.least.1);
    // never merged into AppKit's tabs: the pages draw their own
    #[cfg(target_os = "macos")]
    {
        builder = builder.tabbing_identifier(&label);
    }
    builder = match at {
        Some((x, y)) => builder.position(x, y),
        None => builder.center(),
    };
    let window = builder.build()?;
    if let Some(colour) = background_of(app, &window) {
        let _ = window.set_background_color(Some(colour));
    }
    state.windows.lock().unwrap().insert(label.clone(), Win::default());
    state.order.lock().unwrap().push(label);
    Ok(window)
}

/// The window a page of `project` goes in, and that the window shows that project's set (made when new): the
/// window that holds the project; else the window in front, switched to it; else a new window. With projects in
/// windows of their own, a new window rather than another project's; with pages in windows of their own, always a
/// new window. A page of no project goes in the set the window in front shows.
fn place(app: &AppHandle, project: Option<&Path>) -> tauri::Result<Window> {
    let mode = host(app).windows(app);
    let held = if mode == Windows::PerPage { None } else { project.and_then(|p| window_of_project(app, p)) };
    let label = match held {
        Some(label) => label,
        None => match (mode, front_window(app)) {
            (Windows::Shared, Some(front)) => front,
            (Windows::PerProject, Some(front)) if project.is_none() => front,
            _ => make_window(app)?.label().to_string(),
        },
    };
    with_tabs(app, |w| w.entry(label.clone()).or_default().show(project));
    app.get_window(&label).ok_or(tauri::Error::WindowNotFound)
}

/// A page made at `url` with `label`, as a tab of `project` (`place`), after the tab in front, and brought to the
/// front.
pub fn add(app: &AppHandle, label: &str, url: &str, project: Option<&Path>) -> tauri::Result<Webview> {
    let window = place(app, project)?;
    add_to(app, &window, label, url, None)
}

/// A page that is not a file (a history), named `title`, as a tab beside the page `beside`.
pub fn add_beside(app: &AppHandle, label: &str, url: &str, beside: &str, title: String) -> tauri::Result<Webview> {
    app.state::<Tabs>().beside.lock().unwrap().insert(label.to_string(), beside.to_string());
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

/// The background of `window` and its pages, as the app says for the system's theme now.
fn background_of(app: &AppHandle, window: &Window) -> Option<tauri::window::Color> {
    let dark = window.theme().is_ok_and(|t| t == tauri::Theme::Dark);
    host(app).background(app, dark)
}

/// The windows and their pages given their background again (`Host::background`), when it changes: the app's
/// appearance or the system's.
pub fn repaint(app: &AppHandle) {
    let windows: Vec<String> = with_tabs(app, |w| w.keys().cloned().collect());
    for label in windows {
        let Some(window) = app.get_window(&label) else { continue };
        let Some(colour) = background_of(app, &window) else { continue };
        let _ = window.set_background_color(Some(colour));
        for page in pages_of(app, &label).iter().filter_map(|p| app.get_webview(p)) {
            let _ = page.set_background_color(Some(colour));
        }
    }
}

/// The page made as a tab of the set `window` shows first.
fn add_to(app: &AppHandle, window: &Window, label: &str, url: &str, title: Option<String>) -> tauri::Result<Webview> {
    let size = window.inner_size()?.to_logical::<f64>(window.scale_factor()?);
    let state = app.state::<Tabs>();
    // at the start, the pages after the first are made empty and loaded one at a time once the windows are there
    // (`load_later`): twenty tabs loading at once kept every one of them, the one in view too, from showing for
    // seconds
    let later = state.deferring.load(Ordering::SeqCst) && state.base.lock().unwrap().is_some();
    let address = if later { WebviewUrl::External("about:blank".parse().expect("a URL")) } else { WebviewUrl::App(url.into()) };
    let mut builder = WebviewBuilder::new(label, address).auto_resize();
    if let Some(colour) = background_of(app, window) {
        builder = builder.background_color(colour);
    }
    let page = window.add_child(builder, LogicalPosition::new(0.0, 0.0), LogicalSize::new(size.width, size.height))?;
    if later {
        state.later.lock().unwrap().push((label.to_string(), url.to_string()));
    } else if state.base.lock().unwrap().is_none() {
        *state.base.lock().unwrap() = page.url().ok().and_then(|u| u.join("/").ok());
    }
    // behind the page in view till it says it is drawn (`page_ready`); one that never says so comes to the front
    // after a while all the same
    let on_top = app.state::<Tabs>().on_top.lock().unwrap().get(window.label()).cloned();
    if let Some(shown) = on_top.and_then(|f| app.get_webview(&f)) {
        behind(&page, &shown);
    } else if cfg!(not(target_os = "macos")) {
        // the first page of a window: outside macOS, hidden till it is drawn, the window's background shown
        // meanwhile rather than the page being drawn (`reveal` shows it)
        let _ = page.hide();
    }
    eprintln!("[papers-write {}] page {label} added{}", stamp(), if later { " (loaded later)" } else { "" });
    if !later {
        show_after_a_while(app, label);
    }
    with_tabs(app, |w| w.entry(window.label().to_string()).or_default().insert(label, title));
    host(app).added(app, window, label);
    select(app, label);
    Ok(page)
}

/// The time of day to the millisecond (UTC), for the log of the start (dev build).
pub fn stamp() -> String {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let s = ms / 1000;
    format!("{:02}:{:02}:{:02}.{:03}", s / 3600 % 24, s / 60 % 60, s % 60, ms % 1000)
}

/// A page that never says it is drawn shown all the same, after a while.
fn show_after_a_while(app: &AppHandle, page: &str) {
    let (handle, waited) = (app.clone(), page.to_string());
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        if !handle.state::<Tabs>().ready.lock().unwrap().contains(&waited) {
            eprintln!("[papers-write {}] page {waited} not drawn after 2 s: shown all the same", stamp());
        }
        ready(&handle, &waited);
    });
}

/// The pages made empty at the start loaded, one at a time, each once the one before is drawn (or a few seconds
/// passed): first the tab in front of a window, then those in front of its other projects, then the others, a tab
/// chosen meanwhile coming next.
fn load_later(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        let app = &handle;
        let state = app.state::<Tabs>();
        let base = state.base.lock().unwrap().clone();
        let all = state.host.load_all_at_start(app);
        loop {
            let next = {
                let mut later = state.later.lock().unwrap();
                let beside = state.beside.lock().unwrap().clone();
                let rank = |label: &str| {
                    let window = window_of(app, label);
                    let front = window.as_deref().and_then(|w| front_of(app, w));
                    // the tab in front, and the one it goes with (a preview's text, which draws it)
                    if front.as_deref() == Some(label) || front.and_then(|f| beside.get(&f).cloned()).as_deref() == Some(label) {
                        0
                    } else if with_tabs(app, |w| w.values().any(|win| win.sets.iter().any(|s| s.front.as_deref() == Some(label)))) {
                        1
                    } else {
                        2
                    }
                };
                let best = (0..later.len()).min_by_key(|&i| rank(&later[i].0));
                best.map(|i| {
                    let front = rank(&later[i].0) == 0;
                    let (label, url) = later.remove(i);
                    (label, url, front)
                })
            };
            let Some((label, url, front)) = next else { break };
            let (Some(page), Some(base)) = (app.get_webview(&label), base.as_ref()) else { continue };
            let address = if url == "index.html" { Ok(base.clone()) } else { base.join(&url) };
            let Ok(address) = address else { continue };
            // only the tabs in front loaded (`Host::load_all_at_start`): the others as unloaded ones, loaded as they
            // come to the front
            if !front && !all {
                state.unloaded.lock().unwrap().insert(label, address.to_string());
                continue;
            }
            eprintln!("[papers-write {}] page {label} loading", stamp());
            if page.navigate(address).is_err() {
                continue;
            }
            show_after_a_while(app, &label);
            for _ in 0..80 {
                if state.ready.lock().unwrap().contains(&label) || app.get_webview(&label).is_none() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    });
}

/// The pages of tabs unseen for a while unloaded, and beyond the most recently seen that may keep their page, the
/// least recent first (`Host::unload_policy`): each made empty, its tab staying in the strip, loaded again as it comes
/// to the front (`select`), the page then putting itself back as it was (its place, its undo history: the host's).
/// Never the tab in front of a window, nor the one it goes with (a preview's text). Watched once a minute.
fn unload_idle(app: &AppHandle) {
    let state = app.state::<Tabs>();
    if state.unloading.swap(true, Ordering::SeqCst) {
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
        let app = &handle;
        let state = app.state::<Tabs>();
        let Some(policy) = state.host.unload_policy(app) else { continue };
        let (pages, kept): (Vec<String>, Vec<String>) = with_tabs(app, |w| {
            let pages = w.values().flat_map(Win::pages).cloned().collect();
            let fronts = w.values().filter_map(|win| win.sets.first()?.front.clone()).collect();
            (pages, fronts)
        });
        let beside = state.beside.lock().unwrap().clone();
        let mut keep: std::collections::HashSet<String> = kept.iter().cloned().collect();
        keep.extend(kept.iter().filter_map(|f| beside.get(f).cloned()));
        let unloaded: std::collections::HashSet<String> = state.unloaded.lock().unwrap().keys().cloned().collect();
        let waiting: std::collections::HashSet<String> = state.later.lock().unwrap().iter().map(|(l, _)| l.clone()).collect();
        let shown = state.last_shown.lock().unwrap().clone();
        // the pages loaded, the least recently seen first (never seen: first of all)
        let mut loaded: Vec<(String, Option<std::time::Instant>)> = pages
            .into_iter()
            .filter(|p| !unloaded.contains(p) && !waiting.contains(p))
            .map(|p| {
                let at = shown.get(&p).copied();
                (p, at)
            })
            .collect();
        loaded.sort_by_key(|(_, at)| *at);
        let mut count = loaded.len();
        let over = || policy.memory.is_some_and(|most| memory_used() > most);
        let mut too_much = over();
        for (page, at) in loaded {
            if keep.contains(&page) {
                continue;
            }
            let recent = policy.idle.is_none_or(|idle| at.is_some_and(|at| at.elapsed() < idle));
            if recent && count <= policy.most && !too_much {
                continue;
            }
            let Some(webview) = app.get_webview(&page) else { continue };
            let Ok(url) = webview.url() else { continue };
            if url.as_str() == "about:blank" || webview.navigate("about:blank".parse().expect("a URL")).is_err() {
                continue;
            }
            state.unloaded.lock().unwrap().insert(page.clone(), url.to_string());
            state.ready.lock().unwrap().remove(&page);
            count -= 1;
            if too_much {
                // the page's memory given back, then measured again
                std::thread::sleep(std::time::Duration::from_secs(2));
                too_much = over();
            }
        }
    });
}

/// The memory the app takes, in bytes: its own process and those of its process group it started (the pages'
/// processes; not the commands it runs, which have groups of their own), their proportional share (PSS: what is
/// shared counted once). Linux; elsewhere 0.
fn memory_used() -> u64 {
    #[cfg(target_os = "linux")]
    {
        let me = std::process::id();
        // each process's parent and group: the fields after the name, which is in parentheses
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut group = HashMap::new();
        for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
            let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
            let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else { continue };
            let Some((_, rest)) = stat.rsplit_once(')') else { continue };
            let mut fields = rest.split_whitespace().skip(1);
            let (Some(Ok(parent)), Some(Ok(pgid))) = (fields.next().map(str::parse::<u32>), fields.next().map(str::parse::<u32>)) else { continue };
            children.entry(parent).or_default().push(pid);
            group.insert(pid, pgid);
        }
        let mine = group.get(&me).copied();
        let mut total = 0;
        let mut todo = vec![me];
        while let Some(pid) = todo.pop() {
            if pid != me && group.get(&pid).copied() != mine {
                continue;
            }
            let rollup = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).unwrap_or_default();
            let kb = rollup.lines().find_map(|l| l.strip_prefix("Pss:")).and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok());
            total += kb.unwrap_or(0) * 1024;
            todo.extend(children.get(&pid).into_iter().flatten());
        }
        total
    }
    #[cfg(not(target_os = "linux"))]
    0
}

/// A page drawn (`page_ready`, or the wait for it over): shown when its tab is in front.
pub fn ready(app: &AppHandle, page: &str) {
    let Some(window) = window_of(app, page) else {
        return;
    };
    if app.state::<Tabs>().ready.lock().unwrap().insert(page.to_string()) && front_of(app, &window).as_deref() == Some(page) {
        reveal(app, &window);
    }
}

/// A page put behind `other` in its window: drawn, but not seen (AppKit stacks a window's views in the order they
/// were added; what is above the pages, a pane of the app's, stays above).
fn behind(page: &Webview, other: &Webview) {
    #[cfg(target_os = "macos")]
    {
        let page = page.clone();
        let _ = other.with_webview(move |other| {
            let other = other.inner() as usize;
            let _ = page.with_webview(move |page| {
                use objc2_app_kit::{NSView, NSWindowOrderingMode};
                // the web views live while their window does, and this runs on the main thread
                let (view, other) = unsafe { (&*(page.inner() as *const NSView), &*(other as *const NSView)) };
                if let Some(parent) = unsafe { view.superview() } {
                    parent.addSubview_positioned_relativeTo(view, NSWindowOrderingMode::Below, Some(other));
                }
            });
        });
    }
    // GTK does not stack a window's child webviews: it lays them out one after the other, each taking part of the
    // window. A page not in front is hidden instead (shown again by `reveal`).
    #[cfg(not(target_os = "macos"))]
    {
        let _ = other;
        let _ = page.hide();
    }
}

/// The page of the window's tab in front put above the window's other pages, once it is drawn; till then the page
/// shown before stays. The pages are never hidden, only stacked: a page hidden is not drawn, and shows empty for a
/// moment when shown again.
fn reveal(app: &AppHandle, window: &str) {
    let Some(front) = front_of(app, window) else {
        return;
    };
    if !app.state::<Tabs>().ready.lock().unwrap().contains(&front) {
        return;
    }
    let Some(shown) = app.get_webview(&front) else {
        return;
    };
    app.state::<Tabs>().on_top.lock().unwrap().insert(window.to_string(), front.clone());
    // outside macOS the pages behind are hidden (`behind`): the one in front is shown
    #[cfg(not(target_os = "macos"))]
    let _ = shown.show();
    for other in pages_of(app, window) {
        if let Some(webview) = app.get_webview(&other) {
            if other != front {
                behind(&webview, &shown);
            }
            let _ = app.emit_to(EventTarget::webview(&other), "tab-shown", other == front);
        }
    }
    if let Some(webview) = app.get_webview(&front)
        && key_window(app).as_deref().is_none_or(|key| key == window)
    {
        let _ = webview.set_focus();
    }
}

/// A tab brought to the front: its set shown by its window, its page shown (once drawn: `reveal`) and every other
/// page of the window hidden, the window titled after it and brought forward.
pub fn select(app: &AppHandle, page: &str) {
    let Some(window_label) = window_of(app, page) else {
        return;
    };
    {
        let state = app.state::<Tabs>();
        state.last_shown.lock().unwrap().insert(page.to_string(), std::time::Instant::now());
        // a page unloaded: loaded again, shown once drawn (the one before in view meanwhile)
        let again = state.unloaded.lock().unwrap().remove(page);
        if let (Some(url), Some(webview)) = (again, app.get_webview(page))
            && let Ok(url) = url.parse()
            && webview.navigate(url).is_ok()
        {
            show_after_a_while(app, page);
        }
    }
    visited(app, &window_label, front_of(app, &window_label), page);
    let newly_shown = with_tabs(app, |w| {
        let win = w.get_mut(&window_label)?;
        win.bring(page);
        win.newly_shown()
    });
    if let Some(window) = app.get_window(&window_label) {
        titled(app, &window, page);
        let _ = window.unminimize();
        // GTK shows a window with `show_all`, which shows again the pages hidden behind the one in front (`behind`):
        // a window shown already is not shown again
        if !cfg!(target_os = "linux") || !window.is_visible().unwrap_or(false) {
            let _ = window.show();
        }
        let _ = window.set_focus();
    }
    reveal(app, &window_label);
    *app.state::<Tabs>().focused.lock().unwrap() = Some(window_label.clone());
    if let Some(now) = newly_shown {
        host(app).shown(app, &window_label, now.as_deref());
    }
    host(app).focused(app, page);
    changed(app, &window_label);
}

/// The tab `page` brought to the front of `window` after `before`: the first of its recent tabs (unless Switch to
/// Recent Tab goes on), and `before` the tab Back goes to (unless Back or Forward brought it).
fn visited(app: &AppHandle, window: &str, before: Option<String>, page: &str) {
    let state = app.state::<Tabs>();
    let cycling = state.cycling.lock().unwrap().is_some();
    let stepping = state.stepping.load(Ordering::SeqCst);
    let mut visits = state.visits.lock().unwrap();
    let v = visits.entry(window.to_string()).or_default();
    if let Some(before) = before.filter(|b| b != page) {
        if v.recent.is_empty() {
            v.recent.push(before.clone());
        }
        if !stepping {
            v.back.push(before);
            v.forward.clear();
        }
    }
    if !cycling {
        v.recent.retain(|p| p != page);
        v.recent.insert(0, page.to_string());
    }
}

/// The pages of the set shown in `window` (the tab strip), for the tabs to go to.
fn strip_of(app: &AppHandle, window: &str) -> Vec<String> {
    with_tabs(app, |w| w.get(window).and_then(|win| win.sets.first()).map(|s| s.pages.clone()).unwrap_or_default())
}

/// Switch to Recent Tab (⌃Tab; `by` -1: ⌃⇧Tab): the tab in front before this one; pressed again while ⌃ is held,
/// the one before that, and so on (the order kept till ⌃ is let go, `recent_done`, or a moment passes).
pub fn recent(app: &AppHandle, by: isize) {
    let Some(window) = front_window(app) else {
        return;
    };
    // one press said twice (the page and the menu both): the second left out
    {
        let state = app.state::<Tabs>();
        let mut last = state.last_recent.lock().unwrap();
        let now = std::time::Instant::now();
        if last.is_some_and(|t| now.duration_since(t) < std::time::Duration::from_millis(80)) {
            return;
        }
        *last = Some(now);
    }
    let strip = strip_of(app, &window);
    let state = app.state::<Tabs>();
    let mut recent: Vec<String> = state.visits.lock().unwrap().get(&window).map(|v| v.recent.clone()).unwrap_or_default();
    recent.retain(|p| strip.contains(p));
    // the tabs never in front yet after those that were, in the strip's order
    recent.extend(strip.iter().filter(|p| !recent.contains(p)).cloned().collect::<Vec<_>>());
    if recent.len() < 2 {
        return;
    }
    let target = {
        let mut cycling = state.cycling.lock().unwrap();
        let from = cycling.as_ref().map(|(_, i)| *i);
        let at = match cycling.as_ref() {
            Some((w, i)) if *w == window => *i as isize,
            _ => 0,
        };
        let next = (at + by).rem_euclid(recent.len() as isize) as usize;
        *cycling = Some((window.clone(), next));
        eprintln!("[papers-write] recent tab: {recent:?}, from {from:?} by {by}: {}", recent[next]);
        recent[next].clone()
    };
    select(app, &target);
    // the end of it after a moment without a press, if ⌃ let go was not told
    let press = state.cycle_presses.fetch_add(1, Ordering::SeqCst) + 1;
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        if handle.state::<Tabs>().cycle_presses.load(Ordering::SeqCst) == press {
            recent_done(&handle);
        }
    });
}

/// Switch to Recent Tab over (⌃ let go): the tab it came to the first of the recent ones.
pub fn recent_done(app: &AppHandle) {
    let state = app.state::<Tabs>();
    let Some((window, _)) = state.cycling.lock().unwrap().take() else {
        return;
    };
    if let Some(front) = front_of(app, &window) {
        visited(app, &window, None, &front);
    }
}

/// Back (`back`) or Forward among the tabs visited in the window in front, as a browser goes through pages; tabs
/// closed since are passed over.
pub fn history(app: &AppHandle, back: bool) {
    let Some(window) = front_window(app) else {
        return;
    };
    let strip = strip_of(app, &window);
    let Some(front) = front_of(app, &window) else {
        return;
    };
    let state = app.state::<Tabs>();
    let target = {
        let mut visits = state.visits.lock().unwrap();
        let v = visits.entry(window.clone()).or_default();
        let (from, to) = if back { (&mut v.back, &mut v.forward) } else { (&mut v.forward, &mut v.back) };
        let mut found = None;
        while let Some(page) = from.pop() {
            if strip.contains(&page) && page != front {
                found = Some(page);
                break;
            }
        }
        if found.is_some() {
            to.push(front);
        }
        found
    };
    if let Some(target) = target {
        state.stepping.store(true, Ordering::SeqCst);
        select(app, &target);
        state.stepping.store(false, Ordering::SeqCst);
    }
}

/// The `n`th tab of the strip (1 the first), 9 the last, as browsers have it.
pub fn nth(app: &AppHandle, n: usize) {
    let Some(window) = front_window(app) else {
        return;
    };
    let strip = strip_of(app, &window);
    let page = if n >= 9 { strip.last() } else { strip.get(n.saturating_sub(1)) };
    if let Some(page) = page.cloned() {
        select(app, &page);
    }
}

/// A window titled again after its tab in front (its project renamed, say).
pub fn retitle(app: &AppHandle, window: &str) {
    if let Some(page) = front_of(app, window)
        && let Some(window) = app.get_window(window)
    {
        titled(app, &window, &page);
    }
}

/// A page shows another file (an untitled one saved, a blank one taking a file): its window's title and tabs. A set
/// of loose files with this tab alone becomes the set of the file's project, unless a window holds that one already.
pub fn renamed(app: &AppHandle, page: &str) {
    let Some(window_label) = window_of(app, page) else {
        return;
    };
    let project = host(app).path(app, page).and_then(|p| host(app).project_of(app, &p));
    if let Some(project) = project {
        with_tabs(app, |w| {
            let taken = w.values().any(|win| win.set_for(Some(&project)).is_some());
            if !taken && let Some(win) = w.get_mut(&window_label) {
                win.adopt(page, &project);
            }
        });
    }
    if front_of(app, &window_label).as_deref() == Some(page)
        && let Some(window) = app.get_window(&window_label)
    {
        titled(app, &window, page);
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

/// A tab closed (⌘W, its close button): the app may ask first (`Host::asks_before_closing`); then it goes.
pub fn close(app: &AppHandle, page: &str) {
    if !host(app).asks_before_closing(app, page) {
        remove(app, page);
    }
}

/// A page and its tab gone (asked already): what was kept of it goes; the neighbour comes to the front; a set
/// emptied goes, the window showing the project shown before; the window closes with its last tab.
pub fn remove(app: &AppHandle, page: &str) {
    app.state::<Tabs>().beside.lock().unwrap().remove(page);
    app.state::<Tabs>().unloaded.lock().unwrap().remove(page);
    app.state::<Tabs>().last_shown.lock().unwrap().remove(page);
    let Some(window_label) = window_of(app, page) else {
        return;
    };
    let removed = with_tabs(app, |w| w.get_mut(&window_label).map(|win| win.remove(page)));
    let Some(removed) = removed else {
        return;
    };
    host(app).closed(app, page);
    app.state::<Tabs>().ready.lock().unwrap().remove(page);
    if let Some(webview) = app.get_webview(page) {
        let _ = webview.close();
    }
    if removed.empty {
        // closed by hand, tab by tab: it is not opened again at the next start
        let state = app.state::<Tabs>();
        state.windows.lock().unwrap().remove(&window_label);
        state.order.lock().unwrap().retain(|w| *w != window_label);
        state.on_top.lock().unwrap().remove(&window_label);
        host(app).window_closed(app, &window_label);
        if let Some(window) = app.get_window(&window_label) {
            let _ = window.destroy();
        }
        save(app);
        if !any(app) {
            host(app).none_left(app);
        }
    } else if let Some(next) = removed.next {
        select(app, &next);
    } else {
        changed(app, &window_label);
    }
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
        host(app).closed(app, page);
        state.ready.lock().unwrap().remove(page);
    }
    state.on_top.lock().unwrap().remove(window);
    host(app).window_closed(app, window);
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
        host(app).none_left(app);
    }
}

/// The app quits: its windows close, but stay saved as open.
/// Whether the app is quitting (its windows close, but stay saved as open).
pub fn is_quitting(app: &AppHandle) -> bool {
    app.state::<Tabs>().quitting.load(Ordering::SeqCst)
}

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
        host(app).focused(app, &page);
    }
}

/// The window of tabs in front: AppKit's key window (on the main thread, where the menu's events come), else the
/// last that got the keys.
pub fn front_window(app: &AppHandle) -> Option<String> {
    let ours = |w: &String| with_tabs(app, |t| t.contains_key(w));
    key_window(app).filter(ours).or_else(|| app.state::<Tabs>().focused.lock().unwrap().clone().filter(ours))
}

/// The page in front: the tab in front of the window in front.
pub fn front_page(app: &AppHandle) -> Option<Webview> {
    let window = front_window(app)?;
    app.get_webview(&front_of(app, &window)?)
}

/// The label of the key window, as AppKit says; none off the main thread or when no window of the app is key.
pub fn key_window(app: &AppHandle) -> Option<String> {
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

/// The tab before or after the one in front of the window in front brought to the front, round from the last to
/// the first (⇧⌘[ and ⇧⌘]).
pub fn step(app: &AppHandle, by: isize) {
    let Some(window) = front_window(app) else {
        return;
    };
    let next = with_tabs(app, |w| {
        let set = w.get(&window)?.sets.first()?;
        let n = set.pages.len() as isize;
        let i = set.front.as_ref().and_then(|f| set.pages.iter().position(|p| p == f))? as isize;
        set.pages.get((i + by).rem_euclid(n) as usize).cloned()
    });
    if let Some(next) = next {
        select(app, &next);
    }
}

/// A tab moved to place `to` among the tabs of its set (dragged in the strip): the window's pages told, the order
/// saved.
pub fn move_tab(app: &AppHandle, page: &str, to: usize) {
    let Some(window) = window_of(app, page) else {
        return;
    };
    if with_tabs(app, |w| w.get_mut(&window).is_some_and(|win| win.move_to(page, to))) {
        changed(app, &window);
    }
}

/// The tab in front of the window in front moved one place left or right, not past the ends (Move Tab Left and
/// Move Tab Right).
pub fn shift(app: &AppHandle, by: isize) {
    let Some(window) = front_window(app) else {
        return;
    };
    let at = with_tabs(app, |w| {
        let set = w.get(&window)?.sets.first()?;
        let front = set.front.clone()?;
        let i = set.pages.iter().position(|p| *p == front)? as isize;
        let to = i + by;
        (to >= 0 && (to as usize) < set.pages.len()).then_some((front, to as usize))
    });
    if let Some((page, to)) = at {
        move_tab(app, &page, to);
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

/// A project shown (a tile clicked, Open Project…, Open in New Window): the window that holds it brought forward and
/// switched to it; else the window `into` switched to it, its tabs as saved, else what it opens with.
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
        Target::Front if host(app).windows(app) == Windows::Shared && let Some(front) = front_window(app) => front,
        _ => make_window(app).map_err(|e| e.to_string())?.label().to_string(),
    };
    with_tabs(app, |w| {
        let win = w.entry(window.clone()).or_default();
        win.sets.insert(0, Set { project: Some(root.to_path_buf()), ..Set::default() });
    });
    let opened = restore_project(app, root);
    if opened.is_err() {
        // nothing came of it: the set goes, and the window if it was made for it
        let empty = with_tabs(app, |w| {
            let win = w.get_mut(&window)?;
            let i = win.set_for(Some(root))?;
            if win.sets[i].pages.is_empty() {
                win.sets.remove(i);
            }
            Some(win.sets.is_empty())
        });
        if empty == Some(true) {
            let state = app.state::<Tabs>();
            state.windows.lock().unwrap().remove(&window);
            state.order.lock().unwrap().retain(|w| *w != window);
            if let Some(window) = app.get_window(&window) {
                let _ = window.destroy();
            }
        }
    }
    opened
}

/// A project's tabs opened again in the set its window shows (made by the caller), as saved: the files still there,
/// the one in front; none saved, or none left, what it opens with.
fn restore_project(app: &AppHandle, root: &Path) -> Result<(), String> {
    let host = host(app);
    let layout = host.layout(app, root);
    let mut files: Vec<PathBuf> = layout.files.iter().filter(|f| f.is_file()).cloned().collect();
    if files.is_empty() {
        match host.first_file(app, root)? {
            Some(file) => files.push(file),
            None => {
                host.open(app, root, None)?;
                return Ok(());
            }
        }
    }
    for file in &files {
        if page_of(app, file).is_none() {
            host.open(app, root, Some(file))?;
        }
    }
    let front = layout.front.filter(|f| files.contains(f)).unwrap_or_else(|| files[0].clone());
    // the tabs that went with files (a preview), made again beside them; the one in front again, if it was
    let mut target = page_of(app, &front);
    for c in &layout.companions {
        let Some(beside) = page_of(app, &c.file) else { continue };
        match host.open_companion(app, &beside, &c.kind) {
            Ok(label) if layout.front_companion.as_ref() == Some(c) => target = Some(label),
            Ok(_) => {}
            Err(e) => eprintln!("{}: {e}", c.file.display()),
        }
    }
    if let Some(label) = target {
        select(app, &label);
    }
    Ok(())
}

/// The windows open as the app last quit, opened again, each project with its tabs, the one shown first in front;
/// whether any was.
pub fn restore(app: &AppHandle) -> bool {
    let windows = host(app).saved_windows(app);
    let state = app.state::<Tabs>();
    state.restoring.store(true, Ordering::SeqCst);
    state.deferring.store(true, Ordering::SeqCst);
    let mut any_made = false;
    for saved in windows {
        let projects: Vec<&PathBuf> = saved.projects.iter().filter(|p| p.is_dir() && window_of_project(app, p).is_none()).collect();
        let Some((first, rest)) = projects.split_first() else {
            continue;
        };
        // the first in a new window, the others behind it in the same window, the first shown last
        if let Err(e) = show_project(app, first, Target::New) {
            eprintln!("{e}");
            continue;
        }
        any_made = true;
        let Some(window) = window_of_project(app, first) else {
            continue;
        };
        for project in rest.iter().rev() {
            if let Err(e) = show_project(app, project, Target::Window(window.clone())) {
                eprintln!("{e}");
            }
        }
        let _ = show_project(app, first, Target::Window(window));
    }
    state.deferring.store(false, Ordering::SeqCst);
    // every tab is there with its file, its page loading or not: saved from now on (a quit before the pages are all
    // loaded keeps what was done meanwhile)
    state.restoring.store(false, Ordering::SeqCst);
    save(app);
    // the pages made empty loaded now, one at a time
    load_later(app);
    unload_idle(app);
    any_made
}

/// What is kept of the tabs saved: each project's tabs, and the windows with their projects.
fn save(app: &AppHandle) {
    let state = app.state::<Tabs>();
    if state.restoring.load(Ordering::SeqCst) || state.quitting.load(Ordering::SeqCst) {
        return;
    }
    let sets: Vec<(String, PathBuf, Vec<String>, Option<String>)> = with_tabs(app, |w| {
        w.iter()
            .flat_map(|(label, win)| win.sets.iter().map(move |set| (label, set)))
            .filter_map(|(label, set)| Some((label.clone(), set.project.clone()?, set.pages.clone(), set.front.clone())))
            .collect()
    });
    let path = |page: &String| state.host.path(app, page);
    let recent: HashMap<String, Vec<String>> = state.visits.lock().unwrap().iter().map(|(w, v)| (w.clone(), v.recent.clone())).collect();
    let beside = state.beside.lock().unwrap().clone();
    let layouts = sets
        .into_iter()
        .map(|(window, root, pages, front)| {
            // a tab in front with no file of its own (a preview, a history): the tab of a file in front last, for
            // the project to open on it again
            let front_file = front.as_ref().and_then(path).or_else(|| {
                recent.get(&window)?.iter().filter(|p| pages.contains(p)).find_map(path)
            });
            // the tabs that go with a file (a preview), by their file and kind
            let companion = |page: &String| {
                let kind = state.host.companion_kind(app, page)?;
                let file = beside.get(page).and_then(path)?;
                Some(Companion { file, kind })
            };
            let companions = pages.iter().filter_map(companion).collect();
            let front_companion = front.as_ref().and_then(companion);
            (root, Layout { files: pages.iter().filter_map(path).collect(), front: front_file, companions, front_companion })
        })
        .collect();
    state.host.save_layouts(app, layouts);
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
    state.host.save_windows(app, windows);
}

// --- the pages' commands, registered by the app: `papers_tabs::commands::tab_strip` and so on

pub mod commands {
    use tauri::{AppHandle, Emitter, EventTarget, Manager, Webview};

    use super::{Sidebar, Tab, Tabs, host, pages_of_set, project_of_page, title_of, with_tabs};

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
                path: host(app).path(app, &label).map(|p| p.display().to_string()),
                edited: host(app).edited(app, &label),
                label,
            })
            .collect()
    }

    /// A tab chosen.
    #[tauri::command]
    pub fn select_tab(app: AppHandle, label: String) {
        super::select(&app, &label);
    }

    /// A tab's close button.
    #[tauri::command]
    pub fn close_tab(app: AppHandle, label: String) {
        super::close(&app, &label);
    }

    /// A tab dragged to place `to` in the strip.
    #[tauri::command]
    pub fn move_tab(app: AppHandle, label: String, to: usize) {
        super::move_tab(&app, &label, to);
    }

    /// The sidebar of the asking page's project.
    #[tauri::command]
    pub fn sidebar(page: Webview) -> Sidebar {
        let app = page.app_handle();
        let root = project_of_page(app, page.label());
        let mut sidebar = Sidebar::default();
        host(app).with_sidebars(app, &mut |s| sidebar = s.of(root.as_deref()));
        sidebar
    }

    /// The sidebar of the asking page's project changed: kept, and the project's other pages told
    /// (`sidebar-changed`).
    #[tauri::command]
    pub fn set_sidebar(page: Webview, sidebar: Sidebar) {
        let app = page.app_handle();
        let root = project_of_page(app, page.label());
        host(app).with_sidebars(app, &mut |s| s.set(root.as_deref(), sidebar.clone()));
        for other in pages_of_set(app, root.as_deref()) {
            if other != page.label() {
                let _ = app.emit_to(EventTarget::webview(&other), "sidebar-changed", &sidebar);
            }
        }
    }

    /// ⌃Tab (`by` 1) or ⌃⇧Tab (-1) in a page: Switch to Recent Tab. GTK takes no Tab as a menu's key, so the page
    /// says it.
    #[tauri::command]
    pub fn recent_tab(app: AppHandle, by: isize) {
        super::recent(&app, by);
    }

    /// ⌃ let go in a page: Switch to Recent Tab over.
    #[tauri::command]
    pub fn recent_done(app: AppHandle) {
        super::recent_done(&app);
    }

    /// The asking page is drawn: it may be shown (`ready`).
    #[tauri::command]
    pub fn page_ready(page: Webview) {
        eprintln!("[papers-write {}] page {} drawn", super::stamp(), page.label());
        super::ready(page.app_handle(), page.label());
    }

    /// Whether the asking page is the tab in front of its window (else it is told `tab-shown` when it comes).
    #[tauri::command]
    pub fn page_in_front(page: Webview) -> bool {
        let app = page.app_handle();
        let window = page.window().label().to_string();
        super::front_of(app, &window).as_deref() == Some(page.label())
    }

    /// The folders open in the file tree of the asking page's project.
    #[tauri::command]
    pub fn folders(page: Webview) -> Vec<String> {
        let app = page.app_handle();
        let root = project_of_page(app, page.label());
        app.state::<Tabs>().folders.lock().unwrap().get(&root).cloned().unwrap_or_default()
    }

    /// The folders open in the file tree of the asking page's project changed: the project's other pages told
    /// (`folders-changed`).
    #[tauri::command]
    pub fn set_folders(page: Webview, folders: Vec<String>) {
        let app = page.app_handle();
        let root = project_of_page(app, page.label());
        app.state::<Tabs>().folders.lock().unwrap().insert(root.clone(), folders.clone());
        for other in pages_of_set(app, root.as_deref()) {
            if other != page.label() {
                let _ = app.emit_to(EventTarget::webview(&other), "folders-changed", &folders);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_shown_first_takes_the_last_sidebar_changed_and_then_keeps_its_own() {
        let (a, b) = (Path::new("/a"), Path::new("/b"));
        let outline = Sidebar { tab: Some("outline".into()), ..Sidebar::default() };
        let files = Sidebar { tab: Some("files".into()), ..Sidebar::default() };
        let mut s = Sidebars::default();
        s.set(Some(a), outline.clone());
        assert_eq!(s.of(Some(b)), outline);
        s.set(Some(a), files.clone());
        assert_eq!(s.of(Some(b)), outline);
        assert_eq!(s.of(Some(a)), files);
        assert_eq!(s.of(None), files);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod memory_tests {
    #[test]
    fn the_memory_of_the_process_is_measured() {
        let used = super::memory_used();
        // a test process: more than a megabyte, less than its whole machine
        assert!(used > 1 << 20 && used < 1 << 40, "{used}");
    }
}
