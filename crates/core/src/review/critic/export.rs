//! The Markdown with the open suggestions and comments of a review as CriticMarkup: `{++new++}`, `{--old--}`,
//! `{~~old~>new~~}` (its note `{>>author: note<<}` right after it), and a comment as `{==text==}{>>author: comment<<}`.
//! Marks are never one inside another: a comment on text that a suggestion or another comment marks (or on no text)
//! is `{>>author: comment<<}` alone, at the end of the text it is on (after the mark it would be inside).
//!
//! [`in_text`] writes them as a project that keeps its review in the text has them: each mark signed
//! `{>>@Jane Doe:<<}`, and none on or against a mark the text has already.

use crate::review::Status;
use crate::review::fold::{Item, State};
use crate::review::record::Kind;
use crate::review::text::say_change;
use std::ops::Range;

/// What an export makes.
#[derive(Debug)]
pub struct Export {
    pub markdown: String,
    /// the suggestions and comments marked
    pub suggestions: usize,
    pub comments: usize,
    /// the suggestions left out (outdated, orphaned, or overlapping an earlier one), one line each
    pub skipped: Vec<String>,
}

/// `current` with the open suggestions and comments of `items` (the review folded onto `current`) as CriticMarkup.
pub fn markdown_with_changes(current: &str, items: &[Item]) -> Export {
    marked(current, items, false)
}

/// `current` with the open suggestions and comments of `items` as the marks of a project that keeps its review in
/// the text: each mark followed by its author, `{>>@Jane Doe:<<}` (a note after the colon), which is how the editor
/// reads whose a mark is. A suggestion on, or right against, a mark that `current` has already is left out (listed
/// in `skipped`): the two would run into each other.
pub fn in_text(current: &str, items: &[Item]) -> Export {
    marked(current, items, true)
}

/// The note that says whose a mark is: `{>>@Jane Doe: note<<}`, `{>>@Jane Doe:<<}` without a note (a colon in the
/// name is taken out: it would end the name).
fn signed(author: &str, note: &str) -> String {
    let author = author.replace(':', "");
    let note = note.trim();
    if note.is_empty() { format!("{{>>@{author}:<<}}") } else { format!("{{>>@{author}: {note}<<}}") }
}

/// The marks `text` has already, each from its `{` to its closing delimiter (one not closed is not a mark).
fn marks_in(text: &str) -> Vec<Range<usize>> {
    const MARKS: [(&str, &str); 5] = [("{++", "++}"), ("{--", "--}"), ("{~~", "~~}"), ("{==", "==}"), ("{>>", "<<}")];
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = text[i..].find('{') {
        let start = i + rel;
        i = start + 1;
        let Some((open, close)) = MARKS.iter().find(|(o, _)| text[start..].starts_with(o)) else { continue };
        if let Some(len) = text[start + open.len()..].find(close) {
            let end = start + open.len() + len + close.len();
            out.push(start..end);
            i = end;
        }
    }
    out
}

/// The marks of `items` in `current`; `in_text`: signed, and kept off the marks `current` has.
fn marked(current: &str, items: &[Item], in_text: bool) -> Export {
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let (mut skipped, mut suggestions, mut comments) = (Vec::new(), 0, 0);
    // the text marked so far
    let mut taken: Vec<Range<usize>> = Vec::new();
    let overlaps = |taken: &[Range<usize>], r: &Range<usize>| taken.iter().any(|t| r.start < t.end && t.start < r.end);
    // the marks the text has: a suggestion that touches one would go inside it, or take its note
    let there = if in_text { marks_in(current) } else { Vec::new() };
    let against = |r: &Range<usize>| there.iter().any(|t| r.start <= t.end && t.start <= r.end);
    let fits = |r: &Range<usize>| r.start <= r.end && r.end <= current.len() && current.is_char_boundary(r.start) && current.is_char_boundary(r.end);
    let open = || items.iter().filter(|i| i.state == State::Open);
    for item in open().filter(|i| i.kind == Kind::Suggestion) {
        let r = item.range.clone();
        let change = || say_change(item.record.exact.as_deref().unwrap_or(""), item.replacement.as_deref().unwrap_or(""));
        if !fits(&r) || overlaps(&taken, &r) || !matches!(item.status, Status::Exact | Status::Moved) {
            skipped.push(format!("{}: {} (outdated, or overlapping another)", item.author, change()));
            continue;
        }
        if against(&r) {
            skipped.push(format!("{}: {} (on or against a mark the text has)", item.author, change()));
            continue;
        }
        let (old, new) = (&current[r.clone()], item.replacement.as_deref().unwrap_or(""));
        let mut text = match (old.is_empty(), new.is_empty()) {
            (true, true) => continue,
            (true, false) => format!("{{++{new}++}}"),
            (false, true) => format!("{{--{old}--}}"),
            (false, false) => format!("{{~~{old}~>{new}~~}}"),
        };
        let note = item.record.text.as_deref().map(str::trim).filter(|t| !t.is_empty());
        if in_text {
            text.push_str(&signed(&item.author, note.unwrap_or("")));
        } else if let Some(note) = note {
            text.push_str(&format!("{{>>{}: {note}<<}}", item.author));
        }
        taken.push(r.clone());
        edits.push((r, text));
        suggestions += 1;
    }
    for item in open().filter(|i| i.kind == Kind::Comment && fits(&i.range)) {
        let r = item.range.clone();
        let body = item.record.text.as_deref().unwrap_or("").trim();
        let note = if in_text { signed(&item.author, body) } else { format!("{{>>{}: {body}<<}}", item.author) };
        if r.is_empty() || overlaps(&taken, &r) || current[r.clone()].contains("==}") {
            // at the end of the text it is on, after a mark it would be inside
            let at = taken.iter().find(|t| t.start < r.end && r.end < t.end).map_or(r.end, |t| t.end);
            edits.push((at..at, note));
        } else {
            taken.push(r.clone());
            edits.push((r.clone(), format!("{{=={}==}}{note}", &current[r])));
        }
        comments += 1;
    }
    // from the end, so the offsets stay good (nothing overlaps: the marks were checked)
    edits.sort_by_key(|(r, _)| std::cmp::Reverse((r.start, r.end)));
    let mut markdown = current.to_string();
    for (r, text) in edits {
        markdown.replace_range(r, &text);
    }
    Export { markdown, suggestions, comments, skipped }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::testutil::{doc, make};
    use crate::review::{Person, Review, Record};

    const TEXT: &str = "The grounding line retreats rapidly when the bed slopes.\nWe show this in Fig. 3.";

    fn put(d: &std::path::Path, me: &Person, r: Record) {
        crate::review::store::append(&crate::review::store::person_path(d, &crate::review::text::slug(&me.name)), me, &r).unwrap();
    }

    #[test]
    fn suggestions_and_comments_as_critic_markup() {
        let d = doc("paper.md");
        let jane = Person::new("Jane Doe", "");
        crate::review::store::write_base(&d, TEXT).unwrap();
        put(&d, &jane, make("jane-doe-1", Kind::Suggestion, TEXT, "rapidly", Some("slowly")));
        put(&d, &jane, make("jane-doe-2", Kind::Suggestion, TEXT, "The ", Some("")));
        put(&d, &jane, make("jane-doe-3", Kind::Suggestion, TEXT, "", Some("Note: ")));
        put(&d, &jane, make("jane-doe-4", Kind::Comment, TEXT, "Fig. 3", None));
        let items = Review::load(&d).unwrap().items(TEXT);
        let made = markdown_with_changes(TEXT, &items);
        assert!(made.skipped.is_empty(), "{:?}", made.skipped);
        assert_eq!((made.suggestions, made.comments), (3, 1));
        assert_eq!(
            made.markdown,
            "{++Note: ++}{--The --}grounding line retreats {~~rapidly~>slowly~~} when the bed slopes.\nWe show this in {==Fig. 3==}{>>Jane Doe: a comment<<}."
        );
    }

    #[test]
    fn in_the_text_each_mark_is_signed_and_kept_off_the_marks_there() {
        const MARKED: &str = "The grounding line {++now ++}{>>@MP<<}retreats rapidly when the bed slopes.";
        let d = doc("paper.md");
        let jane = Person::new("Jane Doe", "");
        crate::review::store::write_base(&d, MARKED).unwrap();
        put(&d, &jane, make("jane-doe-1", Kind::Suggestion, MARKED, "rapidly", Some("slowly")));
        put(&d, &jane, make("jane-doe-2", Kind::Suggestion, MARKED, "retreats", Some("moves")));
        put(&d, &jane, make("jane-doe-3", Kind::Comment, MARKED, "bed", None));
        let items = Review::load(&d).unwrap().items(MARKED);
        let made = in_text(MARKED, &items);
        assert_eq!((made.suggestions, made.comments), (1, 1));
        assert_eq!(made.skipped.len(), 1, "{:?}", made.skipped);
        assert!(made.skipped[0].contains("against a mark"), "{:?}", made.skipped);
        assert_eq!(
            made.markdown,
            "The grounding line {++now ++}{>>@MP<<}retreats {~~rapidly~>slowly~~}{>>@Jane Doe:<<} when the {==bed==}{>>@Jane Doe: a comment<<} slopes."
        );
    }

    #[test]
    fn what_goes_out_comes_back() {
        let d = doc("paper.md");
        let (jane, ann) = (Person::new("Jane Doe", ""), Person::new("Ann Lee", ""));
        crate::review::store::write_base(&d, TEXT).unwrap();
        let mut s = make("jane-doe-1", Kind::Suggestion, TEXT, "rapidly", Some("slowly"));
        s.text = Some("too strong".into());
        put(&d, &jane, s);
        put(&d, &ann, make("ann-lee-1", Kind::Comment, TEXT, "Fig. 3", None));
        // on text the suggestion marks: alone, after the text it is on
        put(&d, &ann, make("ann-lee-2", Kind::Comment, TEXT, "rapidly when", None));
        let items = Review::load(&d).unwrap().items(TEXT);
        let made = markdown_with_changes(TEXT, &items);
        assert!(made.skipped.is_empty(), "{:?}", made.skipped);
        let out = made.markdown;
        assert_eq!(
            out,
            "The grounding line retreats {~~rapidly~>slowly~~}{>>Jane Doe: too strong<<} when{>>Ann Lee: a comment<<} the bed slopes.\n\
             We show this in {==Fig. 3==}{>>Ann Lee: a comment<<}."
        );
        let back = super::super::import(TEXT, &out, &["Jane Doe".into(), "Ann Lee".into()], "Bob", false);
        assert!(back.unplaced.is_empty(), "{:?}", back.unplaced);
        let at = |needle: &str| (TEXT.find(needle).unwrap(), TEXT.find(needle).unwrap() + needle.len());
        let got: Vec<_> = back.suggestions.iter().chain(&back.comments).map(|(who, r)| (who.as_str(), r.from, r.to, r.insert.clone(), r.text.clone())).collect();
        let (rapidly, fig, when) = (at("rapidly"), at("Fig. 3"), at("rapidly when").1);
        assert_eq!(
            got,
            [
                ("Jane Doe", rapidly.0, rapidly.1, Some("slowly".to_string()), Some("too strong".to_string())),
                ("Ann Lee", when, when, None, Some("a comment".to_string())),
                ("Ann Lee", fig.0, fig.1, None, Some("a comment".to_string())),
            ]
        );
    }
}
