//! papers-write: a text editor first. A window per document, as macOS tabs; any file opens at once.

mod build;
mod documents;
mod drafts;
mod exchange;
mod files;
mod git;
mod group;
mod hover;
mod menu;
mod notes;
mod package;
mod panetabs;
mod pdfsync;
mod preview;
mod profiles;
mod projects;
mod readaloud;
mod refs;
mod review;
mod settings;
mod sheet;
mod sidepdf;
mod sidebar;
mod spelling;
mod state;
mod tabs;
mod undo;
mod trash;
mod update;
mod watch;

use tauri::{AppHandle, DragDropEvent, Manager, RunEvent, State, WebviewEvent, WindowEvent};
use update::{Available, Installed, Updates};

/// Look for an update now.
#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<Available>, String> {
    update::check(&app).await
}

/// The update found by the last look, if any.
#[tauri::command]
fn found_update(updates: State<'_, Updates>) -> Option<Available> {
    updates.available()
}

/// The update installed just before this start, if any.
#[tauri::command]
fn installed_update(updates: State<'_, Updates>) -> Option<Installed> {
    updates.installed()
}

/// Install the update found, and restart.
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    update::install(&app).await
}

pub fn run() {
    eprintln!("[papers-write {}] start", papers_tabs::stamp());
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_pdfkit::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // managed before the app launches: macOS may hand over files to open before `setup` has run
        .manage(panetabs::PaneTabs::default())
        .manage(documents::Documents::default())
        .manage(files::Watched::default())
        .manage(build::Builds::default())
        .manage(profiles::Watches::default())
        .manage(refs::Loaded::default())
        .manage(preview::Runs::default())
        .manage(review::Reviews::default())
        .manage(git::Git::default())
        .manage(projects::Projects::default())
        .manage(spelling::Tags::default())
        .manage(tabs::Tabs::new(tabs::Write))
        .manage(sidebar::GoTos::default())
        .setup(|app| {
            settings::init(app.handle());
            // elsewhere than on macOS the files to open are the arguments of the command
            #[cfg(not(target_os = "macos"))]
            documents::opened(
                app.handle(),
                std::env::args_os().skip(1).filter_map(|a| std::fs::canonicalize(a).ok()).collect(),
            );
            app.manage(Updates::new(app.handle()));
            app.set_menu(menu::build(app.handle())?)?;
            app.on_menu_event(menu::on_event);
            update::watch(app.handle());
            // after an update, the Settings window tells of it
            if app.state::<Updates>().installed().is_some() {
                settings::show(app.handle(), Some("updates"));
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => documents::closing(window, api),
            WindowEvent::Focused(true) => tabs::window_focused(window.app_handle(), window.label()),
            WindowEvent::Destroyed => tabs::window_closed(window.app_handle(), window.label()),
            // the system's appearance changed: the windows' background, as Settings ▸ Window ▸ Appearance System has it
            WindowEvent::ThemeChanged(_) => tabs::repaint(window.app_handle()),
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
            undo::history_read,
            undo::history_write,
            panetabs::pdf_tab_show,
            panetabs::take_show_source,
            panetabs::preview_tab,
            panetabs::preview_tab_of,
            panetabs::tab_in_front,
            panetabs::pdf_tab_back,
            panetabs::relay,
            panetabs::bring_tab,
            panetabs::page_path,
            menu::zoom_key,
            projects::project_list,
            preview::document_outline,
            preview::chapter_neighbours,
            preview::prepare_preview,
            preview::heading_place,
            settings::settings,
            readaloud::read_aloud,
            readaloud::stop_reading,
            readaloud::reading,
            settings::set_settings,
            settings::set_text_width,
            settings::zoom_window,
            settings::set_appearance,
            settings::tools_found,
            settings::choose_tool,
            documents::document,
            documents::set_blank,
            documents::set_modified,
            drafts::draft_read,
            drafts::draft_write,
            drafts::draft_drop,
            documents::save_untitled,
            tabs::commands::tab_strip,
            tabs::commands::select_tab,
            tabs::commands::close_tab,
            tabs::commands::move_tab,
            tabs::commands::sidebar,
            tabs::commands::set_sidebar,
            tabs::commands::folders,
            tabs::commands::page_ready,
            tabs::commands::page_in_front,
            tabs::commands::recent_done,
            tabs::commands::recent_tab,
            tabs::commands::set_folders,
            files::read_text,
            files::read_pdf,
            files::write_text,
            documents::close_project_tabs,
            documents::print_page,
            preview::preview,
            hover::render_math,
            hover::image_data,
            hover::figure_image,
            hover::keep_figure,
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
            exchange::export_critic,
            exchange::choose_review_file,
            exchange::import_review,
            package::send_for_review,
            package::send_back,
            package::review_package,
            refs::references,
            refs::closest_references,
            refs::reference_sources,
            refs::references_left_out,
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
            review::review_store,
            review::review_items,
            review::review_suggest,
            review::review_withdraw,
            review::review_comment,
            review::review_reply,
            review::review_decide,
            review::review_counter,
            review::review_mode,
            review::review_off,
            review::review_view,
            review::review_confirm,
            review::review_received,
            review::review_cleanup_plan,
            review::review_cleanup,
            projects::project_bar,
            notes::notes_of,
            notes::search_notes,
            notes::new_note,
            notes::kept_by,
            projects::switch_project,
            projects::open_project_in_new_window,
            projects::remove_from_bar,
            projects::move_in_bar,
            projects::project_card,
            projects::set_project_info,
            projects::set_review_store,
            projects::asking_project,
            projects::cancel_project,
            projects::create_project,
            projects::choose_library,
            projects::choose_project,
            sidebar::project_files,
            sidebar::project_file_list,
            sidebar::search_project,
            sidebar::replace_in_project,
            sidebar::open_at,
            pdfsync::open_source_at,
            pdfsync::pdf_boxes,
            pdfsync::place_in_text,
            menu::view_shown,
            menu::sidebar_shown,
            sidepdf::side_pdf,
            profiles::build_profiles,
            profiles::set_build_profile,
            profiles::set_background,
            profiles::background_play,
            profiles::reveal_profiles,
            sidepdf::set_side_pdf,
            sidepdf::choose_side_pdf,
            sidepdf::hide_side_pdf,
            sidebar::pending_go_to,
            sidebar::new_markdown_file,
            sidebar::trash_path,
            check_update,
            found_update,
            installed_update,
            install_update
        ])
        .build(tauri::generate_context!())
        .expect("error while building papers-write")
        .run(|app, event| match event {
            // the files handed over while launching, or else a window of untitled text; files that come after this
            // take the blank window
            RunEvent::Ready => {
                eprintln!("[papers-write {}] ready", papers_tabs::stamp());
                documents::ready(app);
                package::sweep(app);
            }
            // a quit asked by the system (the Dock icon's menu): with untitled text never saved, it asks first
            RunEvent::ExitRequested {
                code: None, api, ..
            } if documents::confirm_quit(app) => {
                api.prevent_exit();
            }
            // the last window closed: the app stays in the Dock; Quit (⌘Q, the Dock icon's menu) ends it with a code
            RunEvent::ExitRequested {
                code: None, api, ..
            } if app.windows().is_empty() => {
                api.prevent_exit();
            }
            // quitting: the windows close now, but stay saved as open for the next start
            RunEvent::ExitRequested { code, .. } => {
                eprintln!("[papers-write] exit requested (code {code:?}, {} windows)", app.windows().len());
                tabs::quitting(app)
            }
            // the commands of watch profiles stop with the app
            RunEvent::Exit => {
                eprintln!("[papers-write] exit");
                profiles::stop_all(app)
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
