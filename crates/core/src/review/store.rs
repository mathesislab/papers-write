//! The review files on disk: `<doc folder>/.papers-write/review/<doc file name>/` with `bases/<hash>.<ext>` (the
//! text a record was made on, written once) and one `<slug>.toml` per person, only ever appended to.

use super::record::{FileContent, Person, Record};
use super::text::blob_hash;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum Error {
    Io(PathBuf, std::io::Error),
    /// a file that is not a valid review file, or a record that cannot be written
    Format(PathBuf, String),
    /// no record of that id (or not one of the right kind)
    NotFound(String),
    /// a request that does not make sense (a range outside the text, an edit of another person's record)
    Invalid(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(p, e) => write!(f, "{}: {e}", p.display()),
            Error::Format(p, m) => write!(f, "{}: {m}", p.display()),
            Error::NotFound(id) => write!(f, "no record {id}"),
            Error::Invalid(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// The folder of the review of the document at `doc`.
pub fn review_dir(doc: &Path) -> PathBuf {
    let parent = doc.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    parent.join(".papers-write").join("review").join(doc.file_name().unwrap_or_default())
}

/// The file of the base with `hash` of the document at `doc`.
pub fn base_path(doc: &Path, hash: &str) -> PathBuf {
    let name = match doc.extension() {
        Some(e) => format!("{hash}.{}", e.to_string_lossy()),
        None => hash.to_string(),
    };
    review_dir(doc).join("bases").join(name)
}

/// Writes `text` as a base (once: an existing one is left alone) and returns its hash.
pub fn write_base(doc: &Path, text: &str) -> Result<String> {
    let hash = blob_hash(text);
    let path = base_path(doc, &hash);
    if path.exists() {
        return Ok(hash);
    }
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| Error::Io(path.clone(), e))?;
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut f) => f.write_all(text.as_bytes()).map_err(|e| Error::Io(path, e))?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(Error::Io(path, e)),
    }
    Ok(hash)
}

pub fn read_base(doc: &Path, hash: &str) -> Option<String> {
    fs::read_to_string(base_path(doc, hash)).ok()
}

/// The file of the person with `slug`.
pub fn person_path(doc: &Path, slug: &str) -> PathBuf {
    review_dir(doc).join(format!("{slug}.toml"))
}

pub(crate) fn parse(path: &Path) -> Result<FileContent> {
    let s = fs::read_to_string(path).map_err(|e| Error::Io(path.to_path_buf(), e))?;
    toml::from_str(&s).map_err(|e| Error::Format(path.to_path_buf(), e.message().to_string()))
}

#[derive(Serialize)]
struct Header<'a> {
    person: &'a Person,
}

#[derive(Serialize)]
struct Appended<'a> {
    record: [&'a Record; 1],
}

/// Appends `record` to the file `path` of `me`, writing the header first when the file is new. Nothing already in
/// the file is touched: it is opened in append mode.
pub(crate) fn append(path: &Path, me: &Person, record: &Record) -> Result<()> {
    let fail = |e| Error::Io(path.to_path_buf(), e);
    fs::create_dir_all(path.parent().unwrap()).map_err(fail)?;
    let existing = fs::read(path).ok();
    let mut out = String::new();
    match &existing {
        None => {
            out.push_str(&toml::to_string(&Header { person: me }).map_err(|e| Error::Format(path.to_path_buf(), e.to_string()))?);
        }
        Some(b) if !b.is_empty() && !b.ends_with(b"\n") => out.push('\n'),
        _ => {}
    }
    out.push('\n');
    out.push_str(&toml::to_string(&Appended { record: [record] }).map_err(|e| Error::Format(path.to_path_buf(), e.to_string()))?);
    let mut f = OpenOptions::new().create(true).append(true).open(path).map_err(fail)?;
    f.write_all(out.as_bytes()).map_err(fail)
}

/// Writes the file `path` again with `records` only: the one thing but appending that is done to a person's file,
/// by its owner (Clean Up Review). The same bytes as appending them one by one would leave; written beside the file
/// and moved over it, so that the file is the old one or the new one, never half of either.
pub(crate) fn rewrite(path: &Path, person: &Person, records: &[&Record]) -> Result<()> {
    let fail = |e| Error::Io(path.to_path_buf(), e);
    let format = |e: toml::ser::Error| Error::Format(path.to_path_buf(), e.to_string());
    let mut out = toml::to_string(&Header { person }).map_err(format)?;
    for r in records {
        out.push('\n');
        out.push_str(&toml::to_string(&Appended { record: [r] }).map_err(format)?);
    }
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, out).map_err(fail)?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        fail(e)
    })
}

/// The `colour` (1 to 6, or none for the one from the name) of the person whose file is `path`, written in its
/// `[person]` header only: the records after the header are kept byte for byte, and the file is written beside and
/// moved over, so that it is the old one or the new one. The one other rewrite by an owner (with Clean Up). Whether
/// the file changed: false when it has no header or the colour is already that.
pub fn set_colour(path: &Path, colour: Option<u8>) -> Result<bool> {
    let fail = |e| Error::Io(path.to_path_buf(), e);
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(fail(e)),
    };
    let content = parse(path)?;
    let Some(person) = content.person else { return Ok(false) };
    let colour = colour.filter(|c| (1..=super::record::COLOURS).contains(c));
    if person.colour == colour {
        return Ok(false);
    }
    // the records start at the first `[[record]]` line; what is before it is the header
    let at = text.lines().scan(0usize, |pos, l| {
        let start = *pos;
        *pos += l.len() + 1;
        Some((start, l))
    }).find(|(_, l)| l.starts_with("[[record]]")).map(|(p, _)| p);
    let person = Person { colour, ..person };
    let mut out = toml::to_string(&Header { person: &person }).map_err(|e| Error::Format(path.to_path_buf(), e.to_string()))?;
    if let Some(at) = at {
        out.push('\n');
        out.push_str(&text[at..]);
    }
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, out).map_err(fail)?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        fail(e)
    })?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::record::Kind;
    use crate::review::{Review, testutil::doc};

    fn rec(id: &str, text: &str) -> Record {
        let mut r = Record::blank(id.into(), Kind::Comment);
        r.time = "2026-11-05T14:00:00Z".into();
        r.text = Some(text.into());
        r
    }

    #[test]
    fn layout_and_header() {
        let d = doc("paper.md");
        assert_eq!(review_dir(&d).file_name().unwrap(), "paper.md");
        let h = write_base(&d, "hello\n").unwrap();
        assert_eq!(h, "ce013625030ba8dba906f756967f9e9ca394464a");
        assert!(base_path(&d, &h).ends_with(".papers-write/review/paper.md/bases/ce013625030ba8dba906f756967f9e9ca394464a.md"));
        assert_eq!(read_base(&d, &h).as_deref(), Some("hello\n"));
        // written once
        std::fs::write(base_path(&d, &h), "changed").unwrap();
        write_base(&d, "hello\n").unwrap();
        assert_eq!(read_base(&d, &h).as_deref(), Some("changed"));
    }

    #[test]
    fn the_colour_is_in_the_header_and_the_records_are_kept_byte_for_byte() {
        let d = doc("paper.md");
        let path = person_path(&d, "jane-doe");
        let me = Person::new("Jane Doe", "jane@example.org");
        append(&path, &me, &rec("jane-doe-1", "one")).unwrap();
        append(&path, &me, &rec("jane-doe-2", "two \"quoted\"")).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        let records = &before[before.find("[[record]]").unwrap()..];
        assert!(!before.contains("colour"));
        assert!(set_colour(&path, Some(4)).unwrap());
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.starts_with("[person]\nname = \"Jane Doe\"\nemail = \"jane@example.org\"\ncolour = 4\n\n[[record]]"), "{after}");
        assert!(after.ends_with(records));
        // the same again changes nothing; none takes it out; 9 is none
        assert!(!set_colour(&path, Some(4)).unwrap());
        assert!(set_colour(&path, Some(9)).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        let review = Review::load(&d).unwrap();
        assert_eq!(review.files[0].person.as_ref().unwrap().colour, None);
        assert_eq!(review.files[0].records.len(), 2);
        // no file, nothing
        assert!(!set_colour(&person_path(&d, "nobody"), Some(2)).unwrap());
        // a file made with a colour has it from the start
        let alex = Person::new("Alex", "").with_colour(Some(2));
        append(&person_path(&d, "alex"), &alex, &rec("alex-1", "x")).unwrap();
        assert_eq!(Review::load(&d).unwrap().files[0].person.as_ref().unwrap().colour, Some(2));
    }

    #[test]
    fn append_never_rewrites() {
        let d = doc("paper.md");
        let me = Person::new("Jane Doe", "jane@example.org");
        let path = person_path(&d, "jane-doe");
        append(&path, &me, &rec("jane-doe-1", "first \"quoted\"\nline")).unwrap();
        let first = std::fs::read(&path).unwrap();
        let text = String::from_utf8(first.clone()).unwrap();
        assert!(text.starts_with("[person]\nname = \"Jane Doe\"\nemail = \"jane@example.org\"\n"), "{text}");
        assert_eq!(text.matches("[[record]]").count(), 1);
        append(&path, &me, &rec("jane-doe-2", "second")).unwrap();
        let second = std::fs::read(&path).unwrap();
        assert_eq!(&second[..first.len()], &first[..], "the bytes before stay identical");
        assert_eq!(String::from_utf8(second).unwrap().matches("[[record]]").count(), 2);
        let r = Review::load(&d).unwrap();
        let ids: Vec<_> = r.records().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["jane-doe-1", "jane-doe-2"]);
        assert_eq!(r.records().next().unwrap().text.as_deref(), Some("first \"quoted\"\nline"));
        assert_eq!(r.files[0].person.as_ref().unwrap().name, "Jane Doe");
    }

    #[test]
    fn append_after_a_file_without_final_newline() {
        let d = doc("paper.md");
        let me = Person::new("A", "");
        let path = person_path(&d, "a");
        append(&path, &me, &rec("a-1", "x")).unwrap();
        let mut b = std::fs::read(&path).unwrap();
        while b.ends_with(b"\n") {
            b.pop();
        }
        std::fs::write(&path, &b).unwrap();
        append(&path, &me, &rec("a-2", "y")).unwrap();
        assert_eq!(Review::load(&d).unwrap().records().count(), 2);
    }

    #[test]
    fn two_branches_two_files_and_a_broken_one() {
        // what a git merge leaves: each person's file, none conflicting
        let d = doc("paper.md");
        append(&person_path(&d, "jane-doe"), &Person::new("Jane Doe", ""), &rec("jane-doe-1", "from jane")).unwrap();
        append(&person_path(&d, "alex"), &Person::new("Alex", ""), &rec("alex-1", "from alex")).unwrap();
        std::fs::write(person_path(&d, "broken"), "[[record]\nid = ").unwrap();
        let r = Review::load(&d).unwrap();
        assert_eq!(r.records().count(), 2);
        assert_eq!(r.errors.len(), 1);
        assert!(r.errors[0].file.ends_with("broken.toml"));
    }

    #[test]
    fn no_review_folder_is_an_empty_review() {
        let d = doc("paper.md");
        let r = Review::load(&d).unwrap();
        assert_eq!(r.records().count(), 0);
        assert!(r.errors.is_empty());
        assert_eq!(r.base("nothing"), None);
    }
}
