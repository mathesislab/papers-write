//! The Markdown with the open suggestions and comments of a review as CriticMarkup: `{++new++}`, `{--old--}`,
//! `{~~old~>new~~}` (its note `{>>author: note<<}` right after it), and a comment as `{==text==}{>>author: comment<<}`.
//! Marks are never one inside another: a comment on text that a suggestion or another comment marks (or on no text)
//! is `{>>author: comment<<}` alone, at the end of the text it is on (after the mark it would be inside).

use crate::review::Status;
use crate::review::fold::{Item, State};
use crate::review::record::Kind;
use std::ops::Range;

/// `current` with the open suggestions and comments of `items` (the review folded onto `current`) as CriticMarkup,
/// and the ids of the suggestions left out (outdated, orphaned, or overlapping an earlier one).
pub fn markdown_with_changes(current: &str, items: &[Item]) -> (String, Vec<String>) {
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut skipped = Vec::new();
    // the text marked so far
    let mut taken: Vec<Range<usize>> = Vec::new();
    let overlaps = |taken: &[Range<usize>], r: &Range<usize>| taken.iter().any(|t| r.start < t.end && t.start < r.end);
    let fits = |r: &Range<usize>| r.start <= r.end && r.end <= current.len() && current.is_char_boundary(r.start) && current.is_char_boundary(r.end);
    let open = || items.iter().filter(|i| i.state == State::Open);
    for item in open().filter(|i| i.kind == Kind::Suggestion) {
        let r = item.range.clone();
        if !fits(&r) || overlaps(&taken, &r) || !matches!(item.status, Status::Exact | Status::Moved) {
            skipped.push(item.id.clone());
            continue;
        }
        let (old, new) = (&current[r.clone()], item.replacement.as_deref().unwrap_or(""));
        let mut text = match (old.is_empty(), new.is_empty()) {
            (true, true) => continue,
            (true, false) => format!("{{++{new}++}}"),
            (false, true) => format!("{{--{old}--}}"),
            (false, false) => format!("{{~~{old}~>{new}~~}}"),
        };
        if let Some(note) = item.record.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            text.push_str(&format!("{{>>{}: {note}<<}}", item.author));
        }
        taken.push(r.clone());
        edits.push((r, text));
    }
    for item in open().filter(|i| i.kind == Kind::Comment && fits(&i.range)) {
        let r = item.range.clone();
        let note = format!("{{>>{}: {}<<}}", item.author, item.record.text.as_deref().unwrap_or("").trim());
        if r.is_empty() || overlaps(&taken, &r) || current[r.clone()].contains("==}") {
            // at the end of the text it is on, after a mark it would be inside
            let at = taken.iter().find(|t| t.start < r.end && r.end < t.end).map_or(r.end, |t| t.end);
            edits.push((at..at, note));
        } else {
            taken.push(r.clone());
            edits.push((r.clone(), format!("{{=={}==}}{note}", &current[r])));
        }
    }
    // from the end, so the offsets stay good (nothing overlaps: the marks were checked)
    edits.sort_by_key(|(r, _)| std::cmp::Reverse((r.start, r.end)));
    let mut out = current.to_string();
    for (r, text) in edits {
        out.replace_range(r, &text);
    }
    (out, skipped)
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
        let (out, skipped) = markdown_with_changes(TEXT, &items);
        assert!(skipped.is_empty(), "{skipped:?}");
        assert_eq!(
            out,
            "{++Note: ++}{--The --}grounding line retreats {~~rapidly~>slowly~~} when the bed slopes.\nWe show this in {==Fig. 3==}{>>Jane Doe: a comment<<}."
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
        let (out, skipped) = markdown_with_changes(TEXT, &items);
        assert!(skipped.is_empty(), "{skipped:?}");
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
