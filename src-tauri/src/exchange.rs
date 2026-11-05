//! A review that comes in and goes out without the app on the other side (docs/design.md, "Sending a review, without
//! git"). Compare as Suggestions turns an edited copy of the document, a commit or a branch into suggestions under
//! the name of whoever made it. Export as Word with Changes makes a .docx of the document with the open suggestions
//! and comments as Word's tracked changes and comments (pandoc: `brew install pandoc`). The document is the file of
//! the window: the page has written it before it asks. Import Word Review reads a .docx back (its tracked changes,
//! the changes made with tracking off, its comments) as the suggestions and comments of their authors.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, EventTarget, Manager, WebviewWindow};
use write_core::exchange::{self, Imported};
use write_core::review::{self, Person, Review};

use crate::{build, documents};

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

/// Where the edited text comes from: a file (any path), or a revision of the document in git (a commit, a tag, a
/// branch).
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

/// What a revision is called in a round: a hash shortened.
fn short(rev: &str) -> &str {
    if rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()) { &rev[..8] } else { rev }
}

/// The suggestions that turn `current` into the text of `source`, written as the suggestions of `author` (their own
/// file, as if they had suggested); how many.
fn compare(doc: &Path, current: &str, source: &Source, author: &Author) -> Result<usize, String> {
    let name = author.name.trim();
    if name.is_empty() {
        return Err("Whose edits are these? A name is needed".into());
    }
    let (edited, round) = match (&source.file, &source.revision) {
        (Some(file), _) => {
            let file = Path::new(file);
            (read(file)?, format!("Compared with {}", file.file_name().unwrap_or_default().to_string_lossy()))
        }
        (None, Some(rev)) => {
            let folder = doc.parent().ok_or("no folder")?;
            let file_name = doc.file_name().unwrap_or_default().to_string_lossy();
            let bytes = build::git_show(folder, &file_name, rev)?;
            (String::from_utf8(bytes).map_err(|_| format!("{rev}: not a UTF-8 text file"))?, format!("Compared with {}", short(rev)))
        }
        (None, None) => return Err("Choose a file or a revision to compare with".into()),
    };
    let me = Person::new(name, author.email.as_deref().unwrap_or("").trim());
    let list = review::compare::suggestions(current, &edited);
    for s in &list {
        Review::add(doc, &me, current, s.clone().round(round.clone())).map_err(|e| e.to_string())?;
    }
    Ok(list.len())
}

/// The window's document compared with a file or a revision: each change is a suggestion of `author`. The count.
#[tauri::command]
pub async fn compare_as_suggestions(app: AppHandle, window: WebviewWindow, source: Source, author: Author) -> Result<usize, String> {
    let doc = document_of(&window)?;
    let n = tauri::async_runtime::spawn_blocking(move || {
        let current = read(&doc)?;
        compare(&doc, &current, &source, &author)
    })
    .await
    .map_err(|e| e.to_string())??;
    if n > 0 {
        changed(&app, &window);
    }
    Ok(n)
}

#[derive(Serialize)]
pub struct Who {
    pub name: String,
    pub email: String,
}

/// The author of the commit `revision` (a tag or a branch: of the commit it names); none when git does not say.
#[tauri::command]
pub async fn revision_author(window: WebviewWindow, revision: String) -> Result<Option<Who>, String> {
    let doc = document_of(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let folder = doc.parent()?;
        let out = build::git_lines(folder, &["log", "-n", "1", "--format=%an%x1f%ae", &revision, "--"])?;
        let (name, email) = out.trim().split_once('\u{1f}')?;
        (!name.is_empty()).then(|| Who { name: name.to_string(), email: email.to_string() })
    })
    .await
    .map_err(|e| e.to_string())
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

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pw-exchange-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn short_hashes() {
        assert_eq!(short("0123456789abcdef0123456789abcdef01234567"), "01234567");
        assert_eq!(short("v1"), "v1");
        assert_eq!(short("feature/x"), "feature/x");
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
        assert_eq!(compare(&doc, current, &source, &author).unwrap(), 2);
        let items = Review::load(&doc).unwrap().items(current);
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.author == "Jane Doe" && i.record.round.as_deref() == Some("Compared with paper-jane.md")));
        assert!(compare(&doc, current, &source, &Author { name: " ".into(), email: None }).is_err());
    }

    #[test]
    fn a_revision_is_read_from_git() {
        let dir = folder("git");
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git").arg("-C").arg(&dir).args(args).output().unwrap();
            assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        git(&["init", "-q"]);
        git(&["config", "user.name", "Bob"]);
        git(&["config", "user.email", "bob@example.org"]);
        git(&["config", "commit.gpgsign", "false"]);
        let doc = dir.join("paper.md");
        std::fs::write(&doc, "one two three\n").unwrap();
        git(&["add", "paper.md"]);
        git(&["commit", "-q", "-m", "first"]);
        let current = "one 2 three\n";
        let source = Source { file: None, revision: Some("HEAD".into()) };
        assert_eq!(compare(&doc, current, &source, &Author { name: "Bob".into(), email: None }).unwrap(), 1);
        let items = Review::load(&doc).unwrap().items(current);
        assert_eq!(items[0].record.insert.as_deref(), Some("two"));
        assert_eq!(items[0].record.round.as_deref(), Some("Compared with HEAD"));
    }
}
