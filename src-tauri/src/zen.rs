//! View ▸ Full Screen (F11) and Distraction Free (Shift+F11), as Sublime's, outside macOS: the window in front over
//! the whole screen; Distraction Free also without its menu bar (its keys still work: GTK keeps them on the window),
//! and its pages without their tabs, bars, sidebar and foot (the class `zen` on their document: theme.css), the text
//! alone. The pages ask as they load (`distraction_free_of`) and are told as it changes (`distraction-free`).

use std::collections::HashSet;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Webview};

/// The windows in Distraction Free.
static ZEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn is_zen(window: &str) -> bool {
    ZEN.lock().unwrap().as_ref().is_some_and(|z| z.contains(window))
}

/// Full Screen: the window in front over the whole screen, or back (Distraction Free left too).
pub fn full_screen(app: &AppHandle) {
    let Some(window) = front(app) else { return };
    if is_zen(window.label()) {
        return set_zen(app, &window, false);
    }
    let _ = window.set_fullscreen(!window.is_fullscreen().unwrap_or(false));
}

/// Distraction Free: the window in front over the whole screen with the text alone, or back as it was.
pub fn distraction_free(app: &AppHandle) {
    let Some(window) = front(app) else { return };
    let on = !is_zen(window.label());
    set_zen(app, &window, on);
}

fn set_zen(app: &AppHandle, window: &tauri::Window, on: bool) {
    let label = window.label().to_string();
    {
        let mut zen = ZEN.lock().unwrap();
        let set = zen.get_or_insert_with(HashSet::new);
        if on {
            set.insert(label.clone());
        } else {
            set.remove(&label);
        }
    }
    let _ = if on { window.hide_menu() } else { window.show_menu() };
    let _ = window.set_fullscreen(on);
    let _ = app.emit("distraction-free", (label, on));
}

/// The window of the tab in front.
fn front(app: &AppHandle) -> Option<tauri::Window> {
    let page = crate::documents::front(app)?;
    Some(page.window())
}

/// Whether the page's window is in Distraction Free (a page loading in it).
#[tauri::command]
pub fn distraction_free_of(webview: Webview) -> bool {
    is_zen(webview.window().label())
}

/// A window closed: no longer in Distraction Free.
pub fn closed(window: &str) {
    if let Some(set) = ZEN.lock().unwrap().as_mut() {
        set.remove(window);
    }
}
