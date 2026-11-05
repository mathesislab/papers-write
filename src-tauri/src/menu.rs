//! The menu bar. New, Open…, Save and Close are the app's; Undo, Redo, Symbols…, References… and Check Citations… go to the editor of the window in
//! front (event `edit`, the id of the item), Cite as [@key] to the list of references (event `cite-key`), and Save to its page (event `menu`).

use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
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
    Menu::with_items(app, &[&app_menu, &file, &edit, &window])
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
        "save" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "menu", id);
            }
        }
        "cite-key" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "cite-key", ());
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
