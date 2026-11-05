//! A review sent without git (docs/design.md, "Sending a review, without git"): the `.review` file that
//! `write_core::review::package` makes and reads.
//!
//! - Send for Review… (File ▸ Exchange): the document of the window (its page has written it) with what it shows and
//!   its review, as `<name> — <round>.review` where a dialog says.
//! - A `.review` opened (Open With, double click, ⌘O, a drop) is, by its kind, a document to review or a review sent
//!   back. The first is unpacked in the app's data folder (`reviews/<name>-<hash8>/`: once for a package, so that
//!   opening it again finds what was done there) and its document opens in a window titled "<name> — review for <who>",
//!   which asks for suggest mode (`review_package` says what the window is a review of). The second is joined to the
//!   review of the document it is of: the open window that has the base it was made on (a base of its review, its
//!   text, its history), else the document asked for.
//! - Send Back… (File ▸ Exchange, the bar of the window): the user's own review file and the bases it needs, as
//!   `<name> — <who>.review`.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, Manager, WebviewWindow};
use crate::sheet::{self, Level};
use tauri_plugin_dialog::DialogExt;
use write_core::review::package::{self, Merged, Opened, PackageKind};
use write_core::review::text::slug;
use write_core::review::{Person, store};

use crate::{documents, review};

/// The `.review` extension, in any case.
pub fn is_review(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(package::EXTENSION))
}

/// Where packages to review unpack.
fn root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("reviews"))
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// What a name goes into a file name as: no slash or colon.
fn file_part(s: &str) -> String {
    s.trim().chars().map(|c| if matches!(c, '/' | ':' | '\\' | '\0') { '-' } else { c }).collect()
}

/// `<stem> — <what>.review`.
fn file_name(doc: &Path, what: &str) -> String {
    format!("{} — {}.{}", doc.file_stem().unwrap_or_default().to_string_lossy(), file_part(what), package::EXTENSION)
}

/// What a merge says: "12 suggestions and 3 comments from Jane Doe".
fn said(merged: &Merged, from: &str) -> String {
    let (s, c) = (merged.suggestions, merged.comments);
    let mut parts = Vec::new();
    if s > 0 {
        parts.push(plural(s, "suggestion", "suggestions"));
    }
    if c > 0 {
        parts.push(plural(c, "comment", "comments"));
    }
    if parts.is_empty() && merged.others > 0 {
        parts.push(plural(merged.others, "reply or decision", "replies and decisions"));
    }
    if parts.is_empty() {
        return format!("Nothing new from {from}: their review is already joined to this document.");
    }
    format!("{} from {from}", parts.join(" and "))
}

fn unreadable(file: &Path, e: impl std::fmt::Display) -> String {
    format!("{}: {e}", file.display())
}

// ---- the page

/// What a window is a review of, for its page.
#[derive(Serialize)]
pub struct Reviewing {
    pub name: String,
    pub email: String,
    pub round: String,
    pub document: String,
}

/// What the document of the window is a review of: none unless it was opened from a `.review`.
#[tauri::command]
pub fn review_package(window: WebviewWindow) -> Option<Reviewing> {
    let doc = documents::path_of(window.app_handle(), window.label())?;
    let m = package::of_unpacked(&doc)?;
    Some(Reviewing { name: m.from.name, email: m.from.email, round: m.round, document: m.document })
}

// ---- send

/// What a package sent holds.
#[derive(Serialize)]
pub struct Sent {
    pub path: String,
    pub figures: usize,
    pub bibliographies: usize,
    /// the files the document names that could not go along, one line each
    pub skipped: Vec<String>,
}

/// The document of the window with what it shows and its review as a `.review` file named for `round`, where a dialog
/// says (beside the document to begin with). None when the dialog was cancelled.
#[tauri::command]
pub async fn send_for_review(app: AppHandle, window: WebviewWindow, round: String) -> Result<Option<Sent>, String> {
    let doc = documents::path_of(&app, window.label()).ok_or("Save the document first (⌘S)")?;
    let round = round.trim().to_string();
    if round.is_empty() {
        return Err("Name the round (Draft of …)".into());
    }
    let me = review::me(&window)?;
    let Some(to) = save_dialog(&app, &doc, &file_name(&doc, &round), "Send for Review") else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || {
        let text = std::fs::read_to_string(&doc).map_err(|e| unreadable(&doc, e))?;
        let sent = package::send(&doc, &text, &me, &round, &to).map_err(|e| e.to_string())?;
        Ok(Some(Sent { path: to.to_string_lossy().into_owned(), figures: sent.figures, bibliographies: sent.bibliographies, skipped: sent.skipped }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The `.review` file to write, chosen in a dialog starting in the folder of `doc`; none when cancelled.
fn save_dialog(app: &AppHandle, doc: &Path, name: &str, title: &str) -> Option<PathBuf> {
    let mut dialog = app.dialog().file().set_title(title).add_filter("Review", &[package::EXTENSION]).set_file_name(name);
    if let Some(folder) = doc.parent() {
        dialog = dialog.set_directory(folder);
    }
    let to = dialog.blocking_save_file().and_then(|f| f.into_path().ok())?;
    Some(if is_review(&to) { to } else { to.with_extension(package::EXTENSION) })
}

// ---- send back

/// What a package sent back holds.
#[derive(Serialize)]
pub struct Returned {
    pub path: String,
    pub suggestions: usize,
    pub comments: usize,
    /// whose
    pub to: String,
}

/// The user's own review of the window's document (one opened from a `.review`) as a `.review` file named for them,
/// where a dialog says. None when the dialog was cancelled.
#[tauri::command]
pub async fn send_back(app: AppHandle, window: WebviewWindow) -> Result<Option<Returned>, String> {
    let doc = documents::path_of(&app, window.label()).ok_or("Save the document first (⌘S)")?;
    let sent = package::of_unpacked(&doc).ok_or("This document was not opened from a review file: there is nothing to send back")?;
    let me: Person = review::me(&window)?;
    if !store::person_path(&doc, &slug(&me.name)).is_file() {
        return Err("Nothing to send back yet: no suggestion or comment of yours".into());
    }
    let Some(to) = save_dialog(&app, &doc, &file_name(&doc, &me.name), "Send Back") else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || {
        let back = package::send_back(&doc, &me, &to).map_err(|e| e.to_string())?;
        Ok(Some(Returned { path: to.to_string_lossy().into_owned(), suggestions: back.suggestions, comments: back.comments, to: sent.from.name }))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- a .review opened

/// A `.review` file opened, on a thread of its own (it may ask, and a window is made): its document to review, or
/// the review sent back joined to its document. What goes wrong is told in a dialog.
pub fn open(app: &AppHandle, file: PathBuf) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = open_now(&app, &file) {
            sheet::tell(&app, None, "Open", e, Level::Error);
        }
    });
}

fn open_now(app: &AppHandle, file: &Path) -> Result<(), String> {
    let opened = package::open(file).map_err(|e| e.to_string())?;
    match opened.manifest.kind {
        PackageKind::Send => {
            let doc = package::unpack(&opened, &root(app)?).map_err(|e| e.to_string())?;
            package::mark_opened(&doc);
            documents::open_file(app, &doc)
        }
        PackageKind::Back => join(app, &opened),
    }
}

/// Unpacked reviews not used for this long go to the Trash when the app starts.
const KEEP: std::time::Duration = std::time::Duration::from_secs(60 * 86_400);

/// The unpacked reviews unused for 60 days moved to the Trash (never one with a document open), on a thread of its
/// own. What fails is told on stderr; the rest goes on.
pub fn sweep(app: &AppHandle) {
    let Ok(root) = root(app) else { return };
    let app = app.clone();
    std::thread::spawn(move || {
        let open = documents::open_paths(&app);
        for home in package::unused_unpacked(&root, std::time::SystemTime::now(), KEEP, &open) {
            if let Err(e) = crate::trash::trash(&home) {
                eprintln!("{e}");
            }
        }
    });
}

/// The open document that `opened` is a review of: the file name is the same and it has the text the review was made
/// on (a base of its review, its text now, a commit). Not a document that is itself a review to send back.
fn find(app: &AppHandle, opened: &Opened) -> Option<PathBuf> {
    let m = &opened.manifest;
    documents::open_paths(app)
        .into_iter()
        .find(|p| p.file_name().is_some_and(|n| *n == *m.document) && package::of_unpacked(p).is_none() && package::holds(p, &m.base))
}

/// The document asked for when no window has the one the review is of.
fn ask(app: &AppHandle, opened: &Opened) -> Option<PathBuf> {
    let m = &opened.manifest;
    let said = format!(
        "{} sent back a review of {} ({}), which is not open here. Choose the document it is a review of.",
        m.from.name, m.document, m.round
    );
    sheet::ask_blocking(app, None, "Review", said, Level::Info, &["Choose…"]);
    app.dialog().file().set_title(format!("The document {} reviewed", m.document)).blocking_pick_file().and_then(|f| f.into_path().ok())
}

/// A review sent back joined to its document, which then shows it; a dialog says what came.
fn join(app: &AppHandle, opened: &Opened) -> Result<(), String> {
    let Some(doc) = find(app, opened).or_else(|| ask(app, opened)) else {
        return Ok(());
    };
    let merged = package::merge(&doc, opened).map_err(|e| e.to_string())?;
    match documents::label_of(app, &doc) {
        Some(label) => {
            let _ = app.emit_to(EventTarget::webview(label), "review-changed", ());
        }
        None => documents::open_file(app, &doc)?,
    }
    let window = documents::label_of(app, &doc).and_then(|l| app.get_webview_window(&l));
    sheet::tell(app, window.as_ref(), "Review received", said(&merged, &opened.manifest.from.name), Level::Info);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_said_of_a_merge() {
        let m = |s, c, o| Merged { suggestions: s, comments: c, others: o, ..Default::default() };
        assert_eq!(said(&m(12, 3, 1), "Jane Doe"), "12 suggestions and 3 comments from Jane Doe");
        assert_eq!(said(&m(1, 0, 0), "Jane"), "1 suggestion from Jane");
        assert_eq!(said(&m(0, 1, 5), "Jane"), "1 comment from Jane");
        assert_eq!(said(&m(0, 0, 2), "Jane"), "2 replies and decisions from Jane");
        assert!(said(&m(0, 0, 0), "Jane").starts_with("Nothing new from Jane"));
    }

    #[test]
    fn a_document_to_review_is_titled_for_who_sent_it() {
        let dir = std::env::temp_dir().join(format!("pw-app-package-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let doc = dir.join("paper.md");
        std::fs::write(&doc, "A text.\n").unwrap();
        let file = dir.join("paper — Draft.review");
        package::send(&doc, "A text.\n", &Person::new("Alex Robinson", ""), "Draft", &file).unwrap();
        let there = package::unpack(&package::open(&file).unwrap(), &dir.join("reviews")).unwrap();
        assert_eq!(documents::title(Some(&there)), "paper.md — review for Alex Robinson");
        assert_eq!(documents::title(Some(&doc)), "paper.md");
        assert_eq!(documents::title(None), "Untitled");
    }

    #[test]
    fn file_names_of_packages() {
        let doc = Path::new("/a/b/paper.md");
        assert_eq!(file_name(doc, "Draft of 5 Nov 2026"), "paper — Draft of 5 Nov 2026.review");
        assert_eq!(file_name(doc, " v1/2: x "), "paper — v1-2- x.review");
        assert!(is_review(Path::new("x.REVIEW")) && !is_review(Path::new("x.md")));
    }
}
