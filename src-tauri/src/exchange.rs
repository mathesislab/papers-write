//! A review that comes in and goes out without the app on the other side (docs/design.md, "Sending a review, without
//! git"). Compare as Suggestions turns an edited copy of the document, a commit or a branch into suggestions under
//! the name of whoever made it. Export as Word with Changes makes a .docx of the document with the open suggestions
//! and comments as Word's tracked changes and comments (pandoc: `brew install pandoc`). The document is the file of
//! the window: the page has written it before it asks. Import Word Review reads a .docx back (its tracked changes,
//! the changes made with tracking off, its comments) as the suggestions and comments of their authors.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, EventTarget, Manager, WebviewWindow};
use write_core::review::{self, Person, Review};

use crate::{build, documents, preview};

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

/// pandoc run in `dir` with `args`, given `input` on its standard input; what it wrote, else what it said.
fn pandoc(args: &[&str], dir: &Path, input: &[u8]) -> Result<Vec<u8>, String> {
    let mut child = Command::new(preview::pandoc()?)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("pandoc did not start: {e}"))?;
    // fed by a thread: pandoc may write while it reads
    let mut stdin = child.stdin.take().unwrap();
    let input = input.to_vec();
    let feed = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let _ = feed.join();
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Whether pandoc should resolve citations: the text names a bibliography.
fn cites(text: &str) -> bool {
    text.contains("bibliography") || text.contains("references:")
}

/// What an export made.
#[derive(Serialize)]
pub struct Exported {
    pub path: String,
    pub suggestions: usize,
    pub comments: usize,
    /// what could not go to Word, one line each
    pub skipped: Vec<String>,
    /// what pandoc said that went wrong, where the document was made all the same
    pub notes: Vec<String>,
}

/// pandoc makes the .docx `to` of `markdown` in `dir` (so `bibliography:` and the images resolve), as the preview
/// renders: the cross-reference filter (if `filter`), then citeproc (if `citeproc`).
fn docx_with(markdown: &str, dir: &Path, to: &Path, citeproc: bool, filter: Option<&Path>) -> Result<(), String> {
    let resource = format!("--resource-path={}", dir.display());
    let mut args = vec!["-f".to_string(), "markdown".into(), "-t".into(), "docx".into(), "-o".into(), to.to_string_lossy().into_owned(), resource];
    if let Some(filter) = filter {
        args.push(format!("--lua-filter={}", filter.display()));
    }
    if citeproc {
        args.push("--citeproc".into());
    }
    pandoc(&args.iter().map(String::as_str).collect::<Vec<_>>(), dir, markdown.as_bytes()).map(drop)
}

/// The .docx `to` of `markdown`: cross-references numbered as in the preview and citations resolved when the text
/// names a bibliography. What fails is dropped in turn, the filter first (an error that speaks of Lua or a filter),
/// then the citations (a bibliography that is missing or does not parse), with a note of it.
fn docx_of(markdown: &str, text: &str, dir: &Path, to: &Path) -> Result<Option<String>, String> {
    let (mut filter, mut citeproc, mut note) = (preview::crossref_filter(), cites(text), None);
    loop {
        match docx_with(markdown, dir, to, citeproc, filter) {
            Ok(()) => return Ok(note),
            Err(e) => {
                let lower = e.to_lowercase();
                if filter.is_some() && (lower.contains("lua") || lower.contains("filter")) {
                    filter = None;
                    note = Some(format!("Cross-references left as typed: {}", e.lines().next().unwrap_or("")));
                } else if citeproc {
                    citeproc = false;
                    note = Some(format!("Citations left as typed: {}", e.lines().next().unwrap_or("")));
                } else {
                    return Err(e);
                }
            }
        }
    }
}

/// The .docx of `current` with `items` (the open ones) as tracked changes and comments, written to `to`.
fn export(doc: &Path, current: &str, to: &Path) -> Result<Exported, String> {
    let items = Review::load(doc).map_err(|e| e.to_string())?.items(current);
    let made = review::word::markdown_with_changes(current, &items);
    let dir = doc.parent().ok_or("no folder")?;
    let notes = docx_of(&made.markdown, current, dir, to)?.into_iter().collect();
    Ok(Exported { path: to.to_string_lossy().into_owned(), suggestions: made.suggestions, comments: made.comments, skipped: made.skipped, notes })
}

/// The window's document as a .docx with its open suggestions and comments as tracked changes and comments, at
/// `path` (none: where a dialog says, `<name>.docx` beside the document to begin with). None when the dialog was
/// cancelled.
#[tauri::command]
pub async fn export_word(app: AppHandle, window: WebviewWindow, path: Option<String>) -> Result<Option<Exported>, String> {
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
    tauri::async_runtime::spawn_blocking(move || export(&doc, &read(&doc)?, &to).map(Some)).await.map_err(|e| e.to_string())?
}

// ---- Import Word Review

/// What an import made.
#[derive(Serialize, Default)]
pub struct Imported {
    pub suggestions: usize,
    pub comments: usize,
    /// whose, each once
    pub authors: Vec<String>,
    /// what could not be placed in the text, one line each
    pub unplaced: Vec<String>,
}

/// A .docx's text as pandoc reads it with the tracked changes rejected or accepted, or all of it as JSON.
fn read_docx(docx: &Path, dir: &Path, mode: &str, to: &str) -> Result<String, String> {
    let file = docx.to_string_lossy().into_owned();
    let mode = format!("--track-changes={mode}");
    let mut args = vec![file.as_str(), mode.as_str(), "-t", to];
    if to == "plain" {
        args.push("--wrap=none");
    }
    pandoc(&args, dir, b"").map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// `current` as plain text the way the .docx's own text is: made into a .docx by the export's pipeline and read back,
/// with citations resolved and cross-references numbered or as typed (a .docx that was made elsewhere may have
/// either).
fn renderings(current: &str, dir: &Path) -> Result<Vec<String>, String> {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let to = std::env::temp_dir().join(format!("pw-compare-{}-{nonce}.docx", std::process::id()));
    let mut out = Vec::new();
    // as typed first, then what the export makes (a variant that fails is left out: the bibliography does not work,
    // or the filter)
    let filter = preview::crossref_filter();
    let mut variants = vec![(false, None)];
    if cites(current) {
        variants.push((true, None));
    }
    if filter.is_some() {
        variants.push((false, filter));
        if cites(current) {
            variants.push((true, filter));
        }
    }
    for (n, (citeproc, filter)) in variants.into_iter().enumerate() {
        match docx_with(current, dir, &to, citeproc, filter).and_then(|()| read_docx(&to, dir, "reject", "plain")) {
            Ok(text) => out.push(text),
            Err(e) if n == 0 => {
                let _ = std::fs::remove_file(&to);
                return Err(e);
            }
            Err(_) => {}
        }
    }
    let _ = std::fs::remove_file(&to);
    Ok(out)
}

/// The suggestions and comments of the .docx at `docx` written to the review of `doc`, each person's to their own
/// file; `current` is the text of `doc`. With `also_untracked`, changes made with tracking off are found too (the
/// whole text compared) and are `fallback_author`'s ("Word" when empty).
fn import(doc: &Path, current: &str, docx: &Path, also_untracked: bool, fallback_author: &str) -> Result<Imported, String> {
    let dir = doc.parent().ok_or("no folder")?;
    let (all, reject, accept) = (read_docx(docx, dir, "all", "json")?, read_docx(docx, dir, "reject", "plain")?, read_docx(docx, dir, "accept", "plain")?);
    let rendered = if also_untracked { renderings(current, dir)? } else { Vec::new() };
    let untracked = also_untracked.then(|| review::word::Untracked { author: fallback_author, renderings: &rendered });
    let found = review::word::import(current, &all, &reject, &accept, untracked);

    let round = format!("Word review: {}", docx.file_name().unwrap_or_default().to_string_lossy());
    let mut made = Imported { suggestions: found.suggestions.len(), comments: found.comments.len(), unplaced: found.unplaced, ..Default::default() };
    for (author, record) in found.suggestions.into_iter().chain(found.comments) {
        let me = Person::new(author.as_str(), "");
        Review::add(doc, &me, current, record.round(round.clone())).map_err(|e| e.to_string())?;
        if !made.authors.contains(&author) {
            made.authors.push(author);
        }
    }
    Ok(made)
}

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
    let made = tauri::async_runtime::spawn_blocking(move || import(&doc, &read(&doc)?, Path::new(&path), also_untracked, &fallback_author))
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
    fn the_document_goes_to_word_with_its_changes() {
        if preview::pandoc().is_err() {
            return;
        }
        let dir = folder("word");
        let doc = dir.join("paper.md");
        let current = "The grounding line retreats rapidly when the bed slopes.\n";
        std::fs::write(&doc, current).unwrap();
        let theirs = dir.join("theirs.md");
        std::fs::write(&theirs, "The grounding line retreats slowly when the bed slopes.\n").unwrap();
        let source = Source { file: Some(theirs.to_string_lossy().into_owned()), revision: None };
        compare(&doc, current, &source, &Author { name: "Jane Doe".into(), email: None }).unwrap();
        let to = dir.join("paper.docx");
        let made = export(&doc, current, &to).unwrap();
        assert_eq!((made.suggestions, made.comments), (1, 0));
        let read = |mode: &str| {
            let out = Command::new(preview::pandoc().unwrap()).arg(&to).args([&format!("--track-changes={mode}"), "-t", "plain", "--wrap=none"]).output().unwrap();
            String::from_utf8(out.stdout).unwrap()
        };
        assert!(read("reject").contains("retreats rapidly when"));
        assert!(read("accept").contains("retreats slowly when"));
    }

    #[test]
    fn cross_references_are_numbered_in_the_docx_as_in_the_preview() {
        if preview::pandoc().is_err() {
            return;
        }
        let dir = folder("xref");
        let doc = dir.join("paper.md");
        let current = "See @fig:x and \\ref{eq:y}.\n\n![The loop](loop.png){#fig:x}\n\n\\begin{equation}\na = b \\label{eq:y}\n\\end{equation}\n";
        std::fs::write(&doc, current).unwrap();
        let to = dir.join("paper.docx");
        let made = export(&doc, current, &to).unwrap();
        assert!(made.notes.is_empty(), "{:?}", made.notes);
        let out = Command::new(preview::pandoc().unwrap()).arg(&to).args(["-t", "plain", "--wrap=none"]).output().unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(text.contains("Fig.\u{a0}1") || text.contains("Fig. 1"), "{text}");
        assert!(!text.contains("fig:x?") && !text.contains("eq:y?") && !text.contains("@fig:x"), "{text}");
    }

    #[test]
    fn a_word_review_goes_out_and_comes_back_as_the_same_review() {
        if preview::pandoc().is_err() {
            return;
        }
        let dir = folder("roundtrip");
        let doc = dir.join("paper.md");
        let current = "The *grounding line* retreats rapidly [@smith2020] when the bed slopes.\n\nThe ice shelf thins.\n";
        std::fs::write(&doc, current).unwrap();
        let theirs = dir.join("theirs.md");
        std::fs::write(&theirs, "The *grounding line* retreats slowly [@smith2020] when the bed slopes.\n\nThe ice shelf thickens.\n").unwrap();
        let source = Source { file: Some(theirs.to_string_lossy().into_owned()), revision: None };
        compare(&doc, current, &source, &Author { name: "Jane Doe".into(), email: None }).unwrap();
        let to = dir.join("paper.docx");
        export(&doc, current, &to).unwrap();

        // the same document somewhere else, with no review: the .docx brings it back
        let other = folder("roundtrip-other");
        let doc2 = other.join("paper.md");
        std::fs::write(&doc2, current).unwrap();
        let made = import(&doc2, current, &to, false, "Me").unwrap();
        assert_eq!((made.suggestions, made.comments), (2, 0));
        assert!(made.unplaced.is_empty(), "{:?}", made.unplaced);
        assert_eq!(made.authors, ["Jane Doe"]);
        let items = Review::load(&doc2).unwrap().items(current);
        assert!(items.iter().all(|i| i.author == "Jane Doe" && i.record.round.as_deref() == Some("Word review: paper.docx")));
        let applied = review::apply_all(current, &items);
        assert_eq!(applied.split_whitespace().collect::<Vec<_>>().join(" "), "The *grounding line* retreats slowly [@smith2020] when the bed slopes. The ice shelf thickens.");
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
