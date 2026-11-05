//! The Markdown with the open suggestions and comments of a review as CriticMarkup (criticmarkup.com): `{++new++}`,
//! `{--old--}`, `{~~old~>new~~}`, and `{>>author: comment<<}` after the text it is on. For a reader without the app
//! (or Word); nothing reads it back.

use super::Status;
use super::fold::{Item, State};
use super::record::Kind;
use std::ops::Range;

/// `current` with the open suggestions and comments of `items` (the review folded onto `current`) as CriticMarkup,
/// and the ids of the suggestions left out (outdated, orphaned, or overlapping an earlier one).
pub fn markdown_with_changes(current: &str, items: &[Item]) -> (String, Vec<String>) {
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut skipped = Vec::new();
    let mut taken: Vec<Range<usize>> = Vec::new();
    for item in items.iter().filter(|i| i.state == State::Open) {
        let r = item.range.clone();
        let ok = r.start <= r.end && r.end <= current.len() && current.is_char_boundary(r.start) && current.is_char_boundary(r.end);
        match item.kind {
            Kind::Suggestion => {
                let overlaps = taken.iter().any(|t| r.start < t.end && t.start < r.end);
                if !ok || overlaps || !matches!(item.status, Status::Exact | Status::Moved) {
                    skipped.push(item.id.clone());
                    continue;
                }
                taken.push(r.clone());
                let (old, new) = (&current[r.clone()], item.replacement.as_deref().unwrap_or(""));
                let text = match (old.is_empty(), new.is_empty()) {
                    (true, true) => continue,
                    (true, false) => format!("{{++{new}++}}"),
                    (false, true) => format!("{{--{old}--}}"),
                    (false, false) => format!("{{~~{old}~>{new}~~}}"),
                };
                edits.push((r.clone(), text));
                if let Some(note) = item.record.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                    edits.push((r.end..r.end, format!("{{>>{}: {note}<<}}", item.author)));
                }
            }
            Kind::Comment if ok => {
                let body = item.record.text.as_deref().unwrap_or("").trim();
                edits.push((r.end..r.end, format!("{{>>{}: {body}<<}}", item.author)));
            }
            _ => {}
        }
    }
    // from the end, so the offsets stay good (nothing overlaps: the replacements were checked)
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
            "{++Note: ++}{--The --}grounding line retreats {~~rapidly~>slowly~~} when the bed slopes.\nWe show this in Fig. 3{>>Jane Doe: a comment<<}."
        );
    }
}
