//! The Word side of a review, with pandoc (`brew install pandoc`): the document as a .docx with the open suggestions
//! and comments as tracked changes and comments, and a .docx read back as suggestions and comments. Shared by the
//! app (Export as Word with Changes, Import Word Review) and the command line (`pw review export|import`).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use serde::Serialize;

use crate::review::{self, Person, Review};

/// The pandoc the app's settings name, which wins over the search (`set_pandoc`); none: searched.
static CHOSEN: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// The pandoc to use from now on (the app's settings), or none to search for it again.
pub fn set_pandoc(path: Option<PathBuf>) {
    *CHOSEN.write().unwrap() = path;
}

/// pandoc: the one set (`set_pandoc`), else on the PATH, else where Homebrew puts it (an app started from the Dock
/// has a short PATH).
pub fn pandoc() -> Result<PathBuf, String> {
    if let Some(chosen) = CHOSEN.read().unwrap().clone() {
        return if chosen.is_file() { Ok(chosen) } else { Err(format!("pandoc: {} (set in Settings) is not there", chosen.display())) };
    }
    let on_path = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
    let fixed = ["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from);
    on_path.into_iter().chain(fixed).map(|d| d.join("pandoc")).find(|p| p.is_file()).ok_or_else(|| "pandoc is not installed: brew install pandoc".to_string())
}

const CROSSREF: &str = include_str!("../lua/crossref.lua");

/// The cross-reference filter, written once to the temp folder (pandoc reads filters from files); None when it
/// cannot be written: the preview is then without it. The export to Word runs it too, so the numbers are the same.
pub fn crossref_filter() -> Option<&'static Path> {
    static FILE: OnceLock<Option<PathBuf>> = OnceLock::new();
    FILE.get_or_init(|| {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        CROSSREF.hash(&mut h);
        let path = std::env::temp_dir().join(format!("papers-write-crossref-{:016x}.lua", h.finish()));
        if std::fs::read_to_string(&path).is_ok_and(|s| s == CROSSREF) {
            return Some(path);
        }
        let part = path.with_extension(format!("{}.part", std::process::id()));
        std::fs::write(&part, CROSSREF).ok()?;
        std::fs::rename(&part, &path).ok()?;
        Some(path)
    })
    .as_deref()
}

/// pandoc run in `dir` with `args`, given `input` on its standard input; what it wrote, else what it said.
fn run_pandoc(args: &[&str], dir: &Path, input: &[u8]) -> Result<Vec<u8>, String> {
    let mut child = Command::new(pandoc()?)
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
    run_pandoc(&args.iter().map(String::as_str).collect::<Vec<_>>(), dir, markdown.as_bytes()).map(drop)
}

/// The .docx `to` of `markdown`: cross-references numbered as in the preview and citations resolved when the text
/// names a bibliography. What fails is dropped in turn, the filter first (an error that speaks of Lua or a filter),
/// then the citations (a bibliography that is missing or does not parse), with a note of it.
fn docx_of(markdown: &str, text: &str, dir: &Path, to: &Path) -> Result<Option<String>, String> {
    let (mut filter, mut citeproc, mut note) = (crossref_filter(), cites(text), None);
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

/// The .docx of `current` with its open suggestions and comments as tracked changes and comments, written to `to`.
pub fn export(doc: &Path, current: &str, to: &Path) -> Result<Exported, String> {
    let items = Review::load(doc).map_err(|e| e.to_string())?.items(current);
    let made = review::word::markdown_with_changes(current, &items);
    let dir = doc.parent().ok_or("no folder")?;
    let notes = docx_of(&made.markdown, current, dir, to)?.into_iter().collect();
    Ok(Exported { path: to.to_string_lossy().into_owned(), suggestions: made.suggestions, comments: made.comments, skipped: made.skipped, notes })
}

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
    run_pandoc(&args, dir, b"").map(|b| String::from_utf8_lossy(&b).into_owned())
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
    let filter = crossref_filter();
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
pub fn import(doc: &Path, current: &str, docx: &Path, also_untracked: bool, fallback_author: &str) -> Result<Imported, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pw-exchange-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// `theirs` as the suggestions of Jane Doe on `doc`.
    fn suggest(doc: &Path, current: &str, theirs: &str) {
        let jane = Person::new("Jane Doe", "");
        for s in review::compare::suggestions(current, theirs) {
            Review::add(doc, &jane, current, s).unwrap();
        }
    }

    #[test]
    fn the_document_goes_to_word_with_its_changes() {
        if pandoc().is_err() {
            return;
        }
        let dir = folder("word");
        let doc = dir.join("paper.md");
        let current = "The grounding line retreats rapidly when the bed slopes.\n";
        std::fs::write(&doc, current).unwrap();
        suggest(&doc, current, "The grounding line retreats slowly when the bed slopes.\n");
        let to = dir.join("paper.docx");
        let made = export(&doc, current, &to).unwrap();
        assert_eq!((made.suggestions, made.comments), (1, 0));
        let read = |mode: &str| {
            let out = Command::new(pandoc().unwrap()).arg(&to).args([&format!("--track-changes={mode}"), "-t", "plain", "--wrap=none"]).output().unwrap();
            String::from_utf8(out.stdout).unwrap()
        };
        assert!(read("reject").contains("retreats rapidly when"));
        assert!(read("accept").contains("retreats slowly when"));
    }

    #[test]
    fn cross_references_are_numbered_in_the_docx_as_in_the_preview() {
        if pandoc().is_err() {
            return;
        }
        let dir = folder("xref");
        let doc = dir.join("paper.md");
        let current = "See @fig:x and \\ref{eq:y}.\n\n![The loop](loop.png){#fig:x}\n\n\\begin{equation}\na = b \\label{eq:y}\n\\end{equation}\n";
        std::fs::write(&doc, current).unwrap();
        let to = dir.join("paper.docx");
        let made = export(&doc, current, &to).unwrap();
        assert!(made.notes.is_empty(), "{:?}", made.notes);
        let out = Command::new(pandoc().unwrap()).arg(&to).args(["-t", "plain", "--wrap=none"]).output().unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(text.contains("Fig.\u{a0}1") || text.contains("Fig. 1"), "{text}");
        assert!(!text.contains("fig:x?") && !text.contains("eq:y?") && !text.contains("@fig:x"), "{text}");
    }

    #[test]
    fn a_word_review_goes_out_and_comes_back_as_the_same_review() {
        if pandoc().is_err() {
            return;
        }
        let dir = folder("roundtrip");
        let doc = dir.join("paper.md");
        let current = "The *grounding line* retreats rapidly [@smith2020] when the bed slopes.\n\nThe ice shelf thins.\n";
        std::fs::write(&doc, current).unwrap();
        suggest(&doc, current, "The *grounding line* retreats slowly [@smith2020] when the bed slopes.\n\nThe ice shelf thickens.\n");
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
}
