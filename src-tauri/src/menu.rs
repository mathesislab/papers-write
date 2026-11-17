//! The menu bar. New, Open…, Open Recent (the projects opened before), Open Project…, Save, Close, Build and PDF are the app's (Build to the page of the window in front, as `menu`; PDF to a PDF's page, as `pdf`); Undo, Redo, Symbols…, References… and Check Citations… go to the editor of the window in
//! front (event `edit`, the id of the item), File ▸ Exchange to its page (event `exchange`), Cite as [@key] to the list of references (event `cite-key`), Project Info… to its page (event `project-info`), and Save to its page (event `menu`).
//! The Review menu goes to the page of the window in front as `review-menu` with the id of the item, the Git menu as `git-menu`, the Format menu as `format` with the format (`bold`, `heading`, …).
//! Edit ▸ Find goes to the page in front as `find` (the text's bar, a PDF's search, the sidebar's search of the
//! project); View ▸ Show Preview, Show Sidebar and Show Outline as `preview`, `sidebar` and `outline`; the Go menu as
//! `go` (a PDF's Back, Forward and pages; in a text Back and Forward are ⌘[ and ⌘], indent less and more).

use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter, EventTarget, Manager, Wry};

use crate::documents;

/// The keys of macOS, else those of Linux and Windows.
const MAC: bool = cfg!(target_os = "macos");

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
            &item("check-updates", "Check for Updates…", None)?,
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
            &item("critic-export", "Export as CriticMarkup…", None)?,
            &item("review-import", "Import Review…", None)?,
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
            // as Sublime's Goto Anything: a file of the project, a heading (@), a line (:)
            &item("go-to-file", "Go to File…", Some("CmdOrCtrl+P"))?,
            &recent,
            &item("open-project", "Open Project…", Some("Shift+CmdOrCtrl+O"))?,
            &separator()?,
            &item("save", "Save", Some("CmdOrCtrl+S"))?,
            &item("close", "Close", Some("CmdOrCtrl+W"))?,
            &separator()?,
            // ⌘I is Italic
            &item("project-info", "Project Info…", Some("Shift+CmdOrCtrl+I"))?,
            &separator()?,
            &exchange,
        ],
    )?;
    // the window in front finds (its text, or its PDF); Find in Project… is the sidebar's search
    let find = Submenu::with_items(
        app,
        "Find",
        true,
        &[
            &item("find", "Find…", Some("CmdOrCtrl+F"))?,
            // outside macOS, the keys of the editors there (gedit, VS Code, Sublime): Ctrl+H Replace, F3 Find Next,
            // Ctrl+G Go to Line
            &item("find-replace", "Find and Replace…", Some(if MAC { "Alt+CmdOrCtrl+F" } else { "Ctrl+H" }))?,
            &item("find-next", "Find Next", Some(if MAC { "CmdOrCtrl+G" } else { "F3" }))?,
            &item("find-previous", "Find Previous", Some(if MAC { "Shift+CmdOrCtrl+G" } else { "Shift+F3" }))?,
            &item("find-selection", "Use Selection for Find", Some("CmdOrCtrl+E"))?,
            &separator()?,
            &item("find-project", "Find in Project…", Some("Shift+CmdOrCtrl+F"))?,
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
            &find,
            &separator()?,
            &item("go-to-line", "Go to Line…", Some("Ctrl+G"))?,
            &item("select-next", "Select Next Occurrence", Some("CmdOrCtrl+D"))?,
            // outside macOS Ctrl+/ comments, as in the editors there; Symbols moves aside
            &item("toggle-comment", "Toggle Comment", if MAC { None } else { Some("Ctrl+/") })?,
            &item("symbols", "Symbols…", Some(if MAC { "CmdOrCtrl+/" } else { "Alt+Ctrl+/" }))?,
            &separator()?,
            &item("references", "References…", Some("Shift+CmdOrCtrl+R"))?,
            // a menu item, not a key of the page: macOS takes ⌘. for Cancel before the page sees it
            &item("cite-key", "Cite as [@key]", Some("CmdOrCtrl+."))?,
            &item("check", "Check Citations…", None)?,
        ],
    )?;
    // the formats of the toolbar, at the selection of the text in front (Markdown, or LaTeX in a .tex file)
    let format = Submenu::with_items(
        app,
        "Format",
        true,
        &[
            &item("format-bold", "Bold", Some("CmdOrCtrl+B"))?,
            &item("format-italic", "Italic", Some("CmdOrCtrl+I"))?,
            &item("format-code", "Code", None)?,
            &item("format-link", "Link", Some("CmdOrCtrl+K"))?,
            &separator()?,
            &item("format-heading", "Heading", Some("Shift+CmdOrCtrl+H"))?,
            &item("format-bullet", "Bulleted List", Some("Shift+CmdOrCtrl+8"))?,
            &item("format-numbered", "Numbered List", Some("Shift+CmdOrCtrl+7"))?,
            &item("format-quote", "Quote", Some("Shift+CmdOrCtrl+9"))?,
        ],
    )?;
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &item("preview", "Show Preview", Some("Shift+CmdOrCtrl+P"))?,
            &item("pdf-beside", "Show PDF", Some("Alt+Shift+CmdOrCtrl+P"))?,
            // the text's preview and PDF beside it, or in tabs of their own (a small screen: panetabs.rs), or so only
            // while the window is narrow; the one chosen checked by the page (`view_shown`)
            &Submenu::with_items(
                app,
                "Panes",
                true,
                &[
                    &CheckMenuItem::with_id(app, "panes-side", "Beside the Text", true, true, None::<&str>)?,
                    &CheckMenuItem::with_id(app, "panes-tabs", "In Tabs of Their Own", true, false, None::<&str>)?,
                    &CheckMenuItem::with_id(app, "panes-auto", "In Tabs on a Narrow Window", true, false, None::<&str>)?,
                ],
            )?,
            &item("sidebar", "Show Sidebar", Some("Alt+CmdOrCtrl+S"))?,
            &item("outline", "Show Outline", Some("Alt+CmdOrCtrl+O"))?,
            &separator()?,
            &item("text-wider", "Text Width +", None)?,
            &item("text-narrower", "Text Width −", None)?,
            &item("text-full", "Full Width", None)?,
            &separator()?,
            // the keys of zooming, whatever the tab: a text's window zoomed, a PDF's pages (the PDF menu's, without
            // keys); ⌃+ itself where + is a key of its own (Italian, German…) is the page's (`zoom_key`)
            &item("zoom-in", "Zoom In", Some("CmdOrCtrl+="))?,
            &item("zoom-out", "Zoom Out", Some("CmdOrCtrl+-"))?,
            &item("zoom-fit", "Actual Size (a PDF: Fit Width)", Some("CmdOrCtrl+0"))?,
            &separator()?,
            &item("appearance-system", "System Appearance", None)?,
            &item("appearance-light", "Light Appearance", None)?,
            &item("appearance-dark", "Dark Appearance", None)?,
            &item("figures-inline", "Show Figures in the Text", None)?,
            // the project bar hidden at the window's left edge till the pointer comes there (Settings ▸ Window)
            &CheckMenuItem::with_id(
                app,
                "project-bar",
                "Show Project Bar on Hover",
                true,
                crate::settings::get().project_bar == crate::settings::ProjectBar::Hover,
                None::<&str>,
            )?,
        ],
    )?;
    // the PDF in front: back to the page a link or a jump left, and its pages (a text takes ⌘[ ⌘] as its indent)
    let go = Submenu::with_items(
        app,
        "Go",
        true,
        &[
            &item("back", "Back", Some("CmdOrCtrl+["))?,
            &item("forward", "Forward", Some("CmdOrCtrl+]"))?,
            &separator()?,
            &item("previous-page", "Previous Page", None)?,
            &item("next-page", "Next Page", None)?,
            &item("first-page", "First Page", None)?,
            &item("last-page", "Last Page", None)?,
            &separator()?,
            &item("go-to-page", "Go to Page…", Some("Alt+CmdOrCtrl+G"))?,
        ],
    )?;
    let build = Submenu::with_items(
        app,
        "Build",
        true,
        &[
            // ⌘B is Bold
            &item("build", "Build", Some("Shift+CmdOrCtrl+B"))?,
            &item(
                "build-show",
                "Build and Show PDF",
                Some("Alt+Shift+CmdOrCtrl+B"),
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
            &item("pdf-zoom-in", "Zoom In", None)?,
            &item("pdf-zoom-out", "Zoom Out", None)?,
            &item("pdf-zoom-fit", "Fit Width", None)?,
        ],
    )?;
    let review = Submenu::with_id_and_items(
        app,
        "review",
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
            &item("accept-here", "Accept Change", None)?,
            &item("reject-here", "Reject Change", None)?,
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
            &separator()?,
            // ⇧⌘[ ⇧⌘] as macOS's apps have them; Ctrl+PageUp and Ctrl+PageDown elsewhere, as browsers and editors do
            &item("previous-tab", "Show Previous Tab", Some(if cfg!(target_os = "macos") { "Shift+CmdOrCtrl+[" } else { "Ctrl+PageUp" }))?,
            &item("next-tab", "Show Next Tab", Some(if cfg!(target_os = "macos") { "Shift+CmdOrCtrl+]" } else { "Ctrl+PageDown" }))?,
            &item("move-tab-left", "Move Tab Left", Some("Shift+CmdOrCtrl+PageUp"))?,
            &item("move-tab-right", "Move Tab Right", Some("Shift+CmdOrCtrl+PageDown"))?,
        ],
    )?;
    // macOS adds its own items to this menu: Show Tab Bar, Merge All Windows, the list of the windows
    #[cfg(target_os = "macos")]
    window.set_as_windows_menu_for_nsapp()?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &format, &view, &go, &review, &git, &build, &pdf, &window])
}

pub fn on_event(app: &AppHandle, event: MenuEvent) {
    command(app, event.id().as_ref());
}

/// ⌃+ (⌘+) where + is a key of its own (an Italian or German keyboard): the menu's ⌃= is ⌃⇧0 there, so the page
/// says it (`step` 1; -1 and 0 the same for ⌃- and ⌃0, should a layout need them).
#[tauri::command]
pub fn zoom_key(app: AppHandle, step: i32) {
    command(&app, match step.signum() {
        1 => "zoom-in",
        -1 => "zoom-out",
        _ => "zoom-fit",
    });
}

/// What a menu item does, by its id.
fn command(app: &AppHandle, id: &str) {
    match id {
        "new" => {
            if let Err(e) = documents::make(app, None) {
                eprintln!("{e}");
            }
        }
        "open" => documents::choose(app),
        "project-bar" => crate::settings::toggle_project_bar(app),
        "open-project" => crate::projects::choose(app),
        // the page of the window in front shows the card of its document's project
        "project-info" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "project-info", ());
            }
        }
        "recent-clear" => crate::projects::clear_recent(app),
        _ if id.starts_with(RECENT) => crate::projects::open_reporting(app, Path::new(&id[RECENT.len()..])),
        "settings" => crate::settings::show(app, None),
        "check-updates" => crate::settings::show(app, Some("updates")),
        "quit" => documents::quit(app),
        "close" => crate::tabs::close_front(app),
        "previous-tab" => crate::tabs::step(app, -1),
        "next-tab" => crate::tabs::step(app, 1),
        "move-tab-left" => crate::tabs::shift(app, -1),
        "move-tab-right" => crate::tabs::shift(app, 1),
        // the page writes the document, then asks for the build
        "save" | "build" | "build-show" | "marked-up" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "menu", id);
            }
        }
        // the page of the window asks what is needed (the revisions, a file, a name) and the backend does the rest
        "send-review" | "send-back" | "compare" | "word-export" | "critic-export" | "review-import" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "exchange", id);
            }
        }
        "find" | "find-replace" | "find-next" | "find-previous" | "find-selection" | "find-project" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "find", id);
            }
        }
        "cite-key" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "cite-key", ());
            }
        }
        "panes-side" | "panes-tabs" | "panes-auto" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "panes", id.trim_start_matches("panes-"));
            }
        }
        "text-wider" => crate::settings::step_text_width(app, 1.0),
        "text-narrower" => crate::settings::step_text_width(app, -1.0),
        "text-full" => crate::settings::toggle_full_width(app),
        // the whole window, text and all (the text's own size is Settings ▸ Editor ▸ Size)
        // the PDF menu's: the pages of a PDF tab
        "pdf-zoom-in" | "pdf-zoom-out" | "pdf-zoom-fit" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "pdf", id.trim_start_matches("pdf-"));
            }
        }
        "appearance-system" => crate::settings::set_appearance(app.clone(), crate::settings::Appearance::System),
        "appearance-light" => crate::settings::set_appearance(app.clone(), crate::settings::Appearance::Light),
        "appearance-dark" => crate::settings::set_appearance(app.clone(), crate::settings::Appearance::Dark),
        "figures-inline" => crate::settings::toggle_inline_figures(app),
        "preview" | "pdf-beside" | "outline" | "sidebar" | "go-to-file" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), id, ());
            }
        }
        "back" | "forward" | "previous-page" | "next-page" | "first-page" | "last-page" | "go-to-page" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "go", id);
            }
        }
        // a PDF zoomed; a text's size changed instead (⌘= ⌘−)
        "zoom-in" | "zoom-out" | "zoom-fit" => {
            if let Some(window) = documents::front(app) {
                let pdf = documents::path_of(app, window.label()).is_some_and(|p| documents::is_pdf(&p));
                match (pdf, id) {
                    // a text: the window zoomed, all it shows (the text's own size is A− A+, View ▸ Text Size)
                    (false, "zoom-in") => crate::settings::step_zoom(app, 1.0),
                    (false, "zoom-out") => crate::settings::step_zoom(app, -1.0),
                    (false, "zoom-fit") => crate::settings::step_zoom(app, 0.0),
                    _ => {
                        let _ = app.emit_to(EventTarget::webview(window.label()), "pdf", id);
                    }
                }
            }
        }
        "suggest" | "comment" | "panel" | "view-markup" | "view-accepted" | "view-original" | "accept-here" | "reject-here" | "accept-all" | "reject-all" | "next" | "previous" | "cleanup" => {
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
        _ if id.starts_with("format-") => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "format", &id["format-".len()..]);
            }
        }
        "undo" | "redo" | "symbols" | "references" | "check" | "go-to-line" | "select-next" | "toggle-comment" => {
            if let Some(window) = documents::front(app) {
                let _ = app.emit_to(EventTarget::webview(window.label()), "edit", id);
            }
        }
        _ => {}
    }
}

/// An item of the menu bar, in whichever menu it is.
fn item_of(app: &AppHandle, id: &str) -> Option<MenuItem<Wry>> {
    let menu = app.menu()?;
    menu.items().ok()?.into_iter().find_map(|kind| kind.as_submenu().and_then(|s| s.get(id)).and_then(|i| i.as_menuitem().cloned()))
}

/// View ▸ Show Sidebar and Show Outline saying Hide while the page in front shows them (as it tells, when it changes and when its
/// window comes to the front); View ▸ Show Preview and Show PDF the same (`view_shown`).
#[tauri::command]
pub fn sidebar_shown(app: AppHandle, shown: bool, outline: bool) {
    for (id, on, name) in [("sidebar", shown, "Sidebar"), ("outline", outline, "Outline")] {
        if let Some(item) = item_of(&app, id) {
            let _ = item.set_text(format!("{} {name}", if on { "Hide" } else { "Show" }));
        }
    }
}

/// View ▸ Show Preview and Show PDF saying Hide when the page in front shows them.
#[tauri::command]
pub fn view_shown(app: AppHandle, preview: bool, pdf: bool, panes: String) {
    for (id, shown, name) in [("preview", preview, "Preview"), ("pdf-beside", pdf, "PDF")] {
        if let Some(item) = item_of(&app, id) {
            let _ = item.set_text(format!("{} {name}", if shown { "Hide" } else { "Show" }));
        }
    }
    for mode in ["side", "tabs", "auto"] {
        if let Some(item) = app.menu().and_then(|m| m.get(&format!("panes-{mode}"))).and_then(|k| k.as_check_menuitem().cloned()) {
            let _ = item.set_checked(panes == mode);
        }
    }
}
