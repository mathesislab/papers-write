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
use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use write_core::review::package;

use crate::{documents, review};

/// The `.review` extension, in any case.
pub fn is_review(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(package::EXTENSION))
}

/// What a name goes into a file name as: no slash or colon.
fn file_part(s: &str) -> String {
    s.trim().chars().map(|c| if matches!(c, '/' | ':' | '\\' | '\0') { '-' } else { c }).collect()
}

/// `<stem> — <what>.review`.
fn file_name(doc: &Path, what: &str) -> String {
    format!("{} — {}.{}", doc.file_stem().unwrap_or_default().to_string_lossy(), file_part(what), package::EXTENSION)
}

fn unreadable(file: &Path, e: impl std::fmt::Display) -> String {
    format!("{}: {e}", file.display())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_of_packages() {
        let doc = Path::new("/a/b/paper.md");
        assert_eq!(file_name(doc, "Draft of 5 Nov 2026"), "paper — Draft of 5 Nov 2026.review");
        assert_eq!(file_name(doc, " v1/2: x "), "paper — v1-2- x.review");
        assert!(is_review(Path::new("x.REVIEW")) && !is_review(Path::new("x.md")));
    }
}
