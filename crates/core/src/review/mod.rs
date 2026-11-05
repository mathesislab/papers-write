//! Suggestions and comments kept in files beside the document, each anchored to the version it was made on.
//!
//! - [`text`]: git's blob hash, UTF-16 positions, slugs and times.
//! - [`diff`]: the word-level difference between two versions, and positions carried through it.
//! - [`anchor`]: a record carried onto the current text (three-way merge, the quote when the base is lost).
//! - [`fold`]: all the records folded into items.
//! - [`ops`]: add, reply, decide, supersede and counter-suggest (appending); apply a suggestion.
//! - [`record`]: the records of a review file, and who writes it.
//! - [`store`]: the files on disk, appended to and never rewritten.

pub mod anchor;
pub mod diff;
pub mod fold;
pub mod ops;
pub mod record;
pub mod store;
pub mod text;

pub use anchor::{Anchor, Status};
pub use fold::{Item, State};
pub use ops::{NewRecord, apply, apply_all};
pub use record::{Decision, Kind, Person, Record};
pub use store::{Error, Result};
pub use text::{blob_hash, byte_of, utf16_of};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A review file that could not be read; the others load all the same.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct LoadError {
    pub file: PathBuf,
    pub message: String,
}

/// The records of one person's file.
#[derive(Clone, Debug)]
pub struct PersonFile {
    pub slug: String,
    pub person: Option<Person>,
    pub records: Vec<Record>,
}

/// The review of a document: every person's file read together, and the bases (read when needed).
pub struct Review {
    doc: PathBuf,
    pub files: Vec<PersonFile>,
    /// the files that could not be read
    pub errors: Vec<LoadError>,
    bases: Mutex<HashMap<String, Option<String>>>,
}

impl Review {
    /// Loads the review files of the document at `doc` (none yet: an empty review). A broken file is listed in
    /// `errors`; the others load.
    pub fn load(doc: &Path) -> Result<Review> {
        let dir = store::review_dir(doc);
        let mut review = Review { doc: doc.to_path_buf(), files: Vec::new(), errors: Vec::new(), bases: Mutex::default() };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(review);
        };
        let mut paths: Vec<PathBuf> =
            entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "toml")).collect();
        paths.sort();
        for path in paths {
            let slug = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            match store::parse(&path) {
                Ok(c) => review.files.push(PersonFile { slug, person: c.person, records: c.record }),
                Err(e) => review.errors.push(LoadError { file: path, message: e.to_string() }),
            }
        }
        Ok(review)
    }

    pub fn doc(&self) -> &Path {
        &self.doc
    }

    /// Every record of every file, in the order of the files.
    pub fn records(&self) -> impl Iterator<Item = &Record> {
        self.files.iter().flat_map(|f| f.records.iter())
    }

    /// The text of the base with `hash`, if its file is there (read once).
    pub fn base(&self, hash: &str) -> Option<String> {
        let mut cache = self.bases.lock().unwrap();
        cache.entry(hash.to_string()).or_insert_with(|| store::read_base(&self.doc, hash)).clone()
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fresh folder with a document file name inside; returns the document's path.
    pub fn doc(name: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("pw-review-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    use crate::review::record::{Kind, Record};
    use crate::review::text::{after, before};

    /// A suggestion or comment on the first occurrence of `needle` in `base`, with its quote.
    pub fn make(id: &str, kind: Kind, base: &str, needle: &str, insert: Option<&str>) -> Record {
        let start = if needle.is_empty() { 0 } else { base.find(needle).expect("needle in base") };
        let end = start + needle.len();
        let mut r = Record::blank(id.into(), kind);
        r.time = "2026-11-05T14:00:00Z".into();
        r.base = Some(crate::review::text::blob_hash(base));
        r.start = Some(start);
        r.end = Some(end);
        r.prefix = Some(before(base, start, 32).into());
        r.exact = Some(needle.into());
        r.suffix = Some(after(base, end, 32).into());
        r.insert = insert.map(Into::into);
        if kind == Kind::Comment {
            r.text = Some("a comment".into());
        }
        r
    }
}
