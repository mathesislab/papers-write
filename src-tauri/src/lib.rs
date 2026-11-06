//! papers-write: a text editor first. A window per document, as macOS tabs; any file opens at once.

mod build;
mod documents;
mod exchange;
mod files;
mod git;
mod hover;
mod menu;
mod package;
mod preview;
mod projects;
mod refs;
mod review;
mod settings;
mod sheet;
mod sidebar;
mod spelling;
mod state;
mod tabs;
mod trash;
mod tray;

use tauri::{DragDropEvent, Manager, RunEvent, WebviewEvent, WindowEvent};

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
        .manage(git::Git::default())
        .manage(projects::Projects::default())
        .manage(spelling::Tags::default())
        .manage(tabs::Tabs::default())
        .manage(sidebar::GoTos::default())
        .setup(|app| {
            settings::init(app.handle());
            app.set_menu(menu::build(app.handle())?)?;
            app.on_menu_event(menu::on_event);
            tray::build(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => documents::closing(window, api),
            WindowEvent::Focused(true) => tabs::window_focused(window.app_handle(), window.label()),
            WindowEvent::Destroyed => tabs::window_closed(window.app_handle(), window.label()),
            // files dropped on a window open in tabs of their own
            WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
                documents::open_files(window.app_handle(), paths);
            }
            _ => {}
        })
        // on a page of a window of tabs, files dropped come to the page
        .on_webview_event(|webview, event| {
            if let WebviewEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                documents::open_files(webview.app_handle(), paths);
            }
        })
        .invoke_handler(tauri::generate_handler![
            settings::settings,
            settings::set_settings,
            settings::tools_found,
            settings::choose_tool,
            documents::document,
            documents::set_blank,
            documents::save_untitled,
            tabs::tab_strip,
            tabs::select_tab,
            tabs::close_tab,
            files::read_text,
            files::write_text,
            preview::preview,
            hover::render_math,
            hover::image_data,
            build::build,
            build::marked_up,
            build::revisions,
            build::earlier_file,
            build::reveal,
            build::open_document,
            git::git_state,
            git::git_fetch,
            git::git_changes,
            git::git_commit,
            git::git_pull,
            git::git_push,
            git::compare_with_commit,
            git::review_branches,
            git::recent_commits,
            git::review_commits,
            git::history,
            exchange::compare_as_suggestions,
            exchange::choose_edited_copy,
            exchange::export_word,
            exchange::choose_word_file,
            exchange::import_word,
            package::send_for_review,
            package::send_back,
            package::review_package,
            refs::references,
            refs::closest_references,
            refs::reference_sources,
            refs::cite_key,
            refs::bib_check,
            refs::cite_options,
            refs::reference_of,
            refs::missing_keys,
            refs::document_labels,
            spelling::check_spelling,
            spelling::spelling_guesses,
            spelling::learn_word,
            spelling::ignore_word,
            review::review_me,
            review::review_items,
            review::review_suggest,
            review::review_withdraw,
            review::review_comment,
            review::review_reply,
            review::review_decide,
            review::review_counter,
            review::review_mode,
            review::review_view,
            review::review_confirm,
            review::review_received,
            review::review_cleanup_plan,
            review::review_cleanup,
            projects::project_bar,
            projects::switch_project,
            projects::open_project_in_new_window,
            projects::remove_from_bar,
            projects::project_card,
            projects::set_project_info,
            projects::asking_project,
            projects::cancel_project,
            projects::create_project,
            projects::choose_library,
            projects::choose_project,
            sidebar::project_files,
            sidebar::search_project,
            sidebar::replace_in_project,
            sidebar::open_at,
            sidebar::pending_go_to,
            sidebar::new_markdown_file,
            sidebar::trash_path
        ])
        .build(tauri::generate_context!())
        .expect("error while building papers-write")
        .run(|app, event| match event {
            // the files handed over while launching, or else a window of untitled text; files that come after this
            // take the blank window
            RunEvent::Ready => {
                documents::ready(app);
                package::sweep(app);
            }
            // a quit asked by the system (the Dock icon's menu): with untitled text never saved, it asks first
            RunEvent::ExitRequested {
                code: None, api, ..
            } if documents::confirm_quit(app) => {
                api.prevent_exit();
            }
            // the last window closed: the app stays in the menu bar (unless the settings say it quits); Quit (⌘Q, the
            // menu bar icon) ends it with a code
            RunEvent::ExitRequested {
                code: None, api, ..
            } if app.windows().is_empty() && settings::get().menu_bar => {
                api.prevent_exit();
            }
            // quitting: the windows close now, but stay saved as open for the next start
            RunEvent::ExitRequested { .. } => tabs::quitting(app),
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
