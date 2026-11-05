//! A review that comes in and goes out without the app on the other side (docs/design.md, "Sending a review, without
//! git"). Compare as Suggestions turns an edited copy of the document, a commit or a branch into suggestions under
//! the name of whoever made each change. Export as Word with Changes makes a .docx of the document with the open suggestions
//! and comments as Word's tracked changes and comments (pandoc: `brew install pandoc`). The document is the file of
//! the window: the page has written it before it asks. Import Word Review reads a .docx back (its tracked changes,
//! the changes made with tracking off, its comments) as the suggestions and comments of their authors.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tauri::{AppHandle, Emitter, EventTarget, Manager, WebviewWindow};
use write_core::exchange::{self, Imported};
use write_core::review::history::{self, Added, Change};
use write_core::review::{self, Person};

use crate::{documents, git};

/// The file of the window, to take the review of.
fn document_of(window: &WebviewWindow) -> Result<PathBuf, String> {
    documents::path_of(window.app_handle(), window.label()).ok_or_else(|| "Save the document first (⌘S)".to_string())
}

fn read(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    String::from_utf8(bytes).map_err(|_| format!("{}: not a UTF-8 text file", path.display()))
}

/// The review changed on disk: the panes that show it read it again.
fn changed(app: &AppHandle, window: &WebviewWindow) {
    let _ = app.emit_to(EventTarget::webview(window.label()), "review-changed", ());
}

// ---- Compare as Suggestions

/// Where the edited text comes from: a file (any path), or a branch or commit in git.
#[derive(Deserialize)]
pub struct Source {
    pub file: Option<String>,
    pub revision: Option<String>,
}

#[derive(Deserialize)]
pub struct Author {
    pub name: String,
    pub email: Option<String>,
}

/// The edits of `source` written as suggestions. A file: the changes from `current` to it, under `author` (their own
/// file, as if they had suggested). A branch or a commit: what was done there since it left this branch's history,
/// each change under the author of the commit that made it (git.rs, `review_revision`). Those the review has already
/// are not written again.
fn compare(doc: &Path, current: &str, source: &Source, author: Option<&Author>) -> Result<Added, String> {
    match (&source.file, &source.revision) {
        (Some(file), _) => {
            let name = author.map_or("", |a| a.name.trim());
            if name.is_empty() {
                return Err("Whose edits are these? A name is needed".into());
            }
            let me = Person::new(name, author.and_then(|a| a.email.as_deref()).unwrap_or("").trim());
            let file = Path::new(file);
            let round = format!("Compared with {}", file.file_name().unwrap_or_default().to_string_lossy());
            let changes: Vec<Change> = review::compare::suggestions(current, &read(file)?)
                .into_iter()
                .map(|r| Change { author: me.clone(), record: r.round(round.clone()) })
                .collect();
            history::add(doc, current, &changes).map_err(|e| e.to_string())
        }
        (None, Some(rev)) => git::review_revision(doc, rev.trim()),
        (None, None) => Err("Choose a branch, a commit or a file to compare with".into()),
    }
}

/// The window's document compared with a file, a branch or a commit: each change a suggestion of whoever made it
/// (`author`, for a file). What was written.
#[tauri::command]
pub async fn compare_as_suggestions(app: AppHandle, window: WebviewWindow, source: Source, author: Option<Author>) -> Result<Added, String> {
    let doc = document_of(&window)?;
    let made = tauri::async_runtime::spawn_blocking(move || {
        let current = read(&doc)?;
        compare(&doc, &current, &source, author.as_ref())
    })
    .await
    .map_err(|e| e.to_string())??;
    if made.added > 0 {
        changed(&app, &window);
    }
    Ok(made)
}

/// The file to compare with, chosen in a dialog; none when it was cancelled.
#[tauri::command]
pub async fn choose_edited_copy(app: AppHandle, window: WebviewWindow) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = app.dialog().file().set_title("Edited copy");
    if let Ok(doc) = document_of(&window)
        && let Some(folder) = doc.parent()
    {
        dialog = dialog.set_directory(folder);
    }
    Ok(dialog.blocking_pick_file().and_then(|f| f.into_path().ok()).map(|p| p.to_string_lossy().into_owned()))
}

// ---- Export as Word with Changes

/// The window's document as a .docx with its open suggestions and comments as tracked changes and comments, at
/// `path` (none: where a dialog says, `<name>.docx` beside the document to begin with). None when the dialog was
/// cancelled.
#[tauri::command]
pub async fn export_word(app: AppHandle, window: WebviewWindow, path: Option<String>) -> Result<Option<exchange::Exported>, String> {
    use tauri_plugin_dialog::DialogExt;
    let doc = document_of(&window)?;
    let to = match path {
        Some(p) => PathBuf::from(p),
        None => {
            let mut dialog = app.dialog().file().set_title("Export as Word").add_filter("Word", &["docx"]);
            if let Some(folder) = doc.parent() {
                dialog = dialog.set_directory(folder);
            }
            if let Some(stem) = doc.file_stem() {
                dialog = dialog.set_file_name(format!("{}.docx", stem.to_string_lossy()));
            }
            match dialog.blocking_save_file().and_then(|f| f.into_path().ok()) {
                Some(p) => p,
                None => return Ok(None),
            }
        }
    };
    let to = if to.extension().is_some_and(|e| e.eq_ignore_ascii_case("docx")) { to } else { to.with_extension("docx") };
    tauri::async_runtime::spawn_blocking(move || exchange::export(&doc, &read(&doc)?, &to).map(Some)).await.map_err(|e| e.to_string())?
}

// ---- Import Word Review

/// The .docx to import, chosen in a dialog; none when it was cancelled.
#[tauri::command]
pub async fn choose_word_file(app: AppHandle, window: WebviewWindow) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = app.dialog().file().set_title("Word review").add_filter("Word", &["docx"]);
    if let Ok(doc) = document_of(&window)
        && let Some(folder) = doc.parent()
    {
        dialog = dialog.set_directory(folder);
    }
    Ok(dialog.blocking_pick_file().and_then(|f| f.into_path().ok()).map(|p| p.to_string_lossy().into_owned()))
}

/// The .docx at `path` read as a review of the window's document: its tracked changes and comments as suggestions and
/// comments, each under the name of whoever made it (their own review file); with `also_untracked`, what was changed
/// with tracking off under `fallback_author`. What could not be placed in the text is returned, not written.
#[tauri::command]
pub async fn import_word(app: AppHandle, window: WebviewWindow, path: String, also_untracked: bool, fallback_author: String) -> Result<Imported, String> {
    let doc = document_of(&window)?;
    let made = tauri::async_runtime::spawn_blocking(move || exchange::import(&doc, &read(&doc)?, Path::new(&path), also_untracked, &fallback_author))
        .await
        .map_err(|e| e.to_string())??;
    if made.suggestions + made.comments > 0 {
        changed(&app, &window);
    }
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_core::review::Review;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pw-exchange-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_file_becomes_the_suggestions_of_its_author() {
        let dir = folder("file");
        let doc = dir.join("paper.md");
        let current = "The line retreats rapidly when the bed slopes.\n";
        std::fs::write(&doc, current).unwrap();
        let theirs = dir.join("paper-jane.md");
        std::fs::write(&theirs, "The line retreats slowly when the bed slopes inland.\n").unwrap();
        let source = Source { file: Some(theirs.to_string_lossy().into_owned()), revision: None };
        let author = Author { name: "Jane Doe".into(), email: Some("jane@example.org".into()) };
        let made = compare(&doc, current, &source, Some(&author)).unwrap();
        assert_eq!((made.added, made.already, made.people), (2, 0, vec!["Jane Doe".to_string()]));
        let items = Review::load(&doc).unwrap().items(current);
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.author == "Jane Doe" && i.record.round.as_deref() == Some("Compared with paper-jane.md")));
        // again: in the review already
        assert_eq!(compare(&doc, current, &source, Some(&author)).unwrap().already, 2);
        assert!(compare(&doc, current, &source, Some(&Author { name: " ".into(), email: None })).is_err());
        assert!(compare(&doc, current, &source, None).is_err());
    }

    #[test]
    fn a_branch_is_compared_from_where_it_started() {
        let dir = folder("git");
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git").arg("-C").arg(&dir).args(args).output().unwrap();
            assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.name", "Alex"]);
        git(&["config", "user.email", "alex@example.org"]);
        git(&["config", "commit.gpgsign", "false"]);
        let doc = dir.join("paper.md");
        std::fs::write(&doc, "one two three\n").unwrap();
        git(&["add", "paper.md"]);
        git(&["commit", "-q", "-m", "first"]);
        git(&["checkout", "-q", "-b", "bob/edits"]);
        std::fs::write(&doc, "one 2 three\n").unwrap();
        git(&["-c", "user.name=Bob", "-c", "user.email=bob@example.org", "commit", "-q", "-am", "A number"]);
        git(&["checkout", "-q", "main"]);
        // the author writes on, committed or not: not undone by the comparison
        let current = "one two three four\n";
        std::fs::write(&doc, current).unwrap();
        let source = |rev: &str| Source { file: None, revision: Some(rev.into()) };
        let made = compare(&doc, current, &source("bob/edits"), None).unwrap();
        assert_eq!((made.added, made.people.clone()), (1, vec!["Bob".to_string()]));
        let items = Review::load(&doc).unwrap().items(current);
        assert_eq!(items.len(), 1);
        assert_eq!((items[0].author.as_str(), items[0].record.insert.as_deref()), ("Bob", Some("2")));
        assert!(items[0].record.round.as_deref().unwrap().ends_with(" A number"));
        assert_eq!(write_core::review::apply(current, &items[0]), "one 2 three four\n");
        assert_eq!(compare(&doc, current, &source("bob/edits"), None).unwrap().already, 1);
        // a commit of this branch has nothing that is not here already
        assert!(compare(&doc, current, &source("HEAD"), None).unwrap_err().contains("in the history of this branch"));
        assert!(compare(&doc, current, &source("nonsense"), None).is_err());
    }
}
