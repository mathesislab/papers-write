//! papers-write: a text editor first. A window per document, as macOS tabs; any file opens at once.

mod build;
mod documents;
mod exchange;
mod files;
mod menu;
mod preview;
mod refs;
mod review;
mod tabs;
mod tray;

use tauri::{DragDropEvent, Manager, RunEvent, WindowEvent};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_pdfkit::init())
        // managed before the app launches: macOS may hand over files to open before `setup` has run
        .manage(documents::Documents::default())
        .manage(files::Watched::default())
        .manage(build::Builds::default())
        .manage(refs::Loaded::default())
        .manage(preview::Runs::default())
        .manage(review::Reviews::default())
        .setup(|app| {
            app.set_menu(menu::build(app.handle())?)?;
            app.on_menu_event(menu::on_event);
            tray::build(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => documents::closing(window, api),
            WindowEvent::Focused(true) => documents::focused(window.app_handle(), window.label()),
            WindowEvent::Destroyed => documents::closed(window.app_handle(), window.label()),
            // files dropped on a window open in tabs of their own
            WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
                documents::open_files(window.app_handle(), paths);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            documents::document,
            documents::set_blank,
            documents::save_untitled,
            files::read_text,
            files::write_text,
            preview::preview,
            build::build,
            build::marked_up,
            build::revisions,
            build::earlier_file,
            build::reveal,
            build::open_document,
            exchange::compare_as_suggestions,
            exchange::revision_author,
            exchange::choose_edited_copy,
            exchange::export_word,
            exchange::choose_word_file,
            exchange::import_word,
            refs::references,
            refs::closest_references,
            refs::reference_sources,
            refs::cite_key,
            refs::bib_check,
            refs::bibliography_files,
            review::review_me,
            review::review_items,
            review::review_suggest,
            review::review_withdraw,
            review::review_comment,
            review::review_reply,
            review::review_decide,
            review::review_counter,
            review::review_mode,
            review::review_confirm
        ])
        .build(tauri::generate_context!())
        .expect("error while building papers-write")
        .run(|app, event| match event {
            // the files handed over while launching, or else a window of untitled text; files that come after this
            // take the blank window
            RunEvent::Ready => documents::ready(app),
            // the last window closed: the app stays in the menu bar; Quit (⌘Q, the menu bar icon) ends it with a code
            RunEvent::ExitRequested {
                code: None, api, ..
            } if app.webview_windows().is_empty() => {
                api.prevent_exit();
            }
            // the Dock icon clicked, no window open
            #[cfg(target_os = "macos")]
            RunEvent::Reopen {
                has_visible_windows: false,
                ..
            } => {
                if let Err(e) = documents::make(app, None) {
                    eprintln!("{e}");
                }
            }
            // Open With, a file dropped on the Dock icon, a double click
            #[cfg(target_os = "macos")]
            RunEvent::Opened { urls } => {
                let paths: Vec<_> = urls.iter().filter_map(|u| u.to_file_path().ok()).collect();
                documents::opened(app, paths);
            }
            _ => {}
        });
}
