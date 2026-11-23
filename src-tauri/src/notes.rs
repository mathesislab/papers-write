//! A folder of notes (`write_core::notes`, a project of kind `notes`): its search, which the page shows over its text
//! (Find in Project, ⇧⌘F, in such a project, and on a page of no file there), and its new notes.
//!
//! And a project's notes (View ▸ Show Notes, ⌥⌘N): one Markdown file of the project (`notes.md`, or the one write.toml
//! names: `notes`), shown as the preview and the PDF are, beside the text or in a tab of its own. Add Note Here
//! (⌥⇧⌘N) starts a line in it with the date and a link to the place it was asked from (`discussion.md:35`,
//! `synopsis.pdf p. 12`); a ⌘-click on such a link goes back there (`open_place`).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, EventTarget, Manager, Webview};
use write_core::notes::{self, Note};
use write_core::project::{self, Kind, Manifest, Project};

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

/// A project's notes file: its path, and its name as the pane shows it (its path in the project).
#[derive(Serialize)]
pub struct ProjectNotes {
    path: String,
    name: String,
}

/// The lines Add Note Here started, by notes file, till the notes' editor takes them (`take_notes_line`).
#[derive(Default)]
pub struct Started(Mutex<HashMap<PathBuf, Vec<String>>>);

/// The project of the page's file.
fn project_of_page(app: &AppHandle, label: &str) -> Result<Project, String> {
    let file = crate::documents::path_of(app, label).ok_or("Save the document first: the notes are the project's")?;
    let root = project::find(&file).ok_or("Not in a project: File ▸ Open Project… makes a folder one")?;
    Project::read(&root).map_err(|e| e.to_string())
}

/// The project's notes file, made (with a heading) when it is not there yet.
fn notes_file(project: &Project) -> Result<ProjectNotes, String> {
    let path = project.notes();
    if !path.exists() {
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        }
        crate::files::write(&path, "# Notes\n\n").map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let path = path.canonicalize().unwrap_or(path);
    let name = path.strip_prefix(&project.root).unwrap_or(&path).to_string_lossy().into_owned();
    Ok(ProjectNotes { path: path.to_string_lossy().into_owned(), name })
}

/// The notes of the page's project (made when they are not there yet).
#[tauri::command]
pub fn project_notes(window: Webview) -> Result<ProjectNotes, String> {
    notes_file(&project_of_page(window.app_handle(), window.label())?)
}

/// The notes shown beside the page's text watched (a change made elsewhere comes to the page as `notes-changed`), or
/// no longer (none).
#[tauri::command]
pub fn watch_notes(window: Webview, path: Option<String>) -> Result<(), String> {
    let app = window.app_handle();
    let key = format!("notes:{}", window.label());
    match path {
        Some(path) => crate::files::watch_for(app, &key, window.label(), Path::new(&path), "notes-changed"),
        None => {
            crate::files::unwatch(app, &key);
            Ok(())
        }
    }
}

/// The notes of the page's project in a tab of their own (the panes in tabs), shown from this page.
#[tauri::command]
pub fn notes_tab_show(window: Webview) -> Result<(), String> {
    let app = window.app_handle();
    let notes = notes_file(&project_of_page(app, window.label())?)?;
    crate::panetabs::show_in_tab(app, window.label(), Path::new(&notes.path)).map(|_| ())
}

/// The project's notes named `notes` in write.toml (the default one: the key taken out).
fn set_notes(app: &AppHandle, project: &mut Project, notes: &Path) -> Result<(), String> {
    let notes = notes.strip_prefix(&project.root).unwrap_or(notes).to_path_buf();
    project.set(&Manifest { notes: Some(notes), ..project.manifest.clone() }).map_err(|e| e.to_string())?;
    crate::projects::changed(app);
    Ok(())
}

/// The notes of the page's project renamed `name` (in their folder; `.md` added without an extension).
#[tauri::command]
pub fn rename_notes(window: Webview, name: String) -> Result<ProjectNotes, String> {
    let app = window.app_handle();
    let mut project = project_of_page(app, window.label())?;
    let name = name.trim();
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.starts_with('.') {
        return Err(format!("“{name}” is not a file name"));
    }
    let name = if Path::new(name).extension().is_some() { name.to_string() } else { format!("{name}.md") };
    let old = PathBuf::from(notes_file(&project)?.path);
    let new = old.with_file_name(&name);
    if new == old {
        return notes_file(&project);
    }
    if new.exists() {
        return Err(format!("{name} is there already: Choose File… to take it as the notes"));
    }
    fs::rename(&old, &new).map_err(|e| e.to_string())?;
    set_notes(app, &mut project, &new)?;
    notes_file(&project)
}

/// Another Markdown file chosen as the notes of the page's project (none: the choice cancelled).
#[tauri::command]
pub async fn choose_notes(window: Webview) -> Result<Option<ProjectNotes>, String> {
    let app = window.app_handle().clone();
    let mut project = project_of_page(&app, window.label())?;
    let Some(picked) = crate::projects::pick_file(&app, &project.root, "notes") else { return Ok(None) };
    set_notes(&app, &mut project, &picked)?;
    notes_file(&project).map(Some)
}

/// Where Add Note Here was asked: the page's file, at a line of a text or a page of a PDF.
#[derive(Deserialize)]
pub struct Place {
    line: Option<u32>,
    page: Option<u32>,
}

/// Add Note Here: a line started in the notes of the page's project, with `date` and a link to `place` in the page's
/// file, for the notes' editor to take (`take_notes_line`; told as `notes-line` to the notes' tab when it is open).
/// The notes.
#[tauri::command]
pub fn note_here(window: Webview, date: String, place: Place) -> Result<ProjectNotes, String> {
    let app = window.app_handle();
    let file = crate::documents::path_of(app, window.label()).ok_or("Save the document first")?;
    let notes = notes_file(&project_of_page(app, window.label())?)?;
    let line = format!("- {date} {}", link_to(Path::new(&notes.path), &file, &place));
    app.state::<Started>().0.lock().unwrap().entry(PathBuf::from(&notes.path)).or_default().push(line);
    if let Some(tab) = crate::tabs::page_of(app, Path::new(&notes.path)) {
        let _ = app.emit_to(EventTarget::webview(&tab), "notes-line", ());
    }
    Ok(notes)
}

/// A Markdown link from the notes `notes` to `place` in `file`: `[discussion.md:35](discussion.md#L35)`,
/// `[synopsis.pdf p. 12](build/synopsis.pdf#page=12)`; the path from the notes' folder (in `<>` with spaces).
fn link_to(notes: &Path, file: &Path, place: &Place) -> String {
    let folder = notes.parent().unwrap_or(Path::new("/"));
    let target = file.strip_prefix(folder).unwrap_or(file).to_string_lossy().into_owned();
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (label, anchor) = match (place.line, place.page) {
        (_, Some(page)) => (format!("{name} p. {page}"), format!("#page={page}")),
        (Some(line), None) => (format!("{name}:{line}"), format!("#L{line}")),
        (None, None) => (name, String::new()),
    };
    let target = format!("{target}{anchor}");
    if target.contains(' ') { format!("[{label}](<{target}>)") } else { format!("[{label}]({target})") }
}

/// The lines started in the notes `path`, taken (none: nothing started).
#[tauri::command]
pub fn take_notes_line(app: AppHandle, path: String) -> Vec<String> {
    let path = Path::new(&path).canonicalize().unwrap_or_else(|_| PathBuf::from(&path));
    app.state::<Started>().0.lock().unwrap().remove(&path).unwrap_or_default()
}

/// A link of the notes `notes` followed: `target` (a path from the notes' folder, or absolute) opened, at its line
/// (`#L35`: the line marked) or its page (`#page=12`).
#[tauri::command]
pub fn open_place(app: AppHandle, notes: String, target: String) -> Result<(), String> {
    let (file, anchor) = target.split_once('#').unwrap_or((target.as_str(), ""));
    let folder = Path::new(&notes).parent().unwrap_or(Path::new("/"));
    let path = folder.join(file);
    if !path.is_file() {
        return Err(format!("{file}: not found"));
    }
    if let Some(page) = anchor.strip_prefix("page=").and_then(|p| p.parse().ok()) {
        return crate::panetabs::show_page(&app, &path, page);
    }
    match anchor.strip_prefix('L').and_then(|l| l.parse::<usize>().ok()) {
        Some(line) => {
            let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let (at, end) = line_range(&text, line);
            crate::sidebar::open_marked(&app, &path, at, end)
        }
        None => crate::documents::open_file(&app, &path),
    }
}

/// Where line `line` (from 1) of `text` starts and ends, counted as the editor counts (UTF-16); past the end, the end.
fn line_range(text: &str, line: usize) -> (usize, usize) {
    let mut at = 0;
    for (i, l) in text.split('\n').enumerate() {
        let len: usize = l.encode_utf16().count();
        if i + 1 == line {
            return (at, at + len);
        }
        at += len + 1;
    }
    let end = text.encode_utf16().count();
    (end, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_from_the_notes_to_a_line_or_a_page() {
        let notes = Path::new("/p/notes.md");
        let at = |line, page| Place { line, page };
        assert_eq!(link_to(notes, Path::new("/p/discussion.md"), &at(Some(35), None)), "[discussion.md:35](discussion.md#L35)");
        assert_eq!(link_to(notes, Path::new("/p/build/synopsis.pdf"), &at(None, Some(12))), "[synopsis.pdf p. 12](build/synopsis.pdf#page=12)");
        assert_eq!(link_to(notes, Path::new("/p/my text.md"), &at(Some(1), None)), "[my text.md:1](<my text.md#L1>)");
        assert_eq!(link_to(notes, Path::new("/elsewhere/a.md"), &at(Some(2), None)), "[a.md:2](/elsewhere/a.md#L2)");
    }

    #[test]
    fn a_line_as_the_editor_counts() {
        let text = "é one\ntwo 😀\nthree";
        assert_eq!(line_range(text, 1), (0, 5));
        assert_eq!(line_range(text, 2), (6, 12));
        assert_eq!(line_range(text, 3), (13, 18));
        assert_eq!(line_range(text, 9), (18, 18));
    }
}
