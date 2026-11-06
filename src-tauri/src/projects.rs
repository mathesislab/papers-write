//! The projects (`write_core::project`): File ▸ Open Project… (a folder: with `write.toml` it joins the project bar
//! and its main document opens; without, the window in front asks for what to write in one), the project bar of a
//! project's documents (a tile per project, the main document's words and its open suggestions), Project Info….
//! The bar and the recent projects are kept in the app's data folder (`state.rs`). A change to them or to a
//! `write.toml` goes to every window as `projects-changed`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, Manager, Webview};
use crate::sheet::{self, Level};
use tauri_plugin_dialog::DialogExt;
use write_core::colour::Colour;
use write_core::project::{self, Manifest, Project};
use write_core::review::{Kind, Review, State};

use crate::documents;
use crate::state::{Layout, Saved, SavedWindow, Sidebars};

#[derive(Default)]
pub struct Projects {
    /// what the app keeps between sessions; read when first needed
    saved: Mutex<Option<Saved>>,
    /// the folders without a `write.toml` opened as projects, by the label of the window that asks for their card
    asking: Mutex<HashMap<String, PathBuf>>,
}

/// A project of the bar: the tile of `ProjectBar` (`papers-write/project`).
#[derive(Serialize)]
pub struct Tile {
    id: String,
    name: String,
    short: Option<String>,
    colour: Colour,
    current: bool,
    /// the main document's words and its open suggestions
    detail: Option<String>,
    missing: bool,
}

/// The project of a window's document.
#[derive(Serialize)]
pub struct Current {
    root: String,
    name: String,
    colour: Colour,
}

/// The project bar of a window: the project of its document (none: no bar) and the tiles.
#[derive(Serialize)]
pub struct Bar {
    current: Option<Current>,
    tiles: Vec<Tile>,
}

/// What the card of a project shows: the folder, what `write.toml` says (for a new one, what it will say to begin
/// with), and the documents to choose the main one from.
#[derive(Serialize)]
pub struct Card {
    root: String,
    /// the folder has a `write.toml`
    defined: bool,
    manifest: Manifest,
    /// the Markdown and LaTeX files of the folder, relative to it
    documents: Vec<String>,
}

fn data_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok()
}

/// What the app keeps, changed by `f` and written back when it changed.
fn with_saved<T>(app: &AppHandle, f: impl FnOnce(&mut Saved) -> T) -> T {
    let dir = data_dir(app);
    let state = app.state::<Projects>();
    let mut slot = state.saved.lock().unwrap();
    let saved = slot.get_or_insert_with(|| dir.as_deref().map(Saved::load).unwrap_or_default());
    let before = saved.clone();
    let out = f(saved);
    if *saved != before
        && let Some(dir) = &dir
    {
        saved.save(dir);
    }
    let recent = (saved.for_menu() != before.for_menu()).then(|| saved.for_menu().to_vec());
    drop(slot);
    // Open Recent follows the list (not with the lock held: the menu does not ask for it)
    if let Some(recent) = recent {
        crate::menu::refill_recent(app, &recent);
    }
    out
}

/// A project's tabs as last saved.
pub fn layout(app: &AppHandle, root: &Path) -> Layout {
    with_saved(app, |saved| saved.layouts.get(root).cloned().unwrap_or_default())
}

/// The projects' tabs as they are now kept (a project without files, untitled tabs only, keeps what it had).
pub fn save_layouts(app: &AppHandle, layouts: Vec<(PathBuf, Layout)>) {
    with_saved(app, |saved| {
        for (root, layout) in layouts {
            if !layout.files.is_empty() {
                saved.layouts.insert(root, layout);
            }
        }
    });
}

/// The windows to open again at the start.
pub fn saved_windows(app: &AppHandle) -> Vec<SavedWindow> {
    with_saved(app, |saved| saved.windows.clone())
}

/// The windows open now, kept to open again at the next start.
pub fn save_windows(app: &AppHandle, windows: Vec<SavedWindow>) {
    with_saved(app, |saved| saved.windows = windows);
}

/// The projects' sidebars, read or changed by `f` (papers-tabs), and saved when changed.
pub fn with_sidebars(app: &AppHandle, f: &mut dyn FnMut(&mut Sidebars)) {
    with_saved(app, |saved| f(&mut saved.sidebars));
}

/// The projects of File ▸ Open Recent, as the app kept them.
pub fn recent(app: &AppHandle) -> Vec<PathBuf> {
    with_saved(app, |saved| saved.for_menu().to_vec())
}

/// Open Recent ▸ Clear Menu.
pub fn clear_recent(app: &AppHandle) {
    with_saved(app, Saved::clear_recent);
}

fn changed(app: &AppHandle) {
    let _ = app.emit("projects-changed", ());
}

fn s(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The folder's name.
fn folder_name(root: &Path) -> String {
    root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| s(root))
}

/// "6.2k words · 3 open": the words of the main document and its open suggestions; none without a main document.
fn detail(project: &Project) -> Option<String> {
    let main = project.main()?;
    let text = std::fs::read_to_string(&main).ok()?;
    let words = project::words(&text);
    let mut detail = format!("{} word{}", project::short_count(words), if words == 1 { "" } else { "s" });
    let open = Review::load(&main)
        .map(|r| r.items(&text).iter().filter(|i| i.kind == Kind::Suggestion && i.state == State::Open).count())
        .unwrap_or(0);
    if open > 0 {
        detail += &format!("\n{open} open");
    }
    Some(detail)
}

fn tile(root: &Path, current: bool) -> Tile {
    match Project::read(root) {
        Ok(project) => Tile {
            id: s(root),
            name: project.name(),
            short: project.manifest.short.clone(),
            colour: project.manifest.colour.unwrap_or(Colour::None),
            current,
            detail: detail(&project),
            missing: false,
        },
        Err(_) => Tile {
            id: s(root),
            name: folder_name(root),
            short: None,
            colour: Colour::None,
            current,
            detail: None,
            missing: true,
        },
    }
}

/// The project bar of a tab: its project (papers-tabs: a text's, a PDF's, a history's), and the tiles of the bar
/// (that project's at the end when it is not in the bar).
#[tauri::command]
pub async fn project_bar(app: AppHandle, window: Webview) -> Bar {
    let project = crate::tabs::project_of_page(&app, window.label()).and_then(|root| Project::read(&root).ok());
    let Some(project) = project else {
        return Bar { current: None, tiles: Vec::new() };
    };
    let mut roots = with_saved(&app, |saved| saved.bar.clone());
    if !roots.contains(&project.root) {
        roots.push(project.root.clone());
    }
    Bar {
        current: Some(Current {
            root: s(&project.root),
            name: project.name(),
            colour: project.manifest.colour.unwrap_or(Colour::None),
        }),
        tiles: roots.iter().map(|root| tile(root, *root == project.root)).collect(),
    }
}

/// A tile clicked: the window that shows the project brought forward, else the window in front switched to it
/// (tabs.rs: its tabs as left, or as saved, else its main document).
#[tauri::command]
pub fn switch_project(app: AppHandle, root: String) -> Result<(), String> {
    let project = Project::read(Path::new(&root)).map_err(|e| e.to_string())?;
    with_saved(&app, |saved| saved.opened(&project.root));
    crate::tabs::show_project(&app, &project.root, crate::tabs::Target::Front)
}

/// A tile's Open in New Window: the project in a window of its own (the one that shows it, if one does).
#[tauri::command]
pub fn open_project_in_new_window(app: AppHandle, root: String) -> Result<(), String> {
    let project = Project::read(Path::new(&root)).map_err(|e| e.to_string())?;
    with_saved(&app, |saved| saved.opened(&project.root));
    crate::tabs::show_project(&app, &project.root, crate::tabs::Target::New)
}

#[tauri::command]
pub fn remove_from_bar(app: AppHandle, root: String) {
    with_saved(&app, |saved| saved.removed(Path::new(&root)));
    changed(&app);
}

/// The colours of the projects of the bar.
fn in_use(app: &AppHandle) -> Vec<Colour> {
    with_saved(app, |saved| saved.bar.clone())
        .iter()
        .filter_map(|root| Project::read(root).ok()?.manifest.colour)
        .collect()
}

/// What the card of the project of `root` shows; a folder without `write.toml` gets the next colour.
#[tauri::command]
pub fn project_card(app: AppHandle, root: String) -> Result<Card, String> {
    let root = PathBuf::from(root);
    let (defined, manifest) = match Project::read(&root) {
        Ok(project) => (true, project.manifest),
        Err(_) if !root.join(project::MANIFEST).exists() && root.is_dir() => (
            false,
            Manifest { colour: Some(Colour::next(&in_use(&app))), ..Manifest::default() },
        ),
        Err(e) => return Err(e.to_string()),
    };
    Ok(Card {
        root: s(&root),
        defined,
        manifest,
        documents: project::documents(&root).iter().map(|p| s(p)).collect(),
    })
}

/// The project info, written (only the keys that changed).
#[tauri::command]
pub fn set_project_info(app: AppHandle, root: String, manifest: Manifest) -> Result<(), String> {
    let mut project = Project::read(Path::new(&root)).map_err(|e| e.to_string())?;
    project.set(&manifest).map_err(|e| e.to_string())?;
    changed(&app);
    Ok(())
}

/// The folder whose card the window asks for: a folder opened as a project that has no `write.toml`.
#[tauri::command]
pub fn asking_project(window: Webview) -> Option<String> {
    window.state::<Projects>().asking.lock().unwrap().get(window.label()).map(|p| s(p))
}

/// The card of a new project cancelled: nothing is written.
#[tauri::command]
pub fn cancel_project(window: Webview) {
    window.state::<Projects>().asking.lock().unwrap().remove(window.label());
}

/// The new project of the card made: with `paper`, its main document first, from the paper-draft skeleton (never
/// over a file); then `write.toml`. It joins the bar and its main document opens.
#[tauri::command]
pub fn create_project(app: AppHandle, window: Webview, root: String, manifest: Manifest, paper: bool) -> Result<(), String> {
    let root = PathBuf::from(root);
    let main = manifest.main.first().ok_or("Choose the main document")?.clone();
    let written = if paper {
        let name = main.to_string_lossy();
        let title = manifest.name.clone().unwrap_or_else(|| folder_name(&root));
        let author = crate::review::person_in(&root).name;
        let today = write_core::review::text::now()[..10].to_string();
        Some(project::new_paper(&root, &name, &title, Some(&author), &today).map_err(|e| e.to_string())?)
    } else {
        let path = root.join(&main);
        if !path.is_file() || !project::is_document(&path) {
            return Err(format!("{}: not a Markdown or LaTeX file of the folder", main.display()));
        }
        None
    };
    if let Err(e) = Project::create(&root, &manifest) {
        if let Some(paper) = written {
            let _ = std::fs::remove_file(paper);
        }
        return Err(e.to_string());
    }
    window.state::<Projects>().asking.lock().unwrap().remove(window.label());
    with_saved(&app, |saved| saved.opened(&root));
    changed(&app);
    documents::open_file(&app, &root.join(main))
}

/// A folder opened as a project: with `write.toml`, in the bar and its main document opened; without, the window
/// of text in front (a new one when there is none: a PDF's window has no card) asks for what to write in one.
pub fn open(app: &AppHandle, root: &Path) -> Result<(), String> {
    let root = root.canonicalize().map_err(|e| format!("{}: {e}", root.display()))?;
    if !root.is_dir() {
        return Err(format!("{}: not a folder", root.display()));
    }
    if root.join(project::MANIFEST).is_file() {
        let project = Project::read(&root).map_err(|e| e.to_string())?;
        with_saved(app, |saved| saved.opened(&root));
        changed(app);
        return crate::tabs::show_project(app, &project.root, crate::tabs::Target::Front);
    }
    let text = documents::front(app).filter(|w| !documents::path_of(app, w.label()).is_some_and(|p| documents::is_pdf(&p)));
    let label = match text {
        Some(window) => window.label().to_string(),
        None => documents::make(app, None).map_err(|e| e.to_string())?,
    };
    app.state::<Projects>().asking.lock().unwrap().insert(label.clone(), root);
    let _ = app.emit_to(EventTarget::webview(&label), "asking-project", ());
    crate::tabs::select(app, &label);
    Ok(())
}

/// A project of Open Recent opened as Open Project… opens one; what went wrong is told in a dialog.
pub fn open_reporting(app: &AppHandle, root: &Path) {
    if let Err(e) = open(app, root) {
        sheet::tell(app, None, "Open Project", e, Level::Error);
    }
}

/// The + of the project bar: Open Project….
#[tauri::command]
pub fn choose_project(app: AppHandle) {
    choose(&app);
}

/// Open Project…: the folder to choose.
pub fn choose(app: &AppHandle) {
    // from the menu bar, with no window: the dialog is in front only for an app with a Dock icon
    documents::regular(app, true);
    let handle = app.clone();
    app.dialog().file().set_title("Open Project").pick_folder(move |folder| {
        let Some(root) = folder.and_then(|f| f.into_path().ok()) else { return };
        open_reporting(&handle, &root);
    });
}

/// The library of a project chosen, a `.bib` file, by its path relative to the project's folder when it is inside it;
/// none when the dialog was cancelled.
#[tauri::command]
pub async fn choose_library(app: AppHandle, root: String) -> Option<String> {
    let path = app
        .dialog()
        .file()
        .set_title("Library")
        .set_directory(&root)
        .add_filter("BibTeX", &["bib"])
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok())?;
    Some(path.strip_prefix(&root).map(s).unwrap_or_else(|_| s(&path)))
}
