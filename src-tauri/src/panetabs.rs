//! The preview and the PDF of a text in tabs of their own (View ▸ Panes ▸ In Tabs of Their Own; a small screen),
//! rather than beside it. The PDF's tab is a PDF's (`pdf` page), brought forward or opened, with the place of the
//! text to show in it (`pdf_tab_show`, taken by the page: `take_show_source`). The preview's tab (`preview-N`,
//! the `preview` page) is drawn from the text's tab: the text's page sends it what pandoc made and the block of its
//! cursor, and the preview's sends back its clicks (`relay`, both ways).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, EventTarget, Manager, Webview};

use crate::documents;

/// A place of a text to show in its PDF: the line, how far along it the cursor is, the word and the words around it
/// (the map the build left beside the PDF), and phrases of the line to look for without one.
#[derive(Clone, Serialize, Deserialize)]
pub struct ShowSource {
    source: String,
    line: u32,
    along: f64,
    word: String,
    before: Vec<String>,
    after: Vec<String>,
    phrases: Vec<String>,
}

#[derive(Default)]
pub struct PaneTabs {
    /// the place each PDF's tab is to show, till it takes it (a tab just opened is not listening yet)
    shows: Mutex<HashMap<String, ShowSource>>,
    /// each text's preview tab
    previews: Mutex<HashMap<String, String>>,
    next: AtomicUsize,
}

/// The PDF of the asking text in a tab of its own, brought forward or opened; the place `show` shown in it.
#[tauri::command]
pub fn pdf_tab_show(window: Webview, show: Option<ShowSource>) -> Result<(), String> {
    let app = window.app_handle();
    let source = documents::path_of(app, window.label()).ok_or("Save the document first, to show its PDF")?;
    let pdf = crate::sidepdf::pdf_of(app, &source).ok_or("No PDF yet: Build first, or choose one with the PDF beside the text")?;
    let page = crate::tabs::page_of(app, &pdf);
    if let Some(show) = show {
        let state = app.state::<PaneTabs>();
        let key = pdf.to_string_lossy().into_owned();
        state.shows.lock().unwrap().insert(key, show);
    }
    match page {
        Some(page) => {
            crate::tabs::select(app, &page);
            let _ = app.emit_to(EventTarget::webview(&page), "show-source", ());
            Ok(())
        }
        None => documents::open_file(app, &pdf),
    }
}

/// The place a PDF's tab is to show, once (none: nothing asked).
#[tauri::command]
pub fn take_show_source(window: Webview) -> Option<ShowSource> {
    let app = window.app_handle();
    let pdf = documents::path_of(app, window.label())?;
    app.state::<PaneTabs>().shows.lock().unwrap().remove(pdf.to_string_lossy().as_ref())
}

/// The preview of the asking text in a tab of its own, beside it: brought forward when it is open, else made. Its
/// label.
#[tauri::command]
pub fn preview_tab(window: Webview) -> Result<String, String> {
    let app = window.app_handle();
    let text = window.label().to_string();
    let state = app.state::<PaneTabs>();
    let open = state.previews.lock().unwrap().get(&text).cloned().filter(|l| app.get_webview(l).is_some());
    if let Some(label) = open {
        crate::tabs::select(app, &label);
        return Ok(label);
    }
    let name = documents::path_of(app, &text).as_deref().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let label = format!("preview-{}", state.next.fetch_add(1, Ordering::Relaxed) + 1);
    state.previews.lock().unwrap().insert(text.clone(), label.clone());
    crate::tabs::add_beside(app, &label, &format!("/preview?of={text}"), &text, format!("Preview: {name}")).map_err(|e| e.to_string())?;
    Ok(label)
}

/// What a text's tab and its preview's tab tell each other: `event` with `payload` to the page `to`; whether it is
/// there.
#[tauri::command]
pub fn relay(app: AppHandle, to: String, event: String, payload: serde_json::Value) -> bool {
    if app.get_webview(&to).is_none() {
        return false;
    }
    app.emit_to(EventTarget::webview(&to), &event, payload).is_ok()
}

/// The tab of `page` brought forward (the text, from its preview's Go to Source).
#[tauri::command]
pub fn bring_tab(app: AppHandle, page: String) {
    crate::tabs::select(&app, &page);
}

/// The file of the page `page` (the preview's tab: its text's, for the project around it).
#[tauri::command]
pub fn page_path(app: AppHandle, page: String) -> Option<String> {
    documents::path_of(&app, &page).map(|p| p.to_string_lossy().into_owned())
}

/// A tab gone: a preview's forgotten.
pub fn closed(app: &AppHandle, page: &str) {
    app.state::<PaneTabs>().previews.lock().unwrap().retain(|text, preview| text != page && preview != page);
}
