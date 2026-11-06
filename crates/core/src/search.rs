//! A project searched, and text replaced in it: a word, a phrase or a regular expression, in the text files of a
//! folder and the folders in it. Positions are UTF-16 code units, as CodeMirror counts.
//!
//! The files searched are the text a paper is made of (`searched`); hidden files and folders and those of builds and
//! tools (`build`, `_build`, `node_modules`, `target`) are not, nor files over 2 MB or not UTF-8. A match spans lines
//! only when the expression asks for it (`\n`); `^` and `$` are the ends of a line.

use std::fs;
use std::path::{Path, PathBuf};

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::files::natural;

/// What to find: the text, and how.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub text: String,
    /// case matters
    pub case: bool,
    /// whole words only
    pub word: bool,
    /// the text is a regular expression
    pub regex: bool,
}

/// A match in a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hit {
    /// the line it begins on, from 1
    pub line: usize,
    /// the match in `text`, in UTF-16 code units
    pub from: usize,
    pub to: usize,
    /// the line, cut to some 160 characters around the match, with "…" where it was cut
    pub text: String,
    /// the match in the whole file, in UTF-16 code units: where the editor selects it
    pub at: usize,
    pub end: usize,
}

/// The matches in a file.
#[derive(Debug, Clone, Serialize)]
pub struct FileHits {
    pub path: PathBuf,
    pub hits: Vec<Hit>,
}

/// The extensions of the files searched.
const EXTENSIONS: &[&str] = &[
    "md", "markdown", "tex", "bib", "txt", "toml", "yaml", "yml", "sty", "cls", "csv", "lua",
];

/// The folders not searched, besides the hidden ones: what builds and tools make.
const SKIPPED: &[&str] = &["build", "_build", "node_modules", "target"];

/// The largest file searched.
const LARGEST: u64 = 2 * 1024 * 1024;

/// The characters of a line shown around a match.
const WIDTH: usize = 160;
/// Of these, the characters before the match (more when the line ends soon after it).
const BEFORE: usize = 40;

/// The expression of a query: the text as it is unless it is a regular expression, case aside unless it matters,
/// within word boundaries for whole words.
pub fn matcher(q: &Query) -> Result<Regex, String> {
    let pattern = if q.regex { q.text.clone() } else { regex::escape(&q.text) };
    let pattern = if q.word { format!(r"\b(?:{pattern})\b") } else { pattern };
    RegexBuilder::new(&pattern)
        .case_insensitive(!q.case)
        .multi_line(true)
        .crlf(true)
        .build()
        .map_err(|e| e.to_string())
}

/// Whether a file is among those searched, by its name: not hidden, and an extension of text.
pub fn searched(path: &Path) -> bool {
    let hidden = path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
    let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    !hidden && EXTENSIONS.contains(&extension.as_str())
}

/// The matches of a query in the files under `root`, file by file in natural order of their paths; `most` in all at
/// most (the last file's may be cut short). An empty query finds nothing.
pub fn search(root: &Path, q: &Query, most: usize) -> Result<Vec<FileHits>, String> {
    if q.text.is_empty() {
        return Ok(Vec::new());
    }
    let re = matcher(q)?;
    let mut found = Vec::new();
    let mut left = most;
    for path in files(root) {
        if left == 0 {
            break;
        }
        let Some(text) = read(&path) else { continue };
        let hits = hits(&re, &text, left);
        if !hits.is_empty() {
            left -= hits.len();
            found.push(FileHits { path, hits });
        }
    }
    Ok(found)
}

/// The text with the matches of a query replaced, and how many there were. For a regular expression `with` may name
/// its groups (`$1`, `${name}`); otherwise it is put as it is. An empty query replaces nothing.
pub fn replace(text: &str, q: &Query, with: &str) -> Result<(String, usize), String> {
    if q.text.is_empty() {
        return Ok((text.to_string(), 0));
    }
    let re = matcher(q)?;
    let mut out = String::with_capacity(text.len());
    let (mut last, mut count) = (0, 0);
    for caps in re.captures_iter(text) {
        let m = caps.get(0).expect("the whole match");
        if m.is_empty() {
            continue;
        }
        out.push_str(&text[last..m.start()]);
        if q.regex {
            caps.expand(with, &mut out);
        } else {
            out.push_str(with);
        }
        last = m.end();
        count += 1;
    }
    out.push_str(&text[last..]);
    Ok((out, count))
}

/// The files searched under `root`, in natural order of their paths.
fn files(root: &Path) -> impl Iterator<Item = PathBuf> {
    WalkDir::new(root)
        .sort_by(|a, b| natural(&a.file_name().to_string_lossy(), &b.file_name().to_string_lossy()))
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            e.depth() == 0 || !(name.starts_with('.') || (e.file_type().is_dir() && SKIPPED.contains(&name.as_ref())))
        })
        .flatten()
        .filter(|e| e.file_type().is_file() && searched(e.path()))
        .filter(|e| e.metadata().is_ok_and(|m| m.len() <= LARGEST))
        .map(walkdir::DirEntry::into_path)
}

/// The text of a file, when it is UTF-8.
fn read(path: &Path) -> Option<String> {
    String::from_utf8(fs::read(path).ok()?).ok()
}

/// The matches of `re` in `text`, `most` at most; empty matches are none.
fn hits(re: &Regex, text: &str, most: usize) -> Vec<Hit> {
    // the line and the UTF-16 offset of `byte`, counted on from the last match
    let (mut byte, mut utf16, mut line, mut line_start) = (0, 0, 1, 0);
    let mut hits = Vec::new();
    for m in re.find_iter(text).filter(|m| !m.is_empty()).take(most) {
        for (i, c) in text[byte..m.start()].char_indices() {
            utf16 += c.len_utf16();
            if c == '\n' {
                line += 1;
                line_start = byte + i + 1;
            }
        }
        byte = m.start();
        let at = utf16;
        let end = at + m.as_str().encode_utf16().count();
        let line_end = text[line_start..].find('\n').map_or(text.len(), |n| line_start + n);
        let whole = text[line_start..line_end].trim_end_matches('\r');
        // the match on its first line, up to the line's end
        let within = |b: usize| b.min(line_start + whole.len()) - line_start;
        let (cut, from, to) = cut(whole, within(m.start()), within(m.end()));
        hits.push(Hit { line, from, to, text: cut, at, end });
    }
    hits
}

/// A line cut to `WIDTH` characters around the match at bytes `start..end` of it, "…" where it was cut; the match in
/// the cut line in UTF-16 code units.
fn cut(line: &str, start: usize, end: usize) -> (String, usize, usize) {
    let chars = |s: &str| s.chars().count();
    let (n, a) = (chars(line), chars(&line[..start]));
    let (lo, hi) = if n <= WIDTH {
        (0, n)
    } else {
        let lo = a.saturating_sub(BEFORE);
        let hi = (lo + WIDTH).min(n);
        (hi - WIDTH, hi)
    };
    // the character `i` of the line as a byte offset
    let byte = |i: usize| line.char_indices().nth(i).map_or(line.len(), |(b, _)| b);
    let (lo_byte, hi_byte) = (byte(lo), byte(hi));
    let lead = if lo > 0 { "…" } else { "" };
    let text = format!("{lead}{}{}", &line[lo_byte..hi_byte], if hi < n { "…" } else { "" });
    let utf16 = |s: &str| s.encode_utf16().count();
    let from = utf16(lead) + utf16(&line[lo_byte..start.max(lo_byte)]);
    let to = utf16(lead) + utf16(&line[lo_byte..end.clamp(lo_byte, hi_byte)]);
    (text, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fresh, empty folder.
    fn folder() -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("pw-search-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn q(text: &str, case: bool, word: bool, regex: bool) -> Query {
        Query { text: text.into(), case, word, regex }
    }

    /// The matched text of each hit, by `at..end` in `text` (UTF-16).
    fn matched(text: &str, hits: &[Hit]) -> Vec<String> {
        let units: Vec<u16> = text.encode_utf16().collect();
        hits.iter().map(|h| String::from_utf16(&units[h.at..h.end]).unwrap()).collect()
    }

    /// The matched text of each hit, by `from..to` in its cut line (UTF-16).
    fn shown(hits: &[Hit]) -> Vec<String> {
        hits.iter()
            .map(|h| String::from_utf16(&h.text.encode_utf16().collect::<Vec<_>>()[h.from..h.to]).unwrap())
            .collect()
    }

    fn find(text: &str, query: &Query) -> Vec<Hit> {
        hits(&matcher(query).unwrap(), text, usize::MAX)
    }

    #[test]
    fn literal_text_case_aside_unless_it_matters() {
        let text = "Ice (sheet) and ice.\nICE sheets";
        assert_eq!(matched(text, &find(text, &q("ice", false, false, false))), ["Ice", "ice", "ICE"]);
        assert_eq!(matched(text, &find(text, &q("ice", true, false, false))), ["ice"]);
        // the text is not an expression
        assert_eq!(matched(text, &find(text, &q("(sheet)", true, false, false))), ["(sheet)"]);
        assert!(find(text, &q("i.e", false, false, false)).is_empty());
        let hits = find(text, &q("sheet", false, false, false));
        assert_eq!(hits.iter().map(|h| h.line).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(hits[1].text, "ICE sheets");
        assert_eq!((hits[1].from, hits[1].to), (4, 9));
    }

    #[test]
    fn whole_words_only() {
        let text = "sheet sheets icesheet sheet.";
        assert_eq!(find(text, &q("sheet", true, true, false)).iter().map(|h| h.at).collect::<Vec<_>>(), [0, 22]);
        assert_eq!(find(text, &q("sheets?", true, true, true)).len(), 3);
    }

    #[test]
    fn regular_expressions_and_their_groups_replaced() {
        let text = "see Fig. 2 and Fig. 10\nFig. x";
        let query = q(r"Fig\. (\d+)", true, false, true);
        assert_eq!(matched(text, &find(text, &query)), ["Fig. 2", "Fig. 10"]);
        assert_eq!(replace(text, &query, "Figure ${1}").unwrap(), ("see Figure 2 and Figure 10\nFig. x".into(), 2));
        // line ends, whatever they are
        assert_eq!(matched("a1\r\nb2\r\n", &find("a1\r\nb2\r\n", &q(r"\d$", true, false, true))), ["1", "2"]);
        // a match across lines shows on its first
        let hits = find("a1\r\nb2", &q(r"1\r\nb", true, false, true));
        assert_eq!((hits[0].line, hits[0].text.as_str(), hits[0].from, hits[0].to, hits[0].end - hits[0].at), (1, "a1", 1, 2, 4));
        assert_eq!(find("a\r\nb", &q(r"\n", true, false, true))[0].text, "a");
        // the text as it is when it is not an expression, `$1` too
        assert_eq!(replace("a.b a.b", &q("a.b", true, false, false), "$1").unwrap(), ("$1 $1".into(), 2));
        assert_eq!(replace("aXb", &q("x", false, false, false), "-").unwrap(), ("a-b".into(), 1));
        // empty matches are none
        assert_eq!(replace("abc", &q("x*", true, false, true), "-").unwrap(), ("abc".into(), 0));
        assert!(find("abc", &q("x*", true, false, true)).is_empty());
        assert!(matcher(&q("(", true, false, true)).is_err());
    }

    #[test]
    fn positions_in_utf16_with_text_that_is_not_ascii() {
        let text = "Die Größe 🧊 der Größe\n🧊🧊 Größe";
        let hits = find(text, &q("größe", false, false, false));
        assert_eq!(matched(text, &hits), ["Größe", "Größe", "Größe"]);
        // the emoji is two code units
        assert_eq!(hits.iter().map(|h| (h.at, h.end)).collect::<Vec<_>>(), [(4, 9), (17, 22), (28, 33)]);
        assert_eq!(hits[2].line, 2);
        assert_eq!((hits[2].from, hits[2].to), (5, 10));
        assert_eq!(shown(&hits), ["Größe", "Größe", "Größe"]);
    }

    #[test]
    fn long_lines_cut_around_the_match() {
        let line = format!("{}Größe{}", "ä".repeat(300), "b".repeat(300));
        let hits = find(&line, &q("Größe", true, false, false));
        assert_eq!(hits[0].text.chars().count(), WIDTH + 2);
        assert!(hits[0].text.starts_with('…') && hits[0].text.ends_with('…'));
        assert_eq!((hits[0].from, hits[0].to), (1 + BEFORE, 1 + BEFORE + 5));
        assert_eq!(shown(&hits), ["Größe"]);
        // near the end of the line the cut begins earlier
        let line = format!("{}🧊 end", "a".repeat(400));
        let hits = find(&line, &q("end", true, false, false));
        assert_eq!(hits[0].text.chars().count(), WIDTH + 1);
        assert!(hits[0].text.starts_with('…') && hits[0].text.ends_with("🧊 end"));
        assert_eq!(shown(&hits), ["end"]);
        // a match longer than the cut is shown as far as it goes
        let line = "x".repeat(500);
        let hits = find(&line, &q("x+", true, false, true));
        assert_eq!((hits[0].from, hits[0].to, hits[0].end), (0, WIDTH, 500));
        // a short line whole
        assert_eq!(find("a short line", &q("short", true, false, false))[0].text, "a short line");
    }

    #[test]
    fn the_files_searched_and_those_left_out() {
        let root = folder();
        for dir in ["build", "_build", "node_modules", "target", ".git", ".papers-write", "sections", "notes/build"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in [
            "paper.md", "refs.bib", "sections/2-methods.tex", "sections/10-results.tex", "sections/intro.md",
            "build/paper.tex", "_build/x.md", "node_modules/x.md", "target/x.txt", ".git/x.md", ".papers-write/x.md",
            ".hidden.md", "fig.png", "notes/build/x.md", "data.json",
        ] {
            fs::write(root.join(file), "the ice sheet").unwrap();
        }
        fs::write(root.join("latin1.txt"), b"ice \xe9t\xe9").unwrap();
        fs::write(root.join("large.csv"), format!("ice,{}", "x".repeat(LARGEST as usize))).unwrap();
        let found = search(&root, &q("ice", true, false, false), 100).unwrap();
        let names: Vec<_> = found.iter().map(|f| f.path.strip_prefix(&root).unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["paper.md", "refs.bib", "sections/2-methods.tex", "sections/10-results.tex", "sections/intro.md"]);
        assert!(found.iter().all(|f| f.hits.len() == 1 && f.hits[0].at == 4));
        assert!(search(&root, &q("", true, false, false), 100).unwrap().is_empty());
        assert!(search(&root, &q("(", true, false, true), 100).is_err());
        assert!(searched(Path::new("x/Paper.MD")) && searched(Path::new("style.sty")));
        assert!(!searched(Path::new("x/.paper.md")) && !searched(Path::new("x/paper.pdf")));
    }

    #[test]
    fn so_many_hits_at_most() {
        let root = folder();
        for name in ["a.md", "b.md", "c.md"] {
            fs::write(root.join(name), "ice ice\nice").unwrap();
        }
        let found = search(&root, &q("ice", true, false, false), 4).unwrap();
        assert_eq!(found.iter().map(|f| f.hits.len()).collect::<Vec<_>>(), [3, 1]);
        assert_eq!(search(&root, &q("ice", true, false, false), 9).unwrap().len(), 3);
    }
}
