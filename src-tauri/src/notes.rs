//! A folder of notes (`write_core::notes`, a project of kind `notes`): its search, which the page shows over its text
//! (Find in Project, ⇧⌘F, in such a project, and on a page of no file there), and its new notes.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager, Webview};
use write_core::notes::{self, Note};
use write_core::project::{Kind, Project};

/// The folder of notes the page is of: its folder and name; none for a page of a paper, or of no project.
#[derive(Serialize)]
pub struct Notes {
    root: String,
    name: String,
}

fn root_of(app: &AppHandle, label: &str) -> Option<PathBuf> {
    let root = crate::tabs::project_of_page(app, label)?;
    let project = Project::read(&root).ok()?;
    (project.manifest.kind == Some(Kind::Notes)).then_some(root)
}

/// Whether the page is of a folder of notes, and which.
#[tauri::command]
pub fn notes_of(window: Webview) -> Option<Notes> {
    let root = root_of(window.app_handle(), window.label())?;
    let name = Project::read(&root).ok()?.name();
    Some(Notes { root: root.to_string_lossy().into_owned(), name })
}

/// The notes of the page's folder of notes matching `query`, the best first (all, the most recent first, without).
#[tauri::command]
pub async fn search_notes(window: Webview, query: String) -> Result<Vec<Note>, String> {
    let root = root_of(window.app_handle(), window.label()).ok_or("Not a folder of notes")?;
    tauri::async_runtime::spawn_blocking(move || notes::search(&root, &query)).await.map_err(|e| e.to_string())
}

/// A new note titled `title` in the page's folder of notes, opened.
#[tauri::command]
pub fn new_note(app: AppHandle, window: Webview, title: String) -> Result<(), String> {
    let root = root_of(&app, window.label()).ok_or("Not a folder of notes")?;
    let path = notes::create(&root, &title).map_err(|e| e.to_string())?;
    crate::documents::open_file(&app, &path)
}

/// Which notes app keeps the folder `root`, if one does (the new project card offers a folder of notes then).
#[tauri::command]
pub fn kept_by(root: String) -> Option<&'static str> {
    notes::kept_by(std::path::Path::new(&root))
}
