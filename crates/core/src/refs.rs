//! The references of a document: the `.bib` files its front matter names and the `.bib` of the papers library
//! (a `.papers/config.json` in the document's folder or above it, else papers' default library), loaded as one
//! library (papers-core) and searched as Papers Desktop searches its own.
//!
//! Watching the files is left to the app: [`References::sources`] is what it watches (papers-core's `BibWatch`),
//! loading them again when one changes.

use std::path::{Path, PathBuf};

pub use papers_core::Found;
use papers_core::{Item, Library, Matching};
use papers_rs::config::{MachineConfig, find_config};
use serde::Serialize;

use crate::frontmatter;
use crate::project::Project;

/// The references of a document, as they were read.
pub struct References {
    sources: Vec<PathBuf>,
    /// the library of the files, or what went wrong reading them (a file that does not parse): the page
    /// shows it, and the files are watched all the same, for the one that is mended
    library: Result<Library, String>,
}

/// An entry of the references: the item as the panel shows it, and its BibTeX as written in its file.
#[derive(Debug, Clone, Serialize)]
pub struct RefEntry {
    pub item: Item,
    pub bibtex: String,
}

impl References {
    /// The references of these `.bib` files, in order. A file that does not parse is kept as the error
    /// ([`References::error`]), with the references empty.
    pub fn load(sources: Vec<PathBuf>) -> References {
        let library = Library::load(&sources).map_err(|e| e.to_string());
        References { sources, library }
    }

    /// The `.bib` files read (to be watched).
    pub fn sources(&self) -> &[PathBuf] {
        &self.sources
    }

    /// What went wrong reading the files, if something did.
    pub fn error(&self) -> Option<&str> {
        self.library.as_ref().err().map(String::as_str)
    }

    /// What the files have that could not be read, and was left out (a field of an entry, or an entry), said.
    pub fn dropped(&self) -> Vec<String> {
        self.library().map(|l| l.dropped().iter().map(|d| d.to_string()).collect()).unwrap_or_default()
    }

    /// The library, if the files were read.
    pub fn library(&self) -> Option<&Library> {
        self.library.as_ref().ok()
    }

    /// The matches of `query`: words, field filters, a citation key; a word matches from the start of a word of
    /// the entry. The first `limit` come back, with their marks. With nothing typed, the entries, newest first.
    pub fn search(&self, query: &str, limit: usize) -> Found {
        let Some(library) = self.library() else {
            return Found::default();
        };
        if !query.trim().is_empty() {
            return library.search(query, Matching::WordStart, limit);
        }
        let ids = library.matching("", Matching::WordStart);
        Found {
            total: ids.len(),
            hits: ids.iter().take(limit).filter_map(|id| library.hit(id)).collect(),
            ..Found::default()
        }
    }

    /// The entries closest to a text (a reference as written in a paper, a title).
    pub fn closest(&self, text: &str, limit: usize) -> Found {
        self.library().map(|l| l.closest(text, limit)).unwrap_or_default()
    }

    /// The entry of citation key `key`, the first of the files when several have it.
    pub fn entry(&self, key: &str) -> Option<RefEntry> {
        let library = self.library()?;
        let item = library.items().iter().find(|i| i.key == key)?;
        let bibtex = library.raw(&item.id).map(str::to_string).or_else(|| library.bibtex(&item.id))?;
        Some(RefEntry { item: item.clone(), bibtex })
    }

    /// Whether a file of the references has an entry of citation key `key`.
    pub fn has(&self, key: &str) -> bool {
        self.library().is_some_and(|l| l.items().iter().any(|i| i.key == key))
    }

    /// Of `keys`, those no file of the references has, in order, each once. Empty when the files could not be read:
    /// nothing can then be said to be missing.
    pub fn missing(&self, keys: &[String]) -> Vec<String> {
        let Some(library) = self.library() else {
            return Vec::new();
        };
        let known: std::collections::HashSet<&str> = library.items().iter().map(|i| i.key.as_str()).collect();
        let mut seen = std::collections::HashSet::new();
        keys.iter().filter(|k| !known.contains(k.as_str()) && seen.insert(k.as_str())).cloned().collect()
    }
}

/// The names of the `.bib` files a document says it uses, as written: the front matter's `bibliography` of Markdown, the
/// `\bibliography` and `\addbibresource` of a `.tex` file and of the files it `\input`s (chosen by the extension of
/// `path`).
pub fn bibliography_of(path: &Path, text: &str) -> Vec<String> {
    if crate::latex::is_latex(path) { crate::latex::bibliography_with_inputs(path, text) } else { frontmatter::bibliography(text) }
}

/// The `.bib` files a document uses: those it names (beside it), else, for a chapter naming none, its main document's
/// (`main_bibliography`). What its citations are checked against, completed from, and added to.
pub fn document_bibliography(path: &Path, text: &str) -> Vec<PathBuf> {
    let folder = folder_of(path);
    let own: Vec<PathBuf> = bibliography_of(path, text).into_iter().map(|n| folder.join(n)).collect();
    if own.is_empty() { main_bibliography(path, text) } else { own }
}

/// The `.bib` files of the project's main document (`write.toml`), for a file of the project that names none itself
/// (a chapter, whose bibliography its main document names, as texmark builds it): those that are there. None for
/// the main document itself, a file outside a project, or one that names its own.
pub fn main_bibliography(path: &Path, text: &str) -> Vec<PathBuf> {
    if path.as_os_str().is_empty() || !bibliography_of(path, text).is_empty() {
        return Vec::new();
    }
    let Some(main) = Project::of(&folder_of(path)).ok().flatten().and_then(|p| p.main()) else {
        return Vec::new();
    };
    let same = |a: &Path, b: &Path| a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(a, b)| a == b);
    if same(&main, path) {
        return Vec::new();
    }
    let Ok(main_text) = std::fs::read_to_string(&main) else {
        return Vec::new();
    };
    let folder = folder_of(&main);
    bibliography_of(&main, &main_text).into_iter().map(|n| folder.join(n)).filter(|f| f.is_file()).collect()
}

/// The files of the whole document `path` is part of (texmark's thesis or book: the main document lists its chapters
/// in `chapters:`): the main document, then the chapters in order, those that are there. None when `path` is neither
/// a chapter of its project's main document nor that document with chapters.
pub fn document_files(path: &Path) -> Option<Vec<PathBuf>> {
    if path.as_os_str().is_empty() {
        return None;
    }
    let main = Project::of(&folder_of(path)).ok().flatten()?.main()?;
    let same = |a: &Path, b: &Path| a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(a, b)| a == b);
    let dir = folder_of(&main);
    let chapters: Vec<PathBuf> = frontmatter::chapters(&std::fs::read_to_string(&main).ok()?).iter().map(|c| dir.join(c)).collect();
    // a chapter, or the main document itself when it has chapters
    if chapters.is_empty() || !(same(&main, path) || chapters.iter().any(|c| same(c, path))) {
        return None;
    }
    Some(std::iter::once(main).chain(chapters.into_iter().filter(|c| c.is_file())).collect())
}

/// The text of a whole document as its files are on disk (`document_files`): the main document's front matter and
/// text, then each chapter's text (a chapter's own front matter left out), for its labels to be numbered as one.
pub fn whole_text(files: &[PathBuf]) -> String {
    let mut whole = String::new();
    for (i, file) in files.iter().enumerate() {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        let body = if i == 0 { text.as_str() } else { &text[frontmatter::body_start(&text)..] };
        // where each file starts, for the headings to be told by file (the filter's PAPERS_WRITE_HEADINGS_OUT); the
        // main document's after its front matter
        let at = if i == 0 { frontmatter::body_start(body) } else { 0 };
        whole.push_str(&body[..at]);
        whole.push_str(&format!("\n\n<!-- papers-write-file {i} -->\n\n"));
        whole.push_str(&body[at..]);
        whole.push_str("\n\n");
    }
    whole
}

/// What tells whether a document's files changed: each one's time of change and size.
pub fn files_stamp(files: &[PathBuf]) -> Vec<(PathBuf, Option<std::time::SystemTime>, u64)> {
    files
        .iter()
        .map(|f| {
            let meta = std::fs::metadata(f).ok();
            (f.clone(), meta.as_ref().and_then(|m| m.modified().ok()), meta.map_or(0, |m| m.len()))
        })
        .collect()
}

/// The `.bib` files of the references of a Markdown or LaTeX file: those it names
/// (relative to the file's folder), then the papers library's, each once; one that is not there is left out.
pub fn sources_of(markdown_path: &Path, markdown_text: &str) -> Vec<PathBuf> {
    sources_in(markdown_path, markdown_text, project_library_of(markdown_path).as_deref())
}

/// The library a document's project (`write.toml`) says to search, by its path; none when the document is in no
/// project, or the project sets none, or its `write.toml` cannot be read (the papers library is searched then).
pub fn project_library_of(document: &Path) -> Option<PathBuf> {
    if document.as_os_str().is_empty() {
        return None;
    }
    Project::of(&folder_of(document)).ok().flatten()?.library()
}

/// [`sources_of`] with the library of the document's project given (`project_library`, as `write.toml` says it:
/// [`project_library_bib`]): searched instead of the papers library found from the folder.
pub fn sources_in(markdown_path: &Path, markdown_text: &str, project_library: Option<&Path>) -> Vec<PathBuf> {
    let folder = folder_of(markdown_path);
    let named = document_bibliography(markdown_path, markdown_text);
    let library = MachineConfig::load(None)
        .map_err(|e| e.message().to_string())
        .and_then(|machine| match project_library {
            Some(library) => project_library_bib(library, machine),
            None => library_bib(&folder, machine),
        })
        // the document's own files are still searched when papers' configuration cannot be read
        .unwrap_or_else(|e| {
            eprintln!("the papers library: {e}");
            None
        });
    sources(&named, library)
}

/// The folder a Markdown file is in.
pub(crate) fn folder_of(markdown_path: &Path) -> PathBuf {
    markdown_path.parent().map(Path::to_path_buf).filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| PathBuf::from("."))
}

/// The `.bib` file of the papers library for a folder: of the library whose `.papers/config.json` is in the
/// folder or above it, else of the default library of `machine`. None when there is none.
pub fn library_bib(folder: &Path, machine: MachineConfig) -> Result<Option<PathBuf>, String> {
    let config = find_config(&folder.to_string_lossy(), machine).map_err(|e| e.message().to_string())?;
    Ok(config.bibtex.filter(|b| !b.is_empty()).map(PathBuf::from))
}

/// The `.bib` file of the library a project names: the file itself when it is a `.bib` path, the `.bib` of the
/// papers library when it is a folder with a `.papers/config.json` (a folder without one is an error: the library
/// above it is not the project's).
pub fn project_library_bib(library: &Path, machine: MachineConfig) -> Result<Option<PathBuf>, String> {
    if !library.is_dir() {
        return Ok(Some(library.to_path_buf()));
    }
    if !library.join(".papers/config.json").is_file() {
        return Err(format!("{}: not a papers library (no .papers/config.json)", library.display()));
    }
    library_bib(library, machine)
}

/// The files to read: those of the document, then the library's, each once (a document in the library folder
/// names its `.bib` too); one that is not there is left out.
fn sources(document: &[PathBuf], library: Option<PathBuf>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut real: Vec<PathBuf> = Vec::new();
    for bib in document.iter().chain(&library) {
        let Ok(canonical) = bib.canonicalize() else {
            continue;
        };
        if canonical.is_file() && !real.contains(&canonical) {
            real.push(canonical);
            found.push(bib.clone());
        }
    }
    found
}

/// A reference offered to complete a citation key as it is typed.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub item: Item,
    /// in the document's own bibliography (else in the library only: citing it copies it there)
    pub in_bib: bool,
}

/// The references offered for a citation key typed so far (`prefix`), at most `limit`: those of the document's own
/// bibliography (`own`, its `.bib` files) first, the keys that start with `prefix` before the other matches; then
/// the library's keys that start with `prefix`, newest first; then the library's other matches (words from their
/// start), as `search` ranks them. Each key once. Searching by words alone lost the document's own entries among the
/// many words that start alike ("jou": every journal).
pub fn candidates(references: &References, prefix: &str, own: &[PathBuf], limit: usize) -> Vec<Candidate> {
    let Some(library) = references.library() else {
        return Vec::new();
    };
    let lower = prefix.to_lowercase();
    let starts = |item: &Item| item.key.to_lowercase().starts_with(&lower);
    let is_own = |item: &Item| own.contains(&item.bib);
    let matched: Vec<&Item> = if prefix.trim().is_empty() {
        library.items().iter().collect()
    } else {
        let found = library.search(prefix, Matching::WordStart, library.items().len());
        let ids: std::collections::HashSet<&str> = found.hits.iter().map(|h| h.item.id.as_str()).collect();
        library.items().iter().filter(|i| starts(i) || ids.contains(i.id.as_str())).collect()
    };
    let newest = |a: &&Item, b: &&Item| b.year.cmp(&a.year);
    let mut own_first: Vec<&Item> = matched.iter().copied().filter(|i| is_own(i)).collect();
    own_first.sort_by(|a, b| starts(b).cmp(&starts(a)).then_with(|| newest(a, b)));
    let mut keyed: Vec<&Item> = matched.iter().copied().filter(|i| !is_own(i) && starts(i)).collect();
    keyed.sort_by(newest);
    let rest: Vec<&Item> = if prefix.trim().is_empty() {
        Vec::new()
    } else {
        let found = library.search(prefix, Matching::WordStart, limit.saturating_mul(4).max(limit));
        let ids: Vec<String> = found.hits.iter().map(|h| h.item.id.clone()).collect();
        ids.iter().filter_map(|id| library.items().iter().find(|i| &i.id == id)).filter(|i| !is_own(i) && !starts(i)).collect()
    };
    let mut seen = std::collections::HashSet::new();
    own_first
        .into_iter()
        .chain(keyed)
        .chain(rest)
        .filter(|i| seen.insert(i.key.clone()))
        .take(limit)
        .map(|i| Candidate { item: i.clone(), in_bib: is_own(i) })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const REFS: &str = "\
@article{robinson2012,
  author = {Robinson, Alexander and Calov, Reinhard and Ganopolski, Andrey},
  title = {Multistability and critical thresholds of the Greenland ice sheet},
  journal = {Nature Climate Change},
  year = {2012},
}

@article{feldmann2017,
  author = {Feldmann, Johannes and Levermann, Anders},
  title = {From cyclic ice streaming to Heinrich-like events},
  journal = {The Cryosphere},
  year = {2017},
}
";

    #[test]
    fn the_documents_own_entries_come_first_then_keys_then_words() {
        let dir = folder("candidates");
        let own = bib(&dir, "own.bib", "@article{joughin2019,\n author = {Joughin, Ian},\n title = {Friction},\n journal = {GRL},\n year = {2019},\n}\n");
        // a library of many newer entries whose words start alike ("Journal"), and a key that starts alike
        let mut lib = String::new();
        for i in 0..40 {
            lib.push_str(&format!("@article{{other{i},\n author = {{Other, A.}},\n title = {{Thing {i}}},\n journal = {{Journal of Glaciology}},\n year = {{2026}},\n}}\n\n"));
        }
        lib.push_str("@article{jourdain2025,\n author = {Jourdain, N.},\n title = {Melt},\n journal = {TC},\n year = {2025},\n}\n");
        let library = bib(&dir, "library.bib", &lib);
        let refs = References::load(vec![own.clone(), library]);
        let got = candidates(&refs, "jou", std::slice::from_ref(&own), 10);
        let keys: Vec<&str> = got.iter().map(|c| c.item.key.as_str()).collect();
        assert_eq!(&keys[..2], ["joughin2019", "jourdain2025"]);
        assert!(got[0].in_bib && !got[1].in_bib);
        assert_eq!(keys.len(), 10);
    }

    #[test]
    fn keys_no_file_has_are_missing() {
        let dir = folder("missing");
        let refs = References::load(vec![bib(&dir, "a.bib", REFS)]);
        let keys = ["robinson2012", "nobody1999", "feldmann2017", "nobody1999", "other"].map(String::from);
        assert_eq!(refs.missing(&keys), ["nobody1999", "other"]);
        // a file that does not parse: nothing is said to be missing
        let broken = References::load(vec![bib(&dir, "b.bib", "@article{x, title = {unclosed")]);
        assert!(broken.error().is_some() && broken.missing(&keys).is_empty());
    }

    /// An empty folder of its own for a test.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("write-core-refs-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn bib(folder: &Path, name: &str, text: &str) -> PathBuf {
        let path = folder.join(name);
        fs::write(&path, text).unwrap();
        path
    }

    fn keys(found: &Found) -> Vec<&str> {
        found.hits.iter().map(|h| h.item.key.as_str()).collect()
    }

    #[test]
    fn a_papers_library_in_the_folder_or_above_gives_its_bib() {
        let dir = folder("library");
        let library = dir.join("library");
        fs::create_dir_all(library.join(".papers")).unwrap();
        fs::create_dir_all(library.join("papers/one")).unwrap();
        fs::write(library.join(".papers/config.json"), r#"{"bibtex": "papers.bib", "version": 3}"#).unwrap();
        let papers = bib(&library, "papers.bib", REFS);
        for folder in [library.clone(), library.join("papers/one")] {
            let found = library_bib(&folder, MachineConfig::empty()).unwrap();
            assert_eq!(found.unwrap().canonicalize().unwrap(), papers.canonicalize().unwrap());
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_sources_are_the_documents_bib_files_then_the_librarys_each_once_and_all_there() {
        let dir = folder("sources");
        let (a, b) = (bib(&dir, "a.bib", ""), bib(&dir, "b.bib", ""));
        let gone = dir.join("gone.bib");
        let project = [a.clone(), gone.clone(), b.clone(), a.clone()];
        assert_eq!(sources(&project, None), [a.clone(), b.clone()]);
        let library = bib(&dir, "library.bib", "");
        assert_eq!(sources(&project, Some(library.clone())), [a.clone(), b.clone(), library.clone()]);
        assert_eq!(sources(&[library.clone(), a.clone()], Some(library.clone())), [library, a]);
        assert_eq!(sources(&project, Some(gone)), [project[0].clone(), b]);
        assert!(sources(&[], None).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_project_library_is_a_bib_or_a_papers_folder_else_none() {
        let dir = folder("project-library");
        let single = bib(&dir, "mine.bib", REFS);
        assert_eq!(project_library_bib(&single, MachineConfig::empty()).unwrap(), Some(single.clone()));
        let library = dir.join("lib");
        fs::create_dir_all(library.join(".papers")).unwrap();
        fs::write(library.join(".papers/config.json"), r#"{"bibtex": "papers.bib", "version": 3}"#).unwrap();
        let papers = bib(&library, "papers.bib", REFS);
        let got = project_library_bib(&library, MachineConfig::empty()).unwrap().unwrap();
        assert_eq!(got.canonicalize().unwrap(), papers.canonicalize().unwrap());
        // a folder that is no library (even inside one) is an error, not the library above it
        fs::create_dir_all(library.join("notes")).unwrap();
        assert!(project_library_bib(&library.join("notes"), MachineConfig::empty()).unwrap_err().contains("not a papers library"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_document_in_a_project_with_a_library_searches_it_beside_its_own_bibliography() {
        let dir = folder("project-sources");
        fs::create_dir_all(dir.join("sections")).unwrap();
        let (own, shared) = (bib(&dir.join("sections"), "own.bib", REFS), bib(&dir, "shared.bib", REFS));
        let doc = dir.join("sections/intro.md");
        let text = "---\nbibliography: own.bib\n---\ntext";
        // no project: no project library
        assert_eq!(project_library_of(&doc), None);
        fs::write(dir.join("write.toml"), "name = \"p\"\n").unwrap();
        assert_eq!(project_library_of(&doc), None);
        // relative to the project's root, not the document's folder
        fs::write(dir.join("write.toml"), "library = \"shared.bib\"\n").unwrap();
        assert_eq!(project_library_of(&doc), Some(shared.clone()));
        let library = project_library_of(&doc);
        assert_eq!(sources_in(&doc, text, library.as_deref()), [own.clone(), shared.clone()]);
        // an absolute path
        fs::write(dir.join("write.toml"), format!("library = {:?}\n", shared.display().to_string())).unwrap();
        assert_eq!(project_library_of(&doc), Some(shared.clone()));
        // untitled: no project
        assert_eq!(project_library_of(Path::new("")), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_sources_of_a_markdown_file_follow_its_front_matter() {
        let dir = folder("markdown");
        let (a, b) = (bib(&dir, "a.bib", REFS), bib(&dir, "b.bib", ""));
        let text = "---\nbibliography: [a.bib, b.bib, missing.bib]\n---\ntext";
        let found = sources_of(&dir.join("paper.md"), text);
        // the library of the machine may follow
        assert_eq!(found[..2], [a, b]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_references_are_searched_by_the_start_of_a_word() {
        let dir = folder("search");
        let refs = References::load(vec![bib(&dir, "refs.bib", REFS)]);
        assert_eq!(refs.sources().len(), 1);
        assert!(refs.error().is_none());
        assert_eq!(keys(&refs.search("robin", 10)), ["robinson2012"]);
        assert_eq!(keys(&refs.search("ice", 10)), ["feldmann2017", "robinson2012"]);
        assert_eq!(refs.search("ice", 1).total, 2);
        assert_eq!(refs.search("ice", 1).hits.len(), 1);
        assert_eq!(refs.search("stability", 10).total, 0);
        assert_eq!(keys(&refs.search("year:2017", 10)), ["feldmann2017"]);
        // nothing typed: all of them, newest first
        assert_eq!(keys(&refs.search(" ", 10)), ["feldmann2017", "robinson2012"]);
        assert_eq!(refs.search("", 1).total, 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_reference_closest_to_a_text_is_found() {
        let dir = folder("closest");
        let refs = References::load(vec![bib(&dir, "refs.bib", REFS)]);
        let text = "Robinson, A., Calov, R. and Ganopolski, A.: Multistability and critical thresholds of the \
                    Greenland ice sheet, Nat. Clim. Change, 2012.";
        assert_eq!(keys(&refs.closest(text, 5)), ["robinson2012"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_entry_comes_with_its_bibtex_as_written() {
        let dir = folder("entry");
        let first = bib(&dir, "a.bib", "@article{k1,\n  title = {First},\n  year = {2000},\n}\n");
        let second = bib(&dir, "b.bib", "@article{k1,\n  title = {Second},\n}\n\n@book{k2, title={Book}}\n");
        let refs = References::load(vec![first, second]);
        let entry = refs.entry("k1").unwrap();
        assert_eq!(entry.item.title, "First");
        assert!(entry.bibtex.starts_with("@article{k1,"), "{}", entry.bibtex);
        assert!(entry.bibtex.contains("First") && !entry.bibtex.contains("Second"));
        assert_eq!(refs.entry("k2").unwrap().item.kind, "book");
        assert!(refs.entry("nope").is_none());
        assert!(refs.has("k2") && !refs.has("nope"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bib_file_that_does_not_parse_is_an_error_naming_it() {
        let dir = folder("broken");
        let refs = References::load(vec![bib(&dir, "broken.bib", "@article{robinson2012, title = {open")]);
        assert!(refs.error().unwrap().contains("broken.bib"));
        assert_eq!(refs.sources().len(), 1);
        assert_eq!(refs.search("x", 5).total, 0);
        assert!(refs.entry("robinson2012").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_the_page_gets_serializes() {
        fn serializes<T: Serialize>() {}
        serializes::<Found>();
        serializes::<RefEntry>();
    }

    #[test]
    fn a_chapter_takes_the_bibliography_of_the_main_document() {
        let root = folder("chapter");
        fs::write(root.join("write.toml"), "main = \"thesis.md\"\n").unwrap();
        fs::write(root.join("references.bib"), REFS).unwrap();
        let main = "---\nbibliography: references.bib\nchapters:\n  - intro.md\n---\n";
        fs::write(root.join("thesis.md"), main).unwrap();
        fs::create_dir_all(root.join("parts")).unwrap();
        let chapter = "# Intro\n\nAs shown [@robinson2012].\n";
        assert_eq!(main_bibliography(&root.join("parts/intro.md"), chapter), vec![root.join("references.bib")]);
        // the main document, and a file that names its own, keep theirs
        assert!(main_bibliography(&root.join("thesis.md"), main).is_empty());
        assert!(main_bibliography(&root.join("other.md"), "---\nbibliography: other.bib\n---\n").is_empty());
        // checked against, completed from and added to: the main document's
        assert_eq!(document_bibliography(&root.join("parts/intro.md"), chapter), vec![root.join("references.bib")]);
        assert_eq!(document_bibliography(&root.join("other.md"), "---\nbibliography: other.bib\n---\n"), vec![root.join("other.bib")]);
    }

    #[test]
    fn the_files_of_a_chapter_s_whole_document() {
        let root = folder("whole");
        fs::write(root.join("write.toml"), "main = \"thesis.md\"\n").unwrap();
        fs::write(root.join("thesis.md"), "---\ntitle: T\nchapters:\n  - one.md\n  - two.md\n---\n\nAbstract.\n").unwrap();
        fs::write(root.join("one.md"), "---\nx: 1\n---\n# One {#sec:one}\n").unwrap();
        fs::write(root.join("two.md"), "# Two {#sec:two}\n").unwrap();
        let files = document_files(&root.join("two.md")).unwrap();
        assert_eq!(files, [root.join("thesis.md"), root.join("one.md"), root.join("two.md")]);
        let whole = whole_text(&files);
        assert!(whole.starts_with("---\ntitle: T"));
        assert!(!whole.contains("x: 1"));
        assert!(whole.find("Abstract.").unwrap() < whole.find("# One").unwrap());
        assert!(whole.find("# One").unwrap() < whole.find("# Two").unwrap());
        assert!(whole.contains("<!-- papers-write-file 2 -->"));
        // the main document has the whole document too
        assert_eq!(document_files(&root.join("thesis.md")).unwrap().len(), 3);
        assert!(document_files(&root.join("other.md")).is_none());
    }
}
