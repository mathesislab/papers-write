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
}

/// The `.bib` files of the references of a Markdown file: those its front matter's `bibliography` names
/// (relative to the file's folder), then the papers library's, each once; one that is not there is left out.
pub fn sources_of(markdown_path: &Path, markdown_text: &str) -> Vec<PathBuf> {
    let folder = folder_of(markdown_path);
    let named = frontmatter::bibliography(markdown_text).into_iter().map(|n| folder.join(n)).collect::<Vec<_>>();
    let library = MachineConfig::load(None)
        .map_err(|e| e.message().to_string())
        .and_then(|machine| library_bib(&folder, machine))
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
}
