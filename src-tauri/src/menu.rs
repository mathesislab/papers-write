//! The menu bar. New, Open…, Open Recent (the projects opened before), Open Project…, Save, Close, Build and PDF are the app's (Build to the page of the window in front, as `menu`; PDF to a PDF's page, as `pdf`); Undo, Redo, Symbols…, References… and Check Citations… go to the editor of the window in
//! front (event `edit`, the id of the item), File ▸ Exchange to its page (event `exchange`), Cite as [@key] to the list of references (event `cite-key`), Project Info… to its page (event `project-info`), and Save to its page (event `menu`).
//! The Review menu goes to the page of the window in front as `review-menu` with the id of the item, the Git menu as `git-menu`.

use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter, EventTarget, Manager, Wry};

use crate::documents;

/// File ▸ Open Recent, kept to be filled again when the list of recent projects changes.
pub struct RecentMenu(Submenu<Wry>);

/// The id of the item of a recent project: the prefix and the folder's path.
const RECENT: &str = "recent:";

/// `submenu` made to list `recent` (a missing folder disabled), then a separator and Clear Menu.
fn fill_recent(app: &AppHandle, submenu: &Submenu<Wry>, recent: &[PathBuf]) -> tauri::Result<()> {
    for old in submenu.items()? {
        submenu.remove(&old)?;
    }
    for root in recent {
        let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| root.to_string_lossy().into_owned());
        let id = format!("{RECENT}{}", root.display());
        submenu.append(&MenuItem::with_id(app, id, name, root.exists(), None::<&str>)?)?;
    }
    submenu.append(&PredefinedMenuItem::separator(app)?)?;
    submenu.append(&MenuItem::with_id(app, "recent-clear", "Clear Menu", !recent.is_empty(), None::<&str>)?)
}

/// Open Recent made to list `recent`.
pub fn refill_recent(app: &AppHandle, recent: &[PathBuf]) {
    if let Some(menu) = app.try_state::<RecentMenu>()
        && let Err(e) = fill_recent(app, &menu.0, recent)
    {
        eprintln!("Open Recent: {e}");
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let item =
        |id: &str, text: &str, keys: Option<&str>| MenuItem::with_id(app, id, text, true, keys);
    let separator = || PredefinedMenuItem::separator(app);
    let app_menu = Submenu::with_items(
        app,
        "Papers Write",
        true,
        &[
            &PredefinedMenuItem::about(app, None, None)?,
            &separator()?,
            &item("settings", "Settings…", Some("CmdOrCtrl+,"))?,
            &separator()?,
            &PredefinedMenuItem::services(app, None)?,
            &separator()?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &separator()?,
            // not the predefined one: that asks the app to terminate, which is kept from ending it with the last window
            &item("quit", "Quit Papers Write", Some("CmdOrCtrl+Q"))?,
        ],
    )?;
    // a review that comes in or goes out without the app on the other side
    let exchange = Submenu::with_items(
        app,
        "Exchange",
        true,
        &[
            &item("send-review", "Send for Review…", None)?,
            &item("send-back", "Send Back…", None)?,
            &separator()?,
            &item("compare", "Compare as Suggestions…", None)?,
            &separator()?,
            &item("word-export", "Export as Word with Changes…", None)?,
            &item("word-import", "Import Word Review…", None)?,
        ],
    )?;
    let recent = Submenu::with_id(app, "open-recent", "Open Recent", true)?;
    fill_recent(app, &recent, &crate::projects::recent(app))?;
    app.manage(RecentMenu(recent.clone()));
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &item("new", "New", Some("CmdOrCtrl+N"))?,
            &item("open", "Open…", Some("CmdOrCtrl+O"))?,
            &recent,
            &item("open-project", "Open Project…", Some("Shift+CmdOrCtrl+O"))?,
            &separator()?,
            &item("save", "Save", Some("CmdOrCtrl+S"))?,
            &item("close", "Close", Some("CmdOrCtrl+W"))?,
            &separator()?,
            &item("project-info", "Project Info…", Some("CmdOrCtrl+I"))?,
            &separator()?,
            &exchange,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &item("undo", "Undo", Some("CmdOrCtrl+Z"))?,
            &item("redo", "Redo", Some("Shift+CmdOrCtrl+Z"))?,
            &separator()?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
            &separator()?,
            &item("symbols", "Symbols…", Some("CmdOrCtrl+/"))?,
            &separator()?,
            &item("references", "References…", Some("Shift+CmdOrCtrl+R"))?,
            // a menu item, not a key of the page: macOS takes ⌘. for Cancel before the page sees it
            &item("cite-key", "Cite as [@key]", Some("CmdOrCtrl+."))?,
            &item("check", "Check Citations…", None)?,
        ],
    )?;
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &item("preview", "Show Preview", Some("Shift+CmdOrCtrl+P"))?,
            &item("outline", "Show Outline", Some("Alt+CmdOrCtrl+O"))?,
        ],
    )?;
    let build = Submenu::with_items(
        app,
        "Build",
        true,
        &[
            &item("build", "Build", Some("CmdOrCtrl+B"))?,
            &item(
                "build-show",
                "Build and Show PDF",
                Some("Shift+CmdOrCtrl+B"),
            )?,
            &separator()?,
            &item("marked-up", "Marked-up PDF…", None)?,
        ],
    )?;
    // the PDF of the window in front (a window of a PDF; the other pages ignore these)
    let pdf = Submenu::with_items(
        app,
        "PDF",
        true,
        &[
            &item("zoom-in", "Zoom In", Some("CmdOrCtrl+="))?,
            &item("zoom-out", "Zoom Out", Some("CmdOrCtrl+-"))?,
            &item("zoom-fit", "Fit Width", Some("CmdOrCtrl+0"))?,
            &separator()?,
            &item("pdf-find", "Find…", Some("CmdOrCtrl+F"))?,
            &item("pdf-next", "Find Next", Some("CmdOrCtrl+G"))?,
            &item("pdf-previous", "Find Previous", Some("Shift+CmdOrCtrl+G"))?,
        ],
    )?;
    let review = Submenu::with_items(
        app,
        "Review",
        true,
        &[
            // a check mark the window in front sets (its mode; `review::focused`, `review_mode`), not the click
            &CheckMenuItem::with_id(app, "suggest", "Suggest Changes", true, false, Some("Shift+CmdOrCtrl+E"))?,
            &item("comment", "Add Comment…", Some("Alt+CmdOrCtrl+M"))?,
            &separator()?,
            &item("panel", "Show Review Panel", Some("Alt+CmdOrCtrl+R"))?,
            &separator()?,
            &CheckMenuItem::with_id(app, "view-markup", "All Markup", true, true, Some("Alt+CmdOrCtrl+1"))?,
            &CheckMenuItem::with_id(app, "view-accepted", "Accepted", true, false, Some("Alt+CmdOrCtrl+2"))?,
            &CheckMenuItem::with_id(app, "view-original", "Original", true, false, Some("Alt+CmdOrCtrl+3"))?,
            &separator()?,
            &item("accept-all", "Accept All", None)?,
            &item("reject-all", "Reject All", None)?,
            &separator()?,
            &item("next", "Next Change", Some("Alt+CmdOrCtrl+Down"))?,
            &item("previous", "Previous Change", Some("Alt+CmdOrCtrl+Up"))?,
            &separator()?,
            &item("cleanup", "Clean Up Review…", None)?,
        ],
    )?;
    let git = Submenu::with_items(
        app,
        "Git",
        true,
        &[
            &item("git-commit", "Commit…", Some("Alt+CmdOrCtrl+K"))?,
            &item("git-pull", "Pull", None)?,
            &item("git-push", "Push", None)?,
            &separator()?,
            &item("git-compare", "Compare with Commit…", None)?,
            &item("git-review-branch", "Review Branch…", None)?,
            &item("git-review-commits", "Review Commits…", None)?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
        ],
    )?;
    // macOS adds its own items to this menu: Show Tab Bar, Merge All Windows, the list of the windows
    #[cfg(target_os = "macos")]
    window.set_as_windows_menu_for_nsapp()?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &review, &git, &build, &pdf, &window])
}

pub fn on_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    match id {
        "new" => {
            if let Err(e) = documents::make(app, None) {
                eprintln!("{e}");
            }
        }
        "open" => documents::choose(app),
        "open-project" => crate::projects::choose(app),
        // the page of the window in front shows the card of its document's project
        "project-info" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "project-info", ());
            }
        }
        "recent-clear" => crate::projects::clear_recent(app),
        _ if id.starts_with(RECENT) => crate::projects::open_reporting(app, Path::new(&id[RECENT.len()..])),
        "settings" => crate::settings::show(app),
        "quit" => documents::quit(app),
        "close" => {
            if let Some(window) = documents::front(app) {
                let _ = window.close();
            }
        }
        // the page writes the document, then asks for the build
        "save" | "build" | "build-show" | "marked-up" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "menu", id);
            }
        }
        // the page of the window asks what is needed (the revisions, a file, a name) and the backend does the rest
        "send-review" | "send-back" | "compare" | "word-export" | "word-import" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "exchange", id);
            }
        }
        "cite-key" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "cite-key", ());
            }
        }
        "preview" | "outline" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), id, ());
            }
        }
        "zoom-in" | "zoom-out" | "zoom-fit" | "pdf-find" | "pdf-next" | "pdf-previous" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "pdf", id);
            }
        }
        "suggest" | "comment" | "panel" | "view-markup" | "view-accepted" | "view-original" | "accept-all" | "reject-all" | "next" | "previous" | "cleanup" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "review-menu", id);
            }
        }
        // the page writes the document first, asks what is needed, and the backend runs git
        "git-commit" | "git-pull" | "git-push" | "git-compare" | "git-review-branch" | "git-review-commits" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "git-menu", id);
            }
        }
        "undo" | "redo" | "symbols" | "references" | "check" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "edit", id);
            }
        }
        _ => {}
    }
}
