//! The menu bar. New, Open…, Save, Close, Build and PDF are the app's (Build to the page of the window in front, as `menu`; PDF to a PDF's page, as `pdf`); Undo, Redo, Symbols…, References… and Check Citations… go to the editor of the window in
//! front (event `edit`, the id of the item), File ▸ Exchange to its page (event `exchange`), Cite as [@key] to the list of references (event `cite-key`), and Save to its page (event `menu`).
//! The Review menu goes to the page of the window in front as `review-menu` with the id of the item, the Git menu as `git-menu`.

use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, EventTarget, Wry};

use crate::documents;

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
            &item("compare", "Compare as Suggestions…", None)?,
            &separator()?,
            &item("word-export", "Export as Word with Changes…", None)?,
            &item("word-import", "Import Word Review…", None)?,
        ],
    )?;
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &item("new", "New", Some("CmdOrCtrl+N"))?,
            &item("open", "Open…", Some("CmdOrCtrl+O"))?,
            &separator()?,
            &item("save", "Save", Some("CmdOrCtrl+S"))?,
            &item("close", "Close", Some("CmdOrCtrl+W"))?,
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
        &[&item("preview", "Show Preview", Some("Shift+CmdOrCtrl+P"))?],
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
            &item("accept-all", "Accept All", None)?,
            &item("reject-all", "Reject All", None)?,
            &separator()?,
            &item("next", "Next Change", Some("Alt+CmdOrCtrl+Down"))?,
            &item("previous", "Previous Change", Some("Alt+CmdOrCtrl+Up"))?,
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
        "quit" => app.exit(0),
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
        "compare" | "word-export" | "word-import" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "exchange", id);
            }
        }
        "cite-key" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "cite-key", ());
            }
        }
        "preview" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "preview", ());
            }
        }
        "zoom-in" | "zoom-out" | "zoom-fit" | "pdf-find" | "pdf-next" | "pdf-previous" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "pdf", id);
            }
        }
        "suggest" | "comment" | "panel" | "accept-all" | "reject-all" | "next" | "previous" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "review-menu", id);
            }
        }
        // the page writes the document first, asks what is needed, and the backend runs git
        "git-commit" | "git-pull" | "git-push" => {
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
