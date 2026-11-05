//! A review sent to someone who has the app but not git: one `.review` file (a zip).
//!
//! ```text
//! manifest.toml        kind = "send" | "back", document, base (hash of the text sent), round, made, [from]
//! document/<name>      the text as sent, and the files it shows (figures, bibliographies) at their relative paths
//! review/              the document's review folder: the person files and `bases/`
//! ```
//!
//! - `send`: the author's document, what it shows and the whole review of it, so the recipient's records anchor on
//!   exactly the text the author sent.
//! - `back`: the recipient's own person file and the bases their records need, nothing else.
//!
//! [`send`] makes the first, [`open`] and [`unpack`] open it as a document with its review, [`send_back`] makes the
//! second from that document's review, and [`merge`] joins one to the review of the author's document: bases are
//! copied if missing, a person's file is copied or has the records its ids lack appended (never rewritten).
//!
//! The zip is the system's `ditto` (macOS): the two functions [`zip`] and [`unzip`] are all that knows.

use super::record::{Decision, Kind, Person, Record};
use super::store::{self, Error, Result};
use super::text::{blob_hash, now};
use crate::frontmatter;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The extension of the file.
pub const EXTENSION: &str = "review";

/// Which way a package goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageKind {
    /// from the author: the document to review
    Send,
    /// from the reviewer: their review of it
    Back,
}

/// `manifest.toml`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub kind: PackageKind,
    /// the file name of the document (`paper.md`)
    pub document: String,
    /// the blob hash of the text the author sent
    pub base: String,
    pub round: String,
    /// RFC 3339, UTC
    pub made: String,
    /// who made the package
    pub from: Person,
}

// ---- the zip

/// The folder `folder` as the zip `to` (the folder itself is the one entry at the top).
fn zip(folder: &Path, to: &Path) -> Result<()> {
    let _ = fs::remove_file(to);
    ditto(&["-c", "-k", "--sequesterRsrc", "--keepParent"], folder, to)
}

/// The zip `file` unpacked into the existing folder `into`.
fn unzip(file: &Path, into: &Path) -> Result<()> {
    ditto(&["-x", "-k"], file, into)
}

fn ditto(flags: &[&str], from: &Path, to: &Path) -> Result<()> {
    let out = Command::new("ditto").args(flags).arg(from).arg(to).output().map_err(|e| Error::Invalid(format!("ditto did not start: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Error::Invalid(format!("ditto: {}", String::from_utf8_lossy(&out.stderr).trim())))
    }
}

// ---- folders

/// A folder made for a while, removed when it goes out of scope.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Scratch> {
        static N: AtomicUsize = AtomicUsize::new(0);
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("pw-package-{}-{nonce}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        fs::create_dir_all(&dir).map_err(|e| Error::Io(dir.clone(), e))?;
        Ok(Scratch(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |e| Error::Io(path.to_path_buf(), e)
}

/// `from` copied to `to` with what is in it (symbolic links are left out).
fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to).map_err(io(to))?;
    for entry in fs::read_dir(from).map_err(io(from))? {
        let entry = entry.map_err(io(from))?;
        let kind = entry.file_type().map_err(io(from))?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target).map_err(io(&target))?;
        }
    }
    Ok(())
}

fn copy_file(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to.parent().unwrap()).map_err(io(to))?;
    fs::copy(from, to).map_err(io(to)).map(drop)
}

/// A file name that stays in its folder.
fn plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<()> {
    let path = dir.join("manifest.toml");
    let text = toml::to_string(manifest).map_err(|e| Error::Format(path.clone(), e.to_string()))?;
    fs::write(&path, text).map_err(io(&path))
}

fn read_manifest(dir: &Path) -> Result<Manifest> {
    let path = dir.join("manifest.toml");
    let text = fs::read_to_string(&path).map_err(|_| Error::Format(path.clone(), "not a review file: no manifest".into()))?;
    let m: Manifest = toml::from_str(&text).map_err(|e| Error::Format(path.clone(), e.message().to_string()))?;
    if !plain_name(&m.document) {
        return Err(Error::Format(path, format!("the document's name {:?} is not a file name", m.document)));
    }
    if m.base.len() != 40 || !m.base.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Format(path, "the base is not a hash".into()));
    }
    Ok(m)
}

// ---- what a document shows

/// The paths in `text` that the document needs next to it: `![…](path)`, `<img src>`, `\includegraphics{path}`.
fn shown_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    // ![alt](dest "title")
    let mut rest = text;
    while let Some(i) = rest.find("![") {
        rest = &rest[i + 2..];
        let mut depth = 1;
        let mut end = None;
        for (j, c) in rest.char_indices() {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(j);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(j) = end else { continue };
        if let Some(dest) = rest[j + 1..].strip_prefix('(') {
            let dest = dest.trim_start();
            let path = match dest.strip_prefix('<') {
                Some(angled) => angled.split('>').next().unwrap_or(""),
                None => dest.split(|c: char| c.is_whitespace() || c == ')').next().unwrap_or(""),
            };
            out.push(path.to_string());
        }
    }
    // <img src="path">
    let mut rest = text;
    while let Some(i) = rest.find("<img") {
        rest = &rest[i + 4..];
        let tag = rest.split('>').next().unwrap_or("");
        if let Some(k) = tag.find("src=") {
            let value = &tag[k + 4..];
            if let Some(q) = value.chars().next().filter(|c| *c == '"' || *c == '\'') {
                out.push(value[1..].split(q).next().unwrap_or("").to_string());
            }
        }
    }
    // \includegraphics[options]{path}
    let mut rest = text;
    while let Some(i) = rest.find("\\includegraphics") {
        rest = &rest[i + "\\includegraphics".len()..];
        let mut r = rest.trim_start_matches('*').trim_start();
        if r.starts_with('[') {
            r = r.split_once(']').map_or("", |(_, after)| after).trim_start();
        }
        if let Some(inner) = r.strip_prefix('{') {
            out.push(inner.split('}').next().unwrap_or("").trim().to_string());
        }
    }
    out
}

/// `%20` and the like taken out of a path.
fn unescaped(path: &str) -> String {
    let b = path.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(hex) = path.get(i + 1..i + 3)
            && let Ok(v) = u8::from_str_radix(hex, 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| path.to_string())
}

/// What a document needs beside it, found in `folder`.
#[derive(Debug, Default, PartialEq, Eq)]
struct Shown {
    figures: Vec<PathBuf>,
    bibliographies: Vec<PathBuf>,
    /// what is named and cannot go along, one line each
    skipped: Vec<String>,
}

/// The file for `name` in `folder` (a LaTeX figure may leave its extension out), relative to `folder`; or why not.
fn locate(folder: &Path, name: &str, extensions: &[&str]) -> std::result::Result<PathBuf, String> {
    let name = unescaped(name);
    if name.is_empty() || name.starts_with('#') || name.contains("://") || name.starts_with("data:") || name.starts_with("mailto:") {
        return Err(String::new());
    }
    let rel = Path::new(&name);
    if rel.is_absolute() || rel.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_))) {
        return Err(format!("{name}: outside the document's folder"));
    }
    if folder.join(rel).is_file() {
        return Ok(rel.to_path_buf());
    }
    if rel.extension().is_none() {
        for ext in extensions {
            let with = PathBuf::from(format!("{name}.{ext}"));
            if folder.join(&with).is_file() {
                return Ok(with);
            }
        }
    }
    Err(format!("{name}: not found"))
}

fn shown(folder: &Path, text: &str) -> Shown {
    let mut out = Shown::default();
    let mut seen = BTreeSet::new();
    let mut note = |out: &mut Shown, line: String| {
        if !line.is_empty() && seen.insert(line.clone()) {
            out.skipped.push(line);
        }
    };
    for name in shown_paths(text) {
        match locate(folder, &name, &["pdf", "png", "jpg", "jpeg", "svg", "eps"]) {
            Ok(p) if !out.figures.contains(&p) => out.figures.push(p),
            Ok(_) => {}
            Err(line) => note(&mut out, line),
        }
    }
    for name in frontmatter::bibliography(text) {
        match locate(folder, &name, &["bib"]) {
            Ok(p) if !out.bibliographies.contains(&p) => out.bibliographies.push(p),
            Ok(_) => {}
            Err(line) => note(&mut out, line),
        }
    }
    out
}

// ---- send

/// What a package sent holds.
#[derive(Debug)]
pub struct Sent {
    pub manifest: Manifest,
    pub figures: usize,
    pub bibliographies: usize,
    /// the files the document names and that could not go along (missing, or outside its folder)
    pub skipped: Vec<String>,
}

/// The package of the document at `doc` with `text` (the text now: its base is written, so that the records of the
/// recipient anchor on exactly it), `round` its name and `me` who sends it, written to `to`.
pub fn send(doc: &Path, text: &str, me: &Person, round: &str, to: &Path) -> Result<Sent> {
    let name = doc.file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| plain_name(n)).ok_or_else(|| Error::Invalid(format!("{}: not a document", doc.display())))?;
    let folder = doc.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let base = store::write_base(doc, text)?;
    let manifest = Manifest { kind: PackageKind::Send, document: name.clone(), base, round: round.trim().to_string(), made: now(), from: me.clone() };

    let scratch = Scratch::new()?;
    let top = scratch.path().join(Path::new(&name).file_stem().map_or_else(|| "review".into(), |s| s.to_string_lossy().into_owned()));
    fs::create_dir_all(&top).map_err(io(&top))?;
    write_manifest(&top, &manifest)?;
    let docs = top.join("document");
    fs::create_dir_all(&docs).map_err(io(&docs))?;
    let target = docs.join(&name);
    fs::write(&target, text).map_err(io(&target))?;
    let found = shown(folder, text);
    for rel in found.figures.iter().chain(&found.bibliographies) {
        copy_file(&folder.join(rel), &docs.join(rel))?;
    }
    let review = store::review_dir(doc);
    if review.is_dir() {
        copy_dir(&review, &top.join("review"))?;
    }
    zip(&top, to)?;
    Ok(Sent { manifest, figures: found.figures.len(), bibliographies: found.bibliographies.len(), skipped: found.skipped })
}

// ---- open

/// A package unzipped for a while.
pub struct Opened {
    pub manifest: Manifest,
    /// kept for the folder to stay while this is
    #[allow(dead_code)]
    scratch: Scratch,
    /// the folder with the manifest in it
    top: PathBuf,
}

/// The package at `file` unzipped and its manifest read.
pub fn open(file: &Path) -> Result<Opened> {
    let scratch = Scratch::new()?;
    unzip(file, scratch.path())?;
    let top = [scratch.path().to_path_buf()]
        .into_iter()
        .chain(fs::read_dir(scratch.path()).map_err(io(scratch.path()))?.filter_map(|e| e.ok()).filter(|e| e.file_name() != "__MACOSX" && e.path().is_dir()).map(|e| e.path()))
        .find(|p| p.join("manifest.toml").is_file())
        .ok_or_else(|| Error::Format(file.to_path_buf(), "not a review file: no manifest".into()))?;
    let manifest = read_manifest(&top)?;
    // nothing but plain files and folders
    check_plain(&top)?;
    Ok(Opened { manifest, scratch, top })
}

fn check_plain(dir: &Path) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(io(dir))? {
        let entry = entry.map_err(io(dir))?;
        let kind = entry.file_type().map_err(io(dir))?;
        if kind.is_symlink() {
            return Err(Error::Invalid(format!("{}: a link in a review file", entry.path().display())));
        }
        if kind.is_dir() {
            check_plain(&entry.path())?;
        }
    }
    Ok(())
}

impl Opened {
    /// The folder with what the package holds.
    fn top(&self) -> &Path {
        &self.top
    }
}

/// Where a package of kind `send` unpacks under `root`: `<stem>-<hash8>`, the same for the same package.
fn home(root: &Path, m: &Manifest) -> PathBuf {
    let stem = Path::new(&m.document).file_stem().map_or_else(|| "review".into(), |s| s.to_string_lossy().into_owned());
    let id = blob_hash(&format!("{}\n{}\n{}", m.base, m.round, m.from.name));
    root.join(format!("{stem}-{}", &id[..8]))
}

/// The package (of kind `send`) unpacked into `root/<stem>-<hash8>/` (the document in `document/`, its review in
/// `document/.papers-write/review/<name>/` where the app looks for it) and the path of its document there. A package
/// unpacked before is not unpacked again: what was done in it stays.
pub fn unpack(opened: &Opened, root: &Path) -> Result<PathBuf> {
    let m = &opened.manifest;
    if m.kind != PackageKind::Send {
        return Err(Error::Invalid("this review is a review sent back, not a document to review".into()));
    }
    let dest = home(root, m);
    let document = dest.join("document").join(&m.document);
    if document.is_file() {
        return Ok(document);
    }
    if !opened.top().join("document").join(&m.document).is_file() {
        return Err(Error::Format(opened.top().to_path_buf(), format!("the document {} is not in the review file", m.document)));
    }
    fs::create_dir_all(root).map_err(io(root))?;
    let part = dest.with_extension("partial");
    let _ = fs::remove_dir_all(&part);
    copy_dir(opened.top(), &part)?;
    // the review where the app looks for the review of the document
    let sent = part.join("review");
    if sent.is_dir() {
        copy_dir(&sent, &store::review_dir(&part.join("document").join(&m.document)))?;
        fs::remove_dir_all(&sent).map_err(io(&sent))?;
    }
    let _ = fs::remove_dir_all(&dest);
    fs::rename(&part, &dest).map_err(io(&dest))?;
    Ok(document)
}

/// The folder an unpacked package is in, if `doc` is its document.
fn unpacked_home(doc: &Path) -> Option<PathBuf> {
    let docs = doc.parent()?;
    (docs.file_name()? == "document").then_some(())?;
    let home = docs.parent()?;
    home.join("manifest.toml").is_file().then(|| home.to_path_buf())
}

/// The manifest of the package `doc` was unpacked from (a document to review); none for any other file.
pub fn of_unpacked(doc: &Path) -> Option<Manifest> {
    let m = read_manifest(&unpacked_home(doc)?).ok()?;
    (m.kind == PackageKind::Send && doc.file_name()?.to_str()? == m.document).then_some(m)
}

// ---- send back

/// What a package sent back holds.
#[derive(Debug)]
pub struct Back {
    pub manifest: Manifest,
    pub suggestions: usize,
    pub comments: usize,
}

/// How many suggestions and comments of `records` stand: the ones not edited away and not withdrawn.
fn count(records: &[Record]) -> (usize, usize) {
    let gone: BTreeSet<&str> = records
        .iter()
        .filter_map(|r| r.supersedes.as_deref())
        .chain(records.iter().filter(|r| r.decision == Some(Decision::Withdrawn)).filter_map(|r| r.on.as_deref()))
        .collect();
    let of = |kind| records.iter().filter(|r| r.kind == kind && !gone.contains(r.id.as_str())).count();
    (of(Kind::Suggestion), of(Kind::Comment))
}

/// The package of kind `back` for the document `doc` (one unpacked from a package sent): `me`'s own person file and
/// the bases its records need, written to `to`.
pub fn send_back(doc: &Path, me: &Person, to: &Path) -> Result<Back> {
    let sent = of_unpacked(doc).ok_or_else(|| Error::Invalid("this document was not opened from a review file".into()))?;
    let slug = super::text::slug(&me.name);
    let file = store::person_path(doc, &slug);
    if !file.is_file() {
        return Err(Error::Invalid("Nothing to send back yet: no suggestion or comment of yours".into()));
    }
    let records = store::parse(&file)?.record;
    let manifest = Manifest { kind: PackageKind::Back, document: sent.document, base: sent.base, round: sent.round, made: now(), from: me.clone() };

    let scratch = Scratch::new()?;
    let top = scratch.path().join(&slug);
    let review = top.join("review");
    fs::create_dir_all(&review).map_err(io(&review))?;
    write_manifest(&top, &manifest)?;
    copy_file(&file, &review.join(format!("{slug}.toml")))?;
    let hashes: BTreeSet<&str> = records.iter().filter_map(|r| r.base.as_deref()).collect();
    for hash in hashes {
        let from = store::base_path(doc, hash);
        if from.is_file() {
            copy_file(&from, &review.join("bases").join(from.file_name().unwrap()))?;
        }
    }
    zip(&top, to)?;
    let (suggestions, comments) = count(&records);
    Ok(Back { manifest, suggestions, comments })
}

// ---- merge

/// Whether the text with `hash` is a version of the document at `doc`: its file now, a base of its review, or a
/// commit of its history (git).
pub fn holds(doc: &Path, hash: &str) -> bool {
    if store::base_path(doc, hash).is_file() || fs::read_to_string(doc).is_ok_and(|t| blob_hash(&t) == hash) {
        return true;
    }
    let (Some(folder), Some(name)) = (doc.parent().filter(|p| !p.as_os_str().is_empty()), doc.file_name()) else {
        return false;
    };
    // `:<mode> <mode> <old blob> <new blob> <status>\t<name>`, one line for each commit that touched the file
    let Ok(out) = Command::new("git").arg("-C").arg(folder).args(["log", "--format=", "--raw", "--no-abbrev", "--"]).arg(name).output() else {
        return false;
    };
    out.status.success() && String::from_utf8_lossy(&out.stdout).lines().any(|l| l.split_whitespace().take(4).any(|f| f == hash))
}

/// What a merge added.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Merged {
    pub suggestions: usize,
    pub comments: usize,
    /// replies and decisions
    pub others: usize,
    /// whose, each once
    pub people: Vec<String>,
    /// the bases copied
    pub bases: usize,
}

/// The review of a package sent back joined to the review of the document at `doc`: the bases that are missing
/// copied; a person's file copied when `doc` has none of theirs, else the records whose ids it lacks appended, in
/// order. Nothing already there is rewritten, and the same package merged again adds nothing.
pub fn merge(doc: &Path, opened: &Opened) -> Result<Merged> {
    if opened.manifest.kind != PackageKind::Back {
        return Err(Error::Invalid("this review file is a document to review, not a review sent back".into()));
    }
    let review = opened.top().join("review");
    let mut merged = Merged::default();
    let bases = review.join("bases");
    if bases.is_dir() {
        for entry in fs::read_dir(&bases).map_err(io(&bases))?.filter_map(|e| e.ok()) {
            let path = entry.path();
            let Some(hash) = path.file_stem().map(|s| s.to_string_lossy().into_owned()).filter(|h| h.len() == 40 && h.bytes().all(|b| b.is_ascii_hexdigit())) else {
                continue;
            };
            let to = store::base_path(doc, &hash);
            if !to.exists() && path.is_file() {
                copy_file(&path, &to)?;
                merged.bases += 1;
            }
        }
    }
    let mut files: Vec<PathBuf> = fs::read_dir(&review).map_err(io(&review))?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "toml")).collect();
    files.sort();
    let mut added: Vec<Record> = Vec::new();
    for path in files {
        let slug = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if slug.is_empty() || !slug.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            return Err(Error::Format(path, "not a name of a person's file".into()));
        }
        let content = store::parse(&path)?;
        let person = content.person.clone().unwrap_or_else(|| Person::new(slug.clone(), ""));
        let to = store::person_path(doc, &slug);
        let new: Vec<Record> = if to.is_file() {
            let have: BTreeSet<String> = store::parse(&to)?.record.into_iter().map(|r| r.id).collect();
            let new: Vec<Record> = content.record.into_iter().filter(|r| !have.contains(&r.id)).collect();
            for r in &new {
                store::append(&to, &person, r)?;
            }
            new
        } else {
            copy_file(&path, &to)?;
            content.record
        };
        if !new.is_empty() && !merged.people.contains(&person.name) {
            merged.people.push(person.name.clone());
        }
        added.extend(new);
    }
    (merged.suggestions, merged.comments) = count(&added);
    merged.others = added.iter().filter(|r| matches!(r.kind, Kind::Reply | Kind::Decision)).count();
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::ops::NewRecord;
    use crate::review::testutil::doc;
    use crate::review::{Review, State, Status, text::utf16_of};

    const TEXT: &str = "---\ntitle: Retreat\nbibliography: refs.bib\n---\n\nThe grounding line retreats rapidly when the bed slopes inland [@smith2020].\n\n![The loop](figures/loop.png){#fig:x}\n\n\\includegraphics[width=5cm]{figures/b}\n\nWe show this in Fig. 3. The ice shelf thins.\n";

    fn range(text: &str, needle: &str) -> (usize, usize) {
        let b = text.find(needle).unwrap();
        (utf16_of(text, b), utf16_of(text, b + needle.len()))
    }

    fn alex() -> Person {
        Person::new("Alex Robinson", "alex@example.org")
    }

    fn jane() -> Person {
        Person::new("Jane Doe", "jane@example.org")
    }

    /// An author's folder with the document, what it shows, and one comment of the author's in its review.
    fn author() -> PathBuf {
        let d = doc("paper.md");
        let dir = d.parent().unwrap();
        fs::write(&d, TEXT).unwrap();
        fs::create_dir_all(dir.join("figures")).unwrap();
        fs::write(dir.join("figures/loop.png"), b"png").unwrap();
        fs::write(dir.join("figures/b.pdf"), b"pdf").unwrap();
        fs::write(dir.join("figures/unused.png"), b"x").unwrap();
        fs::write(dir.join("refs.bib"), "@article{smith2020, title={T}}\n").unwrap();
        let (f, t) = range(TEXT, "Fig. 3");
        Review::add(&d, &alex(), TEXT, NewRecord::comment(f, t, "Check the number")).unwrap();
        d
    }

    fn names(dir: &Path) -> Vec<String> {
        fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
            for e in fs::read_dir(dir).unwrap().filter_map(|e| e.ok()) {
                let p = e.path();
                if p.is_dir() {
                    walk(base, &p, out);
                } else {
                    out.push(p.strip_prefix(base).unwrap().to_string_lossy().into_owned());
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    #[test]
    fn what_a_document_shows() {
        let dir = author().parent().unwrap().to_path_buf();
        let text = format!("{TEXT}\n![Remote](https://example.org/x.png) ![up](../x.png) ![gone](missing.png) ![spaced](<figures/loop.png> \"title\") <img src=\"figures/loop.png\"> ![%](figures/loop%2Epng)\n");
        let s = shown(&dir, &text);
        assert_eq!(s.figures, [PathBuf::from("figures/loop.png"), PathBuf::from("figures/b.pdf")]);
        assert_eq!(s.bibliographies, [PathBuf::from("refs.bib")]);
        assert_eq!(s.skipped, ["../x.png: outside the document's folder", "missing.png: not found"]);
    }

    #[test]
    fn the_package_holds_the_document_what_it_shows_and_the_review() {
        let d = author();
        let to = d.parent().unwrap().join("paper — Draft.review");
        let sent = send(&d, TEXT, &alex(), " Draft of 5 Nov 2026 ", &to).unwrap();
        assert_eq!((sent.figures, sent.bibliographies), (2, 1));
        assert_eq!(sent.manifest.base, blob_hash(TEXT));
        assert_eq!(sent.manifest.round, "Draft of 5 Nov 2026");
        let opened = open(&to).unwrap();
        assert_eq!(opened.manifest, sent.manifest);
        assert_eq!(opened.manifest.kind, PackageKind::Send);
        let files = names(opened.top());
        let base = format!("review/bases/{}.md", blob_hash(TEXT));
        assert_eq!(files, ["document/figures/b.pdf", "document/figures/loop.png", "document/paper.md", "document/refs.bib", "manifest.toml", "review/alex-robinson.toml", &base]);
        assert_eq!(fs::read_to_string(opened.top().join("document/paper.md")).unwrap(), TEXT);
        // the manifest as a person reads it
        let m = fs::read_to_string(opened.top().join("manifest.toml")).unwrap();
        assert!(m.contains("kind = \"send\"") && m.contains("document = \"paper.md\"") && m.contains("name = \"Alex Robinson\""), "{m}");
        // sending again over the same file replaces it
        send(&d, TEXT, &alex(), "Draft of 5 Nov 2026", &to).unwrap();
        assert_eq!(open(&to).unwrap().manifest.round, "Draft of 5 Nov 2026");
    }

    #[test]
    fn not_a_review_file() {
        let dir = author().parent().unwrap().to_path_buf();
        let junk = dir.join("junk.review");
        fs::write(&junk, "hello").unwrap();
        assert!(open(&junk).is_err());
        assert!(open(&dir.join("nothing.review")).is_err());
        assert_eq!(of_unpacked(&dir.join("paper.md")), None);
    }

    #[test]
    fn a_round_trip() {
        // the author sends
        let d = author();
        let dir = d.parent().unwrap().to_path_buf();
        let out = dir.join("paper — Draft.review");
        send(&d, TEXT, &alex(), "Draft", &out).unwrap();

        // the recipient unpacks it
        let apps = doc("data").parent().unwrap().to_path_buf();
        let opened = open(&out).unwrap();
        let mine = unpack(&opened, &apps.join("reviews")).unwrap();
        assert!(mine.starts_with(apps.join("reviews")));
        assert_eq!(fs::read_to_string(&mine).unwrap(), TEXT);
        assert!(mine.parent().unwrap().join("figures/loop.png").is_file());
        assert!(mine.parent().unwrap().join("refs.bib").is_file());
        assert_eq!(of_unpacked(&mine).unwrap().from, alex());
        // the author's comment is in the review there
        assert_eq!(Review::load(&mine).unwrap().items(TEXT).len(), 1);
        // opened again: the same folder, what was done there kept
        let (f, t) = range(TEXT, "rapidly");
        Review::add(&mine, &jane(), TEXT, NewRecord::suggestion(f, t, "slowly")).unwrap();
        let (f, t) = range(TEXT, "thins");
        Review::add(&mine, &jane(), TEXT, NewRecord::suggestion(f, t, "thickens").note("data?")).unwrap();
        let (f, t) = range(TEXT, "Fig. 3");
        let c = Review::add(&mine, &jane(), TEXT, NewRecord::comment(f, t, "Which panel?")).unwrap();
        // an edit of the text made here is allowed, and a comment on it goes with its base
        let edited = TEXT.replace("The ice shelf", "The big ice shelf");
        let (f, t) = range(&edited, "big");
        let gone = Review::add(&mine, &jane(), &edited, NewRecord::comment(f, t, "why big")).unwrap();
        Review::decide(&mine, &jane(), &gone.id, Decision::Withdrawn).unwrap();
        Review::reply(&mine, &jane(), "alex-robinson-1", "Agreed").unwrap();
        let again = unpack(&open(&out).unwrap(), &apps.join("reviews")).unwrap();
        assert_eq!(again, mine);
        assert_eq!(Review::load(&mine).unwrap().records().filter(|r| r.id.starts_with("jane-doe")).count(), 6);

        // and sends back: their own file, the bases it needs
        let back = apps.join("paper — Jane Doe.review");
        let b = send_back(&mine, &jane(), &back).unwrap();
        assert_eq!((b.suggestions, b.comments), (2, 1), "the withdrawn comment is not counted");
        assert_eq!(b.manifest.kind, PackageKind::Back);
        assert_eq!(b.manifest.base, blob_hash(TEXT));
        assert_eq!(b.manifest.from, jane());
        let theirs = open(&back).unwrap();
        let files = names(theirs.top());
        let (base_a, base_b) = (format!("review/bases/{}.md", blob_hash(TEXT)), format!("review/bases/{}.md", blob_hash(&edited)));
        let mut want = vec!["manifest.toml".to_string(), "review/jane-doe.toml".to_string(), base_a, base_b];
        want.sort();
        assert_eq!(files, want, "no document, nothing of the author's");
        assert!(unpack(&theirs, &apps.join("reviews")).is_err(), "a review sent back is not a document");

        // the author finds the document by the base and merges
        assert!(holds(&d, &theirs.manifest.base));
        assert!(!holds(&d, &blob_hash("another text")));
        let m = merge(&d, &theirs).unwrap();
        assert_eq!((m.suggestions, m.comments, m.others), (2, 1, 2), "{m:?}");
        assert_eq!(m.people, ["Jane Doe"]);
        let items = Review::load(&d).unwrap().items(TEXT);
        let by_jane: Vec<_> = items.iter().filter(|i| i.author == "Jane Doe" && i.state == State::Open).collect();
        assert_eq!(by_jane.len(), 3);
        assert_eq!(by_jane[0].record.exact.as_deref(), Some("rapidly"));
        assert_eq!(by_jane[0].record.insert.as_deref(), Some("slowly"));
        assert_eq!(by_jane[1].id, c.id);
        assert_eq!(by_jane[2].record.exact.as_deref(), Some("thins"));
        assert_eq!(by_jane[2].record.text.as_deref(), Some("data?"));
        assert!(items.iter().any(|i| i.author == "Alex Robinson" && i.thread.len() == 1), "the reply is in the author's thread");
        assert!(by_jane.iter().all(|i| i.status == Status::Exact));
        // the author's own file is untouched
        assert_eq!(Review::load(&d).unwrap().files.iter().find(|f| f.slug == "alex-robinson").unwrap().records.len(), 1);

        // the same package again adds nothing
        let before = fs::read(store::person_path(&d, "jane-doe")).unwrap();
        let m2 = merge(&d, &theirs).unwrap();
        assert_eq!(m2, Merged::default());
        assert_eq!(fs::read(store::person_path(&d, "jane-doe")).unwrap(), before);
    }

    #[test]
    fn merging_appends_to_a_file_that_is_there_and_carries_over_an_edit() {
        let d = author();
        let out = d.parent().unwrap().join("p.review");
        send(&d, TEXT, &alex(), "Draft", &out).unwrap();
        let apps = doc("data").parent().unwrap().to_path_buf();
        let mine = unpack(&open(&out).unwrap(), &apps.join("reviews")).unwrap();
        let (f, t) = range(TEXT, "rapidly");
        Review::add(&mine, &jane(), TEXT, NewRecord::suggestion(f, t, "slowly")).unwrap();
        let back1 = apps.join("one.review");
        send_back(&mine, &jane(), &back1).unwrap();
        let (f, t) = range(TEXT, "thins");
        Review::add(&mine, &jane(), TEXT, NewRecord::comment(f, t, "How much?")).unwrap();
        let back2 = apps.join("two.review");
        send_back(&mine, &jane(), &back2).unwrap();

        // the author edits meanwhile: a paragraph in front, and a word of the sentence
        let now_text = TEXT.replace("The grounding line", "A new paragraph, long enough to move things.\n\nThe grounding line").replace("retreats rapidly", "retreats quite rapidly");
        fs::write(&d, &now_text).unwrap();
        let m = merge(&d, &open(&back1).unwrap()).unwrap();
        assert_eq!((m.suggestions, m.comments), (1, 0));
        // the second package holds both of Jane's records: only the one the file lacks is added
        let m = merge(&d, &open(&back2).unwrap()).unwrap();
        assert_eq!((m.suggestions, m.comments), (0, 1));
        let jane_file = &Review::load(&d).unwrap().files.iter().find(|f| f.slug == "jane-doe").unwrap().records.len();
        assert_eq!(*jane_file, 2);
        let items = Review::load(&d).unwrap().items(&now_text);
        let sug = items.iter().find(|i| i.author == "Jane Doe" && i.kind == Kind::Suggestion).unwrap();
        assert_ne!(sug.status, Status::Exact, "the text moved under it");
        assert_eq!(&now_text[sug.range.clone()], "rapidly");
        assert_eq!(sug.replacement.as_deref(), Some("slowly"));
        let com = items.iter().find(|i| i.author == "Jane Doe" && i.kind == Kind::Comment).unwrap();
        assert_eq!(&now_text[com.range.clone()], "thins");
        // merging a document to review is refused
        assert!(merge(&d, &open(&out).unwrap()).is_err());
    }

    #[test]
    fn the_base_is_found_in_git_history() {
        let d = author();
        let dir = d.parent().unwrap();
        let git = |args: &[&str]| {
            let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
            assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        git(&["init", "-q"]);
        git(&["config", "user.name", "Bob"]);
        git(&["config", "user.email", "bob@example.org"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["add", "paper.md"]);
        git(&["commit", "-q", "-m", "first"]);
        fs::write(&d, "something else\n").unwrap();
        git(&["commit", "-q", "-am", "second"]);
        assert!(holds(&d, &blob_hash(TEXT)));
        assert!(holds(&d, &blob_hash("something else\n")));
        assert!(!holds(&d, &blob_hash("never")));
    }
}
