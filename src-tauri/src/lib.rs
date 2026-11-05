//! papers-write: a text editor first. A window per document, as macOS tabs; any file opens at once.

mod documents;
mod files;

use tauri::{Manager, RunEvent, WindowEvent};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(documents::Documents::default());
            app.manage(files::Watched::default());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Destroyed = event {
                documents::closed(window.app_handle(), window.label());
            }
        })
        .invoke_handler(tauri::generate_handler![documents::document, files::read_text, files::write_text])
        .build(tauri::generate_context!())
        .expect("error while building papers-write")
        .run(|app, event| {
            if let RunEvent::Ready = event {
                // nothing else was opened: a window of untitled text
                if app.webview_windows().is_empty()
                    && let Err(e) = documents::make(app, None)
                {
                    eprintln!("{e}");
                }
            }
        });
}
