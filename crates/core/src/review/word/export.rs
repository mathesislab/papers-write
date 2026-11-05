//! The Markdown with the open suggestions and comments of a review as pandoc spans, which pandoc's docx writer makes
//! tracked changes and comments of.
//!
//! - a suggestion: `[old]{.deletion author=".." date=".."}[new]{.insertion author=".." date=".."}` (either half left
//!   out when empty); white space at the edges of the changed text stays outside the span (a span loses it);
//! - a comment: `[body]{.comment-start id="1" author=".." date=".."}text[]{.comment-end id="1"}`.
//!
//! Not inside math, code, raw LaTeX or an HTML comment (pandoc does not read a span there as one): a suggestion
//! there is kept as a comment that says what it suggests, at the end of the region (before it, as a paragraph of its
//! own, for a block). The same for text that cannot be put in a span (unbalanced brackets, a paragraph break).
//! A suggestion that overlaps an earlier one is left out, and listed.

use super::protect::{Kind as Protected, Region, regions};
use crate::review::fold::{Item, State};
use crate::review::record::Kind;
use std::ops::Range;

/// The Markdown for pandoc, and what was left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Export {
    pub markdown: String,
    /// suggestions as tracked changes, comments (a suggestion kept as a comment counts as one)
    pub suggestions: usize,
    pub comments: usize,
    /// what could not go to Word, one line each
    pub skipped: Vec<String>,
}

/// A comment to place: on a range (or a point), or by the region it cannot be inside.
struct Note<'a> {
    range: Range<usize>,
    region: Option<&'a Region>,
    author: String,
    date: String,
    text: String,
}

/// A change of the text, at `pos`: `text` put there, and the text up to `end` taken out.
struct Edit {
    pos: usize,
    end: usize,
    /// at the same place: a comment's end, then its start (or a paragraph), then a suggestion
    rank: u8,
    text: String,
}

fn quote(s: &str) -> String {
    let flat: String = s.chars().map(|c| if c == '"' { '\'' } else if c == '\n' || c == '\r' { ' ' } else { c }).collect();
    flat.trim().to_string()
}

fn attrs(author: &str, date: &str) -> String {
    format!("author=\"{}\" date=\"{}\"", quote(author), quote(date))
}

/// A comment's text as the content of a span: its Markdown specials escaped, paragraphs on one line.
fn body(text: &str) -> String {
    let mut out = String::new();
    for (i, para) in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()).enumerate() {
        if i > 0 {
            out.push_str(" / ");
        }
        for c in para.chars() {
            match c {
                '\n' | '\r' => out.push(' '),
                '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '$' | '#' | '@' | '~' | '^' | '|' | '!' => {
                    out.push('\\');
                    out.push(c);
                }
                c => out.push(c),
            }
        }
    }
    if out.is_empty() { "(empty)".into() } else { out }
}

fn balanced(s: &str) -> bool {
    let mut depth = 0i32;
    let mut escaped = false;
    for c in s.chars() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0 && !escaped
}

/// Whether `s` can be the content of a span.
fn spannable(s: &str) -> bool {
    balanced(s) && !s.contains("\n\n") && !s.contains("\r\n\r\n") && !s.contains('{') && !s.contains('}')
}

fn short(s: &str) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 60 { format!("{}…", flat.chars().take(60).collect::<String>()) } else { flat }
}

fn lead(s: &str) -> &str {
    &s[..s.len() - s.trim_start().len()]
}

fn trail(s: &str) -> &str {
    &s[s.trim_end().len()..]
}

/// What is said of a suggestion that is not a tracked change.
fn describe(old: &str, new: &str, why: &str) -> String {
    let what = match (old.trim().is_empty(), new.trim().is_empty()) {
        (true, _) => format!("insert “{}”", short(new)),
        (_, true) => format!("delete “{}”", short(old)),
        _ => format!("replace “{}” with “{}”", short(old), short(new)),
    };
    format!("Suggestion, not shown as a tracked change ({why}): {what}")
}

/// `current` with the open suggestions and comments of `items` (the review folded onto `current`) as pandoc spans.
pub fn markdown_with_changes(current: &str, items: &[Item]) -> Export {
    let guarded = regions(current);
    let touching = |s: usize, e: usize| guarded.iter().find(|r| r.touches(s, e));
    let mut export = Export::default();
    let mut edits: Vec<Edit> = Vec::new();
    // the suggestions that are tracked changes, by range
    let mut replaced: Vec<Range<usize>> = Vec::new();
    // comments (or notes) to place: range or point, author, date, text
    let mut comments: Vec<Note> = Vec::new();

    let mut open: Vec<&Item> = items.iter().filter(|i| i.state == State::Open).collect();
    open.sort_by_key(|i| (i.range.start, i.range.end));
    for item in open {
        let (s, e) = (item.range.start, item.range.end);
        let date = item.record.time.as_str();
        if item.status == crate::review::Status::Orphaned || e > current.len() || !current.is_char_boundary(s) || !current.is_char_boundary(e) {
            export.skipped.push(format!("{}: {} whose text is gone", item.author, if item.kind == Kind::Comment { "a comment" } else { "a suggestion" }));
            continue;
        }
        if item.kind == Kind::Comment {
            comments.push(Note {
                range: s..e,
                region: touching(s, e),
                author: item.author.clone(),
                date: date.into(),
                text: body(item.record.text.as_deref().unwrap_or("")),
            });
            continue;
        }
        if item.kind != Kind::Suggestion {
            continue;
        }
        let (old, new) = (&current[s..e], item.replacement.as_deref().unwrap_or(""));
        if item.on.is_some() {
            export.skipped.push(format!("{}: a counter-suggestion ({})", item.author, describe(old, new, "")));
            continue;
        }
        if old.trim().is_empty() && new.trim().is_empty() {
            export.skipped.push(format!("{}: a change of white space only", item.author));
            continue;
        }
        if let Some(region) = touching(s, e) {
            if region.kind == Protected::FrontMatter {
                export.skipped.push(format!("{}: {}", item.author, describe(old, new, "in the front matter")));
            } else {
                let note = body(&describe(old, new, &format!("in {}", region.kind.name())));
                comments.push(Note { range: s..e, region: Some(region), author: item.author.clone(), date: date.into(), text: note });
            }
            continue;
        }
        let (old_core, new_core) = (old.trim(), new.trim());
        if !spannable(old_core) || !spannable(new_core) {
            let note = body(&describe(old, new, "it cannot be put in a span"));
            comments.push(Note { range: s..e, region: None, author: item.author.clone(), date: date.into(), text: note });
            continue;
        }
        if replaced.iter().any(|r| overlap(r, &(s..e))) {
            export.skipped.push(format!("{}: {} (overlaps an earlier suggestion)", item.author, describe(old, new, "")));
            continue;
        }
        let who = attrs(&item.author, date);
        let first = if lead(old).len() >= lead(new).len() { lead(old) } else { lead(new) };
        let last = if trail(old).len() >= trail(new).len() { trail(old) } else { trail(new) };
        let mut text = String::from(first);
        if !old_core.is_empty() {
            text.push_str(&format!("[{old_core}]{{.deletion {who}}}"));
        }
        if !new_core.is_empty() {
            text.push_str(&format!("[{new_core}]{{.insertion {who}}}"));
        }
        text.push_str(last);
        edits.push(Edit { pos: s, end: e, rank: 2, text });
        replaced.push(s..e);
        export.suggestions += 1;
    }

    // comments: a point inside a tracked change moves to its edge
    let clamp = |p: usize, start: bool| {
        replaced.iter().find(|r| r.start < p && p < r.end).map_or(p, |r| if start { r.start } else { r.end })
    };
    for (n, Note { range, region, author, date, text }) in comments.into_iter().enumerate() {
        let id = n + 1;
        let who = attrs(&author, &date);
        let (open, close) = (format!("[{text}]{{.comment-start id=\"{id}\" {who}}}"), format!("[]{{.comment-end id=\"{id}\"}}"));
        export.comments += 1;
        match region {
            // a comment on a region it cannot be inside: a point after it, or a paragraph before a block
            Some(r) if r.block => {
                let before = &current[..r.range.start];
                let lead = if before.is_empty() || before.ends_with("\n\n") { "" } else { "\n" };
                edits.push(Edit { pos: r.range.start, end: r.range.start, rank: 1, text: format!("{lead}{open}{close}\n\n") });
            }
            Some(r) => {
                edits.push(Edit { pos: r.range.end, end: r.range.end, rank: 1, text: format!("{open}{close}") });
            }
            None => {
                let (a, b) = (clamp(range.start, true), clamp(range.end, false));
                edits.push(Edit { pos: b, end: b, rank: 0, text: close });
                edits.push(Edit { pos: a, end: a, rank: 1, text: open });
            }
        }
    }

    edits.sort_by_key(|e| (e.pos, e.rank));
    let mut out = String::with_capacity(current.len() + edits.len() * 80);
    let mut at = 0;
    for e in edits {
        let pos = e.pos.max(at);
        out.push_str(&current[at..pos]);
        out.push_str(&e.text);
        at = e.end.max(pos);
    }
    out.push_str(&current[at..]);
    export.markdown = out;
    export
}

fn overlap(a: &Range<usize>, b: &Range<usize>) -> bool {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => false,
        (true, false) => b.start < a.start && a.start < b.end,
        (false, true) => a.start < b.start && b.start < a.end,
        (false, false) => a.start < b.end && b.start < a.end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::ops::NewRecord;
    use crate::review::record::Person;
    use crate::review::testutil::doc;
    use crate::review::{Review, utf16_of};

    /// The items of a review of `text`: `(needle, replacement)` suggestions by Jane, comments by Bob.
    fn items(text: &str, suggest: &[(&str, &str)], comment: &[(&str, &str)]) -> Vec<Item> {
        let d = doc("paper.md");
        let jane = Person::new("Jane Doe", "");
        let bob = Person::new("Bob", "");
        let at = |needle: &str| {
            let s = text.find(needle).unwrap();
            (utf16_of(text, s), utf16_of(text, s + needle.len()))
        };
        for (n, r) in suggest {
            let (a, b) = at(n);
            Review::add(&d, &jane, text, NewRecord::suggestion(a, b, *r)).unwrap();
        }
        for (n, c) in comment {
            let (a, b) = at(n);
            Review::add(&d, &bob, text, NewRecord::comment(a, b, *c)).unwrap();
        }
        Review::load(&d).unwrap().items(text)
    }

    #[test]
    fn spans_are_placed() {
        let t = "The grounding line retreats rapidly when the bed slopes.\n";
        let it = items(t, &[("rapidly", "quickly")], &[("grounding line", "Cite X [here]?")]);
        let x = markdown_with_changes(t, &it);
        assert_eq!(x.suggestions, 1);
        assert_eq!(x.comments, 1);
        assert!(x.skipped.is_empty());
        let m = &x.markdown;
        assert!(m.contains("[rapidly]{.deletion author=\"Jane Doe\" date=\"2026-"), "{m}");
        assert!(m.contains("[quickly]{.insertion author=\"Jane Doe\""), "{m}");
        assert!(m.contains("The [Cite X \\[here\\]?]{.comment-start id=\"1\" author=\"Bob\""), "{m}");
        assert!(m.contains("grounding line[]{.comment-end id=\"1\"} retreats"), "{m}");
    }

    #[test]
    fn white_space_at_the_edges_stays_outside() {
        let t = "The line retreats now.\n";
        // an insertion with the space before it, and a deletion with the space before it
        let it = items(t, &[("retreats", "retreats rapidly")], &[]);
        let m = markdown_with_changes(t, &it).markdown;
        assert!(m.contains("[retreats]{.deletion"), "{m}");
        let d = doc("p2.md");
        let jane = Person::new("Jane", "");
        let at = t.find(" now").unwrap();
        Review::add(&d, &jane, t, NewRecord::suggestion(at, at + 4, "")).unwrap();
        let it = Review::load(&d).unwrap().items(t);
        let m = markdown_with_changes(t, &it).markdown;
        assert!(m.lines().next().unwrap().split("retreats").nth(1).unwrap().starts_with(" [now]{.deletion"), "{m}");
    }

    #[test]
    fn math_code_and_latex_are_not_spanned() {
        let t = "Let $x^2$ grow, see `f(x)` and \\cite{smith}.\n\n$$E = mc^2$$\n";
        let it = items(t, &[("x^2", "y^2"), ("f(x)", "g(x)"), ("smith", "jones"), ("E = mc^2", "E = m")], &[]);
        let x = markdown_with_changes(t, &it);
        assert_eq!(x.suggestions, 0);
        assert_eq!(x.comments, 4);
        let m = &x.markdown;
        assert!(!m.contains("{.deletion"), "{m}");
        assert!(m.contains("$x^2$[Suggestion, not shown as a tracked change (in math): replace “x\\^2” with “y\\^2”]{.comment-start id=\"1\""), "{m}");
        assert!(m.contains("[]{.comment-end id=\"4\"}\n\n$$E = mc^2$$"), "{m}");
    }

    #[test]
    fn overlapping_suggestions_leave_the_later_out() {
        let t = "The grounding line retreats rapidly now.\n";
        let it = items(t, &[("line retreats", "ice retreats"), ("retreats rapidly", "moves quickly")], &[]);
        let x = markdown_with_changes(t, &it);
        assert_eq!(x.suggestions, 1);
        assert_eq!(x.skipped.len(), 1);
        assert!(x.skipped[0].contains("overlaps"), "{:?}", x.skipped);
    }

    #[test]
    fn a_comment_beside_a_suggestion_moves_to_its_edge() {
        let t = "The grounding line retreats rapidly now.\n";
        let it = items(t, &[("grounding line retreats", "ice retreats")], &[("line", "Which line?")]);
        let m = markdown_with_changes(t, &it).markdown;
        // the comment's range lies inside the tracked change: it takes the change whole
        let (a, b) = (m.find("comment-start").unwrap(), m.find("comment-end").unwrap());
        let (del, ins) = (m.find("{.deletion").unwrap(), m.find("{.insertion").unwrap());
        assert!(a < del && ins < b, "{m}");
    }
}
