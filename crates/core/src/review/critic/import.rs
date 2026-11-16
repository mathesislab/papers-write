//! A file of CriticMarkup read back as suggestions and comments on the Markdown.
//!
//! - `{++new++}`, `{--old--}` and `{~~old~>new~~}` are suggestions; a `{>>note<<}` right after one is its note (as
//!   the export writes it).
//! - `{==text==}{>>note<<}` is a comment on the text, and `{>>note<<}` alone a comment at its place. A highlight with
//!   no comment after it is left out, and listed; a mark that is not closed stays text, and is listed.
//! - Whose: the note's `Name: ` when Name is someone of the review (as the export writes it) or is written
//!   `@Name: `; else the name the import is given.
//!
//! The file with every mark rejected is the text the reviewer had. Where it is the document's text (the usual case),
//! the marks stand where they are. Where the document has changed since, their places are carried through the
//! word-level difference, and a mark in a stretch that changed is listed, not guessed. With `untracked`, what the
//! reviewer changed outside the marks (the difference from the document to that text) is suggested too.

use crate::review::Imported;
use crate::review::compare;
use crate::review::diff::Diff;
use crate::review::ops::NewRecord;
use crate::review::text::{say_change, short, utf16_of};
use std::ops::Range;

/// Whose a mark is when its note names no one, and the import is given no name.
pub const CRITIC: &str = "CriticMarkup";

/// A mark: its place in the text with the marks rejected (bytes), and the line of the file it is on.
#[derive(Debug, PartialEq)]
enum Mark {
    Suggestion { at: Range<usize>, insert: String, note: Option<String>, line: usize },
    Comment { at: Range<usize>, note: String, line: usize },
}

/// A file read: the text with the marks rejected, the marks in order, and what could not be read.
#[derive(Debug, Default)]
struct Marked {
    original: String,
    marks: Vec<Mark>,
    problems: Vec<String>,
}

/// The marks, and how each is closed.
const MARKS: [(&str, &str); 5] = [("{++", "++}"), ("{--", "--}"), ("{~~", "~~}"), ("{==", "==}"), ("{>>", "<<}")];

/// What a comment right after the last mark belongs to.
enum Last {
    /// the suggestion `marks[k]`: the comment is its note
    Suggestion(usize),
    /// a highlight of this range (on this line): the comment is on it
    Highlight(Range<usize>, usize),
}

fn line_of(text: &str, byte: usize) -> usize {
    text[..byte].matches('\n').count() + 1
}

fn read(file: &str) -> Marked {
    let mut out = Marked::default();
    // the last mark, with the byte of the file where it ends: a comment that begins there belongs to it
    let mut last: Option<(usize, Last)> = None;
    let mut i = 0;
    while let Some(rel) = file[i..].find('{') {
        let start = i + rel;
        let Some(&(open, close)) = MARKS.iter().find(|(o, _)| file[start..].starts_with(o)) else {
            out.original.push_str(&file[i..=start]);
            i = start + 1;
            continue;
        };
        let line = line_of(file, start);
        let body_at = start + open.len();
        let Some(len) = file[body_at..].find(close) else {
            out.problems.push(format!("line {line}: {open} is not closed (left as text)"));
            out.original.push_str(&file[i..body_at]);
            i = body_at;
            continue;
        };
        let body = &file[body_at..body_at + len];
        let end = body_at + len + close.len();
        out.original.push_str(&file[i..start]);
        let p = out.original.len();
        let follows = last.as_ref().is_some_and(|(e, _)| *e == start);
        match (open, last.take()) {
            ("{>>", Some((_, Last::Suggestion(k)))) if follows => {
                if let Mark::Suggestion { note, .. } = &mut out.marks[k] {
                    *note = Some(body.trim().to_string()).filter(|n| !n.is_empty());
                }
            }
            ("{>>", Some((_, Last::Highlight(at, line)))) if follows && !body.trim().is_empty() => {
                out.marks.push(Mark::Comment { at, note: body.trim().to_string(), line });
            }
            (_, before) => {
                if let Some((_, Last::Highlight(at, line))) = before {
                    out.problems.push(format!("line {line}: the highlight “{}” has no comment (left out)", short(&out.original[at])));
                }
                match open {
                    "{++" => {
                        out.marks.push(Mark::Suggestion { at: p..p, insert: body.to_string(), note: None, line });
                        last = Some((end, Last::Suggestion(out.marks.len() - 1)));
                    }
                    "{--" => {
                        out.original.push_str(body);
                        out.marks.push(Mark::Suggestion { at: p..p + body.len(), insert: String::new(), note: None, line });
                        last = Some((end, Last::Suggestion(out.marks.len() - 1)));
                    }
                    "{~~" => match body.split_once("~>") {
                        Some((old, new)) => {
                            out.original.push_str(old);
                            out.marks.push(Mark::Suggestion { at: p..p + old.len(), insert: new.to_string(), note: None, line });
                            last = Some((end, Last::Suggestion(out.marks.len() - 1)));
                        }
                        None => {
                            out.problems.push(format!("line {line}: {{~~ without ~> (left as text)"));
                            out.original.push_str(&file[start..end]);
                        }
                    },
                    "{==" => {
                        out.original.push_str(body);
                        last = Some((end, Last::Highlight(p..p + body.len(), line)));
                    }
                    _ if body.trim().is_empty() => {}
                    _ => out.marks.push(Mark::Comment { at: p..p, note: body.trim().to_string(), line }),
                }
            }
        }
        i = end;
    }
    out.original.push_str(&file[i..]);
    if let Some((_, Last::Highlight(at, line))) = last {
        out.problems.push(format!("line {line}: the highlight “{}” has no comment (left out)", short(&out.original[at])));
    }
    out
}

/// Whose a note is, and the note without the name: `Name: ` at its start when Name is one of `people` (as they
/// spell it), or `@Name: `; else it is `author`'s, all of it. A note left empty is none.
fn named(note: Option<&str>, people: &[String], author: &str) -> (String, Option<String>) {
    let note = note.map(str::trim).unwrap_or("");
    let found = note.split_once(':').and_then(|(head, rest)| {
        let head = head.trim();
        if head.contains('\n') || head.chars().count() > 60 {
            return None;
        }
        let name = match head.strip_prefix('@') {
            Some(name) => name.trim().to_string(),
            None => people.iter().find(|p| p.eq_ignore_ascii_case(head))?.clone(),
        };
        (!name.is_empty()).then(|| (name, rest.trim()))
    });
    let (who, text) = found.unwrap_or_else(|| (author.to_string(), note));
    (who, Some(text.to_string()).filter(|t| !t.is_empty()))
}

/// The CriticMarkup of `file` as suggestions and comments on `current` (positions in UTF-16 units of `current`),
/// each under the name its note begins with when that is one of `people` (the names of the review) or is written
/// `@Name:`, else `author`'s ([`CRITIC`] when empty). With `untracked`, the edits made outside the marks are `author`'s
/// suggestions too.
pub fn import(current: &str, file: &str, people: &[String], author: &str, untracked: bool) -> Imported {
    let author = if author.trim().is_empty() { CRITIC } else { author.trim() };
    let marked = read(file);
    let original = marked.original.as_str();
    let mut out = Imported { unplaced: marked.problems, ..Default::default() };
    let diff = Diff::new(original, current);
    // the place in `current` of a range of the text the reviewer had, if nothing there has changed since
    let place = |at: &Range<usize>| -> Option<(usize, usize)> {
        if diff.overlapping(at.start, at.end).next().is_some() {
            return None;
        }
        let start = diff.map_start(at.start);
        let end = if at.is_empty() { start } else { diff.map_end(at.end) };
        Some((utf16_of(current, start), utf16_of(current, end)))
    };
    for mark in marked.marks {
        match mark {
            Mark::Suggestion { at, insert, note, line } => {
                let (who, note) = named(note.as_deref(), people, author);
                let old = &original[at.clone()];
                if old == insert {
                    continue;
                }
                match place(&at) {
                    Some((from, to)) => {
                        let record = NewRecord::suggestion(from, to, insert);
                        out.suggestions.push((who, if let Some(note) = note { record.note(note) } else { record }));
                    }
                    None => out.unplaced.push(format!("line {line}, {who}: {} (the text there has changed)", say_change(old, &insert))),
                }
            }
            Mark::Comment { at, note, line } => {
                let (who, note) = named(Some(&note), people, author);
                let Some(note) = note else {
                    out.unplaced.push(format!("line {line}, {who}: a comment with nothing in it (left out)"));
                    continue;
                };
                match place(&at) {
                    Some((from, to)) => out.comments.push((who, NewRecord::comment(from, to, note))),
                    None => out.unplaced.push(format!(
                        "line {line}, {who}: comment “{}” on “{}” (the text there has changed)",
                        short(&note),
                        short(&original[at])
                    )),
                }
            }
        }
    }
    if untracked {
        out.suggestions.extend(compare::suggestions(current, original).into_iter().map(|s| (author.to_string(), s)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::Kind;

    const TEXT: &str = "The grounding line retreats rapidly when the bed slopes.\nWe show this in Fig. 3.\n";

    /// What the suggestions and comments do, said plainly: `who: old -> new (note)` and `who: on "text": note`.
    fn said(current: &str, imported: &Imported) -> Vec<String> {
        let units: Vec<u16> = current.encode_utf16().collect();
        let text = |f: usize, t: usize| String::from_utf16(&units[f..t]).unwrap();
        let mut out = Vec::new();
        for (who, r) in &imported.suggestions {
            assert_eq!(r.kind, Kind::Suggestion);
            let note = r.text.as_deref().map(|n| format!(" ({n})")).unwrap_or_default();
            out.push(format!("{who}: {:?} -> {:?}{note}", text(r.from, r.to), r.insert.as_deref().unwrap()));
        }
        for (who, r) in &imported.comments {
            assert_eq!(r.kind, Kind::Comment);
            out.push(format!("{who}: on {:?}: {}", text(r.from, r.to), r.text.as_deref().unwrap()));
        }
        out
    }

    #[test]
    fn the_marks_read() {
        let m = read("a {++b++}c {--d--}e {~~f~>g~~}h {==i==}{>>j<<} k{>>l<<}\n{++m++}{>>Jane: n<<}");
        assert_eq!(m.original, "a c de fh i k\n");
        assert_eq!(
            m.marks,
            vec![
                Mark::Suggestion { at: 2..2, insert: "b".into(), note: None, line: 1 },
                Mark::Suggestion { at: 4..5, insert: String::new(), note: None, line: 1 },
                Mark::Suggestion { at: 7..8, insert: "g".into(), note: None, line: 1 },
                Mark::Comment { at: 10..11, note: "j".into(), line: 1 },
                Mark::Comment { at: 13..13, note: "l".into(), line: 1 },
                Mark::Suggestion { at: 14..14, insert: "m".into(), note: Some("Jane: n".into()), line: 2 },
            ]
        );
        assert!(m.problems.is_empty(), "{:?}", m.problems);
    }

    #[test]
    fn what_cannot_be_read_stays_text_and_is_listed() {
        let m = read("a {==b==} c {~~d~~} e {++f\n");
        assert_eq!(m.original, "a b c {~~d~~} e {++f\n");
        assert!(m.marks.is_empty());
        assert_eq!(
            m.problems,
            ["line 1: the highlight “b” has no comment (left out)", "line 1: {~~ without ~> (left as text)", "line 1: {++ is not closed (left as text)"]
        );
        // braces of LaTeX and Markdown are no marks
        let m = read("\\textbf{x} {#sec:a} {-}");
        assert_eq!(m.original, "\\textbf{x} {#sec:a} {-}");
        assert!(m.marks.is_empty() && m.problems.is_empty());
    }

    #[test]
    fn suggestions_and_comments_where_the_marks_are() {
        let file = "The grounding line retreats {~~rapidly~>slowly~~}{>>Jane Doe: too strong<<} when the bed slopes.\n\
                    We show this in {==Fig. 3==}{>>@Ann Lee: which panel?<<}.{++ It is new.++}{>>Note: check<<}\n";
        let imported = import(TEXT, file, &["Jane Doe".into()], "Bob", false);
        assert!(imported.unplaced.is_empty(), "{:?}", imported.unplaced);
        assert_eq!(
            said(TEXT, &imported),
            [
                "Jane Doe: \"rapidly\" -> \"slowly\" (too strong)",
                "Bob: \"\" -> \" It is new.\" (Note: check)",
                "Ann Lee: on \"Fig. 3\": which panel?",
            ]
        );
    }

    #[test]
    fn positions_are_utf16_units() {
        let current = "Let 𝜃 be the small angle.\n";
        let imported = import(current, "Let 𝜃 be the {--small --}angle.{>>why?<<}\n", &[], "Bob", false);
        // 𝜃 is two units
        let ((_, s), (_, c)) = (&imported.suggestions[0], &imported.comments[0]);
        assert_eq!((s.from, s.to, c.from, c.to), (14, 20, 26, 26));
        assert_eq!(said(current, &imported), ["Bob: \"small \" -> \"\"", "Bob: on \"\": why?"]);
    }

    #[test]
    fn a_mark_where_the_document_has_changed_since_is_listed() {
        // the reviewer had the text before "rapidly" became "quickly"
        let current = TEXT.replace("rapidly", "quickly");
        let file = "The grounding line retreats {--rapidly--} when the {~~bed~>sea floor~~} slopes.\nWe show this in Fig. 3.\n";
        let imported = import(&current, file, &[], "Bob", false);
        assert_eq!(said(&current, &imported), ["Bob: \"bed\" -> \"sea floor\""]);
        assert_eq!(imported.unplaced, ["line 1, Bob: delete “rapidly” (the text there has changed)"]);
    }

    #[test]
    fn edits_outside_the_marks_only_when_asked() {
        let file = "The grounding line retreats rapidly when the bed {++gently ++}slopes.\nWe show this in Figure 3.\n";
        assert_eq!(said(TEXT, &import(TEXT, file, &[], "Bob", false)), ["Bob: \"\" -> \"gently \""]);
        assert_eq!(said(TEXT, &import(TEXT, file, &[], "Bob", true)), ["Bob: \"\" -> \"gently \"", "Bob: \"Fig.\" -> \"Figure\""]);
    }
}
