//! The bibliography that grows: a key cited in the text is copied from the papers library into the document's
//! own `.bib` (the first file its front matter's `bibliography` names, else `references.bib` beside it), and the
//! keys cited are compared with those the `.bib` has.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use papers_core::Library;
use serde::Serialize;

use crate::frontmatter;
use crate::refs::{References, folder_of};

/// The `.bib` file a document gets when its front matter names none.
pub const DEFAULT_BIB: &str = "references.bib";

/// What citing a key did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Cited {
    /// the `.bib` file of the document the key is in now
    pub bib: PathBuf,
    /// whether the entry was added to it (false: it was there)
    pub added: bool,
    /// the Markdown text with the `bibliography` line added to its front matter, when the document named no
    /// `.bib` (the editor's text is to be replaced with it)
    pub markdown: Option<String>,
}

/// Why a key could not be cited.
#[derive(Debug)]
pub enum Error {
    /// the key is in neither the document's `.bib` nor the papers library
    NotFound { key: String },
    /// the `.bib` file of the document does not parse (it is left as it is)
    Bib { path: PathBuf, message: String },
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound { key } => write!(f, "{key}: in neither the bibliography nor the papers library"),
            Error::Bib { message, .. } => f.write_str(message),
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for Error {}

/// The key `key` cited in the document `markdown_path` (its text `markdown_text`): in its `.bib`, copied there
/// from the references if it is not. The `.bib` is the first its front matter names, else `references.bib` in
/// the document's folder, and then [`Cited::markdown`] has the line that names it. A key already in a `.bib` the
/// front matter names is left where it is. The file is written whole, by a rename, and its text is kept.
pub fn cite(markdown_path: &Path, markdown_text: &str, key: &str, references: &References) -> Result<Cited, Error> {
    let folder = folder_of(markdown_path);
    let named: Vec<PathBuf> = frontmatter::bibliography(markdown_text).into_iter().map(|n| folder.join(n)).collect();
    let (target, markdown) = match named.first() {
        Some(first) => (first.clone(), None),
        None => (folder.join(DEFAULT_BIB), Some(frontmatter::with_bibliography(markdown_text, DEFAULT_BIB))),
    };
    // the target is read for its keys, and it must parse; the others are only looked in
    let in_target = target.is_file() && keys_of(&target)?.iter().any(|k| k == key);
    let in_others = named.iter().skip(1).any(|bib| keys_of(bib).is_ok_and(|keys| keys.iter().any(|k| k == key)));
    if in_target || in_others {
        return Ok(Cited { bib: target, added: false, markdown });
    }
    let entry = references.entry(key).ok_or_else(|| Error::NotFound { key: key.to_string() })?;
    append(&target, &without(&entry.bibtex, PERSONAL))?;
    Ok(Cited { bib: target, added: true, markdown })
}

/// The fields of a library's entry that are its owner's, not the paper's: dropped as the entry is copied into the
/// document's bibliography (a `.bib` that goes to co-authors and journals).
const PERSONAL: &[&str] = &["file"];

/// A BibTeX entry without the fields `names` (case ignored): each field from its name to the end of its value,
/// braced, quoted or bare, and the comma after it. The rest of the text is kept as it is.
fn without(entry: &str, names: &[&str]) -> String {
    let bytes = entry.as_bytes();
    // the fields are at depth 1, inside the entry's braces
    let Some(open) = entry.find(['{', '(']) else {
        return entry.to_string();
    };
    let mut out = String::with_capacity(entry.len());
    out.push_str(&entry[..=open]);
    let mut i = open + 1;
    let mut kept = open + 1;
    while i < bytes.len() {
        // the start of a field: after `,` or the opening, spaces, a name, spaces, `=`
        let line_start = i;
        let mut j = i;
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        let name_start = j;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || b"_-:.".contains(&bytes[j])) {
            j += 1;
        }
        let name = &entry[name_start..j];
        let mut k = j;
        while k < bytes.len() && (bytes[k] as char).is_whitespace() {
            k += 1;
        }
        let is_field = !name.is_empty() && k < bytes.len() && bytes[k] == b'=';
        // the end of this field's value: the `,` or closing brace at depth 0 of the value
        let mut end = if is_field { k + 1 } else { line_start };
        let mut depth = 0i32;
        let mut quoted = false;
        while end < bytes.len() {
            match bytes[end] {
                b'{' => depth += 1,
                b'}' if depth > 0 => depth -= 1,
                b'"' if depth == 0 => quoted = !quoted,
                b',' if depth == 0 && !quoted => break,
                b'}' | b')' if depth == 0 && !quoted => break,
                _ => {}
            }
            end += 1;
        }
        if is_field && names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            out.push_str(&entry[kept..line_start]);
            if end < bytes.len() && bytes[end] == b',' {
                // the comma after it goes too
                kept = end + 1;
            } else {
                // the last field: the comma before it goes instead
                if out.ends_with(',') {
                    out.pop();
                }
                kept = end;
            }
        }
        if end >= bytes.len() || bytes[end] != b',' {
            break;
        }
        i = end + 1;
    }
    out.push_str(&entry[kept..]);
    out
}

/// The keys of the entries of a `.bib` file.
fn keys_of(bib: &Path) -> Result<Vec<String>, Error> {
    if !bib.is_file() {
        return Ok(Vec::new());
    }
    let library = Library::load(&[bib.to_path_buf()]).map_err(|e| Error::Bib { path: bib.to_path_buf(), message: e.to_string() })?;
    Ok(library.items().iter().map(|i| i.key.clone()).collect())
}

/// `entry` added at the end of `bib` after one blank line, the text of the file as it was; the file is made if
/// it is not there. Written to a file beside it that then takes its place.
fn append(bib: &Path, entry: &str) -> Result<(), Error> {
    let io = |source| Error::Io { path: bib.to_path_buf(), source };
    let old = match fs::read_to_string(bib) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(io(e)),
    };
    let eol = if old.contains("\r\n") { "\r\n" } else { "\n" };
    let mut text = old;
    if !text.trim().is_empty() {
        let newlines = text.chars().rev().take_while(|&c| c == '\n' || c == '\r').filter(|&c| c == '\n').count();
        for _ in newlines..2 {
            text.push_str(eol);
        }
    }
    text.push_str(entry.trim());
    text.push_str(eol);
    let name = bib.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = bib.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    fs::write(&temp, text).map_err(io)?;
    if let Err(e) = fs::rename(&temp, bib) {
        let _ = fs::remove_file(&temp);
        return Err(io(e));
    }
    Ok(())
}

/// The keys cited in a Markdown text, each once, in order: `[@key]` and `@key` (pandoc; not the cross-references
/// `@fig:`, `@eq:`, `@tbl:`, `@sec:`, `@lst:`, not an e-mail address), `\cite{a,b}` and its kin (`\citep`,
/// `\citet`, `\parencite`, `\nocite`, …). Neither the front matter, nor code (fenced or inline) counts.
pub fn cited_keys(markdown_text: &str) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let mut add = |key: &str| {
        let key = key.trim();
        if !key.is_empty() && !keys.iter().any(|k| k == key) {
            keys.push(key.to_string());
        }
    };
    let mut fence: Option<(char, usize)> = None;
    for line in markdown_text[frontmatter::body_start(markdown_text)..].lines() {
        let t = line.trim_start();
        let run = |c: char| t.chars().take_while(|&x| x == c).count();
        if line.len() - t.len() < 4 {
            let (c, n) = if t.starts_with('`') { ('`', run('`')) } else if t.starts_with('~') { ('~', run('~')) } else { (' ', 0) };
            if n >= 3 {
                match fence {
                    None => {
                        fence = Some((c, n));
                        continue;
                    }
                    Some((fc, fnum)) if fc == c && n >= fnum && t[n..].trim().is_empty() => {
                        fence = None;
                        continue;
                    }
                    _ => {}
                }
            }
        }
        if fence.is_none() {
            scan(&without_code(line), &mut add);
        }
    }
    keys
}

/// `line` with its inline code spans blanked.
fn without_code(line: &str) -> String {
    if !line.contains('`') {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '`' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let n = chars[i..].iter().take_while(|&&c| c == '`').count();
        // the closing run of the same length
        let mut j = i + n;
        let mut close = None;
        while j < chars.len() {
            if chars[j] == '`' {
                let m = chars[j..].iter().take_while(|&&c| c == '`').count();
                if m == n {
                    close = Some(j + m);
                    break;
                }
                j += m;
            } else {
                j += 1;
            }
        }
        match close {
            Some(end) => {
                out.extend(std::iter::repeat_n(' ', end - i));
                i = end;
            }
            None => {
                out.extend(std::iter::repeat_n('`', n));
                i += n;
            }
        }
    }
    out
}

/// The keys of a line, given to `add`.
fn scan(line: &str, add: &mut impl FnMut(&str)) {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let at = |i: usize| chars.get(i).map(|&(_, c)| c);
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut i = 0;
    while i < chars.len() {
        match chars[i].1 {
            '\\' => {
                let mut j = i + 1;
                while at(j).is_some_and(|c| c.is_ascii_alphabetic()) {
                    j += 1;
                }
                let name = line[chars.get(i + 1).map_or(line.len(), |c| c.0)..chars.get(j).map_or(line.len(), |c| c.0)].to_ascii_lowercase();
                if !(name.starts_with("cite") || name.ends_with("cite")) || name.is_empty() {
                    // `\@` and the like: the next character is not a start
                    i = j.max(i + 2);
                    continue;
                }
                if at(j) == Some('*') {
                    j += 1;
                }
                // up to two optional arguments, then the keys
                for _ in 0..2 {
                    let mut k = j;
                    while at(k) == Some(' ') {
                        k += 1;
                    }
                    if at(k) != Some('[') {
                        break;
                    }
                    match (k..chars.len()).find(|&m| chars[m].1 == ']') {
                        Some(m) => j = m + 1,
                        None => break,
                    }
                }
                while at(j) == Some(' ') {
                    j += 1;
                }
                if at(j) == Some('{')
                    && let Some(m) = (j..chars.len()).find(|&m| chars[m].1 == '}')
                {
                    line[chars[j + 1].0..chars[m].0].split(',').for_each(&mut *add);
                    j = m + 1;
                }
                i = j.max(i + 1);
            }
            '@' if i == 0 || !(word(chars[i - 1].1) || chars[i - 1].1 == '\\') => {
                let start = i + 1;
                let braced = at(start) == Some('{');
                let (key, end) = if braced {
                    match (start..chars.len()).find(|&m| chars[m].1 == '}') {
                        Some(m) => (&line[chars[start + 1].0..chars[m].0], m + 1),
                        None => ("", start),
                    }
                } else {
                    let mut m = start;
                    if at(m).is_some_and(word) {
                        m += 1;
                        // inside a key: punctuation only when a word character follows it
                        while let Some(c) = at(m) {
                            let inner = ":.#$%&-+?<>~/".contains(c) && at(m + 1).is_some_and(word);
                            if word(c) || inner {
                                m += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    let from = chars.get(start).map_or(line.len(), |c| c.0);
                    let to = chars.get(m).map_or(line.len(), |c| c.0);
                    (&line[from..to], m)
                };
                let cross = ["fig:", "eq:", "tbl:", "sec:", "lst:"].iter().any(|p| key.starts_with(p));
                if !key.is_empty() && !cross {
                    add(key);
                }
                i = end.max(i + 1);
            }
            _ => i += 1,
        }
    }
}

/// The keys cited in the text that `bib_keys` do not have, in order.
pub fn missing<S: AsRef<str>>(markdown_text: &str, bib_keys: &[S]) -> Vec<String> {
    let have: Vec<&str> = bib_keys.iter().map(AsRef::as_ref).collect();
    cited_keys(markdown_text).into_iter().filter(|k| !have.contains(&k.as_str())).collect()
}

/// The keys of `bib_keys` that the text does not cite, in the order given.
pub fn unused<S: AsRef<str>>(markdown_text: &str, bib_keys: &[S]) -> Vec<String> {
    let cited = cited_keys(markdown_text);
    bib_keys.iter().map(|k| k.as_ref()).filter(|k| !cited.iter().any(|c| c == k)).map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn personal_fields_dropped() {
        let entry = "@article{joughin2019,\n author = {Joughin, Ian},\n file = {:/Users/me/lit/a.pdf:PDF},\n title = {Regularized {Coulomb} friction, laws},\n year = {2019}\n}";
        let kept = without(entry, PERSONAL);
        assert_eq!(kept, "@article{joughin2019,\n author = {Joughin, Ian},\n title = {Regularized {Coulomb} friction, laws},\n year = {2019}\n}");
        // the last field, with no comma after it, and quoted values
        let last = "@misc{a, title = \"x, y\", File = \"/p.pdf\"}";
        assert_eq!(without(last, PERSONAL), "@misc{a, title = \"x, y\"}");
        // nothing to drop
        let none = "@book{b,\n title = {Fields of file = {x}}\n}";
        assert_eq!(without(none, PERSONAL), none);
    }

    use super::*;

    /// An empty folder of its own for a test.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("write-core-bib-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const LIBRARY: &str = "@article{robinson2012,\n  author = {Robinson, Alexander},\n  title = {Multistability},\n  year = {2012},\n}\n\n@article{feldmann2017,\n  author = {Feldmann, Johannes},\n  title = {Cyclic ice streaming},\n  year = {2017},\n}\n";

    /// A library of the two entries in its own file, and its references.
    fn library(dir: &Path) -> References {
        let path = dir.join("library").join("papers.bib");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, LIBRARY).unwrap();
        References::load(vec![path])
    }

    #[test]
    fn a_key_already_in_the_bib_is_left() {
        let dir = folder("present");
        let refs = library(&dir);
        let own = "@book{own,\n  title = {Own},\n}\n";
        fs::write(dir.join("my.bib"), own).unwrap();
        let text = "---\nbibliography: my.bib\n---\nSee [@own].";
        let cited = cite(&dir.join("paper.md"), text, "own", &refs).unwrap();
        assert_eq!(cited, Cited { bib: dir.join("my.bib"), added: false, markdown: None });
        assert_eq!(fs::read_to_string(dir.join("my.bib")).unwrap(), own);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_key_of_the_library_is_added_and_the_old_text_kept() {
        let dir = folder("added");
        let refs = library(&dir);
        let own = "% mine\n@book{own,\n title = {Own}  ,\n}\n";
        fs::write(dir.join("my.bib"), own).unwrap();
        let text = "---\nbibliography: [my.bib, other.bib]\n---\n";
        let cited = cite(&dir.join("paper.md"), text, "robinson2012", &refs).unwrap();
        assert_eq!(cited, Cited { bib: dir.join("my.bib"), added: true, markdown: None });
        let now = fs::read_to_string(dir.join("my.bib")).unwrap();
        assert!(now.starts_with(own), "{now}");
        // one blank line, then the entry
        assert!(now[own.len()..].starts_with("\n@article{robinson2012,"), "{now}");
        assert!(now.ends_with("}\n"));
        // and now it is there
        let again = cite(&dir.join("paper.md"), text, "robinson2012", &refs).unwrap();
        assert!(!again.added);
        assert_eq!(fs::read_to_string(dir.join("my.bib")).unwrap(), now);
        // another one: still one blank line between
        cite(&dir.join("paper.md"), text, "feldmann2017", &refs).unwrap();
        let keys = keys_of(&dir.join("my.bib")).unwrap();
        assert_eq!(keys, ["own", "robinson2012", "feldmann2017"]);
        assert!(fs::read_to_string(dir.join("my.bib")).unwrap().starts_with(&now));
        // no temporary file is left
        let left: Vec<_> = fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(left.iter().all(|n| !n.ends_with(".tmp")), "{left:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_old_text_without_a_final_newline_is_separated_by_one_blank_line() {
        let dir = folder("newline");
        let refs = library(&dir);
        fs::write(dir.join("my.bib"), "@book{own, title = {Own}}").unwrap();
        cite(&dir.join("p.md"), "---\nbibliography: my.bib\n---\n", "feldmann2017", &refs).unwrap();
        let now = fs::read_to_string(dir.join("my.bib")).unwrap();
        assert!(now.starts_with("@book{own, title = {Own}}\n\n@article{feldmann2017,"), "{now}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_bib_is_made() {
        let dir = folder("made");
        let refs = library(&dir);
        let text = "---\nbibliography: new.bib\n---\n";
        let cited = cite(&dir.join("p.md"), text, "feldmann2017", &refs).unwrap();
        assert!(cited.added);
        let now = fs::read_to_string(dir.join("new.bib")).unwrap();
        assert!(now.starts_with("@article{feldmann2017,") && now.ends_with("}\n"), "{now}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_key_in_neither_is_an_error_and_nothing_is_written() {
        let dir = folder("neither");
        let refs = library(&dir);
        let text = "---\nbibliography: my.bib\n---\n";
        let error = cite(&dir.join("p.md"), text, "nobody1999", &refs).unwrap_err();
        assert!(matches!(error, Error::NotFound { .. }));
        assert!(error.to_string().contains("nobody1999"));
        assert!(!dir.join("my.bib").exists());
        let error = cite(&dir.join("p.md"), "text", "nobody1999", &refs).unwrap_err();
        assert!(matches!(error, Error::NotFound { .. }));
        assert!(!dir.join("references.bib").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bib_that_does_not_parse_is_not_written_to() {
        let dir = folder("broken");
        let refs = library(&dir);
        let broken = "@article{x, title = {open";
        fs::write(dir.join("my.bib"), broken).unwrap();
        let text = "---\nbibliography: my.bib\n---\n";
        assert!(matches!(cite(&dir.join("p.md"), text, "feldmann2017", &refs), Err(Error::Bib { .. })));
        assert_eq!(fs::read_to_string(dir.join("my.bib")).unwrap(), broken);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn with_no_bibliography_field_references_bib_is_made_and_the_markdown_changes() {
        let dir = folder("default");
        let refs = library(&dir);
        let text = "---\ntitle: T\n---\n\nSee [@robinson2012].\n";
        let cited = cite(&dir.join("p.md"), text, "robinson2012", &refs).unwrap();
        assert_eq!(cited.bib, dir.join("references.bib"));
        assert!(cited.added);
        assert_eq!(cited.markdown.as_deref(), Some("---\ntitle: T\nbibliography: references.bib\n---\n\nSee [@robinson2012].\n"));
        assert!(fs::read_to_string(dir.join("references.bib")).unwrap().starts_with("@article{robinson2012,"));
        // no front matter at all
        let cited = cite(&dir.join("q.md"), "# Intro\n", "feldmann2017", &refs).unwrap();
        assert_eq!(cited.markdown.as_deref(), Some("---\nbibliography: references.bib\n---\n\n# Intro\n"));
        // an existing references.bib with the key: nothing added, but the document gets linked
        let cited = cite(&dir.join("q.md"), "# Intro\n", "feldmann2017", &refs).unwrap();
        assert!(!cited.added && cited.markdown.is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    fn keys_in(text: &str) -> Vec<String> {
        cited_keys(text)
    }

    #[test]
    fn citations_in_pandoc_and_latex() {
        assert_eq!(keys_in("A [@a2000] and [@b2001; @c2002, p. 3] and @d2003 said; [-@e2004]."), ["a2000", "b2001", "c2002", "d2003", "e2004"]);
        assert_eq!(keys_in("\\cite{a, b} \\citep[see][p.~2]{c} \\citet*{a} \\parencite{d}"), ["a", "b", "c", "d"]);
        assert_eq!(keys_in("[@{odd key}] and @smith:2010-x, then @a_b."), ["odd key", "smith:2010-x", "a_b"]);
    }

    #[test]
    fn cross_references_emails_code_and_front_matter_are_not_citations() {
        let text = "---\ntitle: mail me@here.org @x\n---\n\nSee @fig:one, @eq:two, @tbl:three and @sec:four.\nWrite to alex@awi.de or @home.\n\
                    `@code` and ``a ` @b``\n\n```\n[@fenced]\n\\cite{fenced}\n```\n\nAfter [@real].\n";
        assert_eq!(keys_in(text), ["home", "real"]);
        assert_eq!(keys_in("\\ref{x} \\@ \\section{y}"), Vec::<String>::new());
    }

    #[test]
    fn missing_and_unused_keys() {
        let text = "[@a; @b] \\cite{c} @a @fig:x";
        let bib = vec!["b".to_string(), "z".to_string()];
        assert_eq!(missing(text, &bib), ["a", "c"]);
        assert_eq!(unused(text, &bib), ["z"]);
        assert_eq!(missing(text, &["a", "b", "c"]), Vec::<String>::new());
        assert_eq!(unused("nothing", &["a", "b"]), ["a", "b"]);
    }
}
