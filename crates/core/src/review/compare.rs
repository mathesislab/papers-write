//! An edited copy, a commit or a branch as suggestions: the word-level difference from the current text to the
//! edited one, a suggestion per changed stretch. Whoever made the edited text is not asked to use the app.

use super::anchor::{Anchor, Status};
use super::byte_of;
use super::diff::hunks;
use super::fold::{Item, State};
use super::ops::NewRecord;
use super::record::{Kind, Record};
use super::text::utf16_of;

/// The suggestions that turn `current` into `edited`, in order, with positions in UTF-16 code units of `current`. A
/// stretch changed is one suggestion (changes with only white space between them are one); applied all, they give
/// `edited`.
pub fn suggestions(current: &str, edited: &str) -> Vec<NewRecord> {
    hunks(current, edited)
        .into_iter()
        .map(|h| NewRecord::suggestion(utf16_of(current, h.old.start), utf16_of(current, h.old.end), &edited[h.new]))
        .collect()
}

/// `list` (the suggestions that turn `text` into another text, as `suggestions` gives them) as open items on `text`,
/// to draw as tracked changes without a review on disk: ids `history-1`, `history-2`, ..., all by `author`.
pub fn as_items(text: &str, list: &[NewRecord], author: &str) -> Vec<Item> {
    list.iter()
        .enumerate()
        .map(|(n, s)| {
            let range = byte_of(text, s.from)..byte_of(text, s.to);
            let id = format!("history-{}", n + 1);
            let insert = s.insert.clone().unwrap_or_default();
            let mut record = Record::blank(id.clone(), Kind::Suggestion);
            record.start = Some(range.start);
            record.end = Some(range.end);
            record.exact = Some(text[range.clone()].to_string());
            record.insert = Some(insert.clone());
            Item {
                id,
                kind: Kind::Suggestion,
                author: author.to_string(),
                on: None,
                record,
                thread: Vec::new(),
                state: State::Open,
                at: Anchor { from: s.from, to: s.to },
                status: Status::Exact,
                replacement: Some(insert),
                range,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(current: &str, list: &[NewRecord]) -> String {
        let mut out = current.to_string();
        for s in list.iter().rev() {
            let (a, b) = (super::super::byte_of(current, s.from), super::super::byte_of(current, s.to));
            out.replace_range(a..b, s.insert.as_deref().unwrap_or(""));
        }
        out
    }

    #[test]
    fn nothing_when_equal() {
        assert!(suggestions("same", "same").is_empty());
    }

    #[test]
    fn an_insertion() {
        let (a, b) = ("The line retreats.", "The line retreats quickly.");
        let s = suggestions(a, b);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].from, s[0].to);
        assert!(s[0].insert.as_deref().unwrap().contains("quickly"));
        assert_eq!(apply(a, &s), b);
    }

    #[test]
    fn a_deletion() {
        let (a, b) = ("The very large line retreats.", "The large line retreats.");
        let s = suggestions(a, b);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].insert.as_deref(), Some(""));
        assert_eq!(apply(a, &s), b);
    }

    #[test]
    fn a_replacement() {
        let (a, b) = ("The line retreats rapidly now.", "The line retreats slowly now.");
        let s = suggestions(a, b);
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].from, s[0].to), (18, 25));
        assert_eq!(s[0].insert.as_deref(), Some("slowly"));
    }

    #[test]
    fn changes_across_a_space_are_one_and_far_ones_are_two() {
        let (a, b) = ("we saw a big drop here and there", "we saw the huge drop here and elsewhere");
        let s = suggestions(a, b);
        assert_eq!(s.len(), 2, "{s:?}");
        assert_eq!(s[0].insert.as_deref(), Some("the huge"));
        assert_eq!(apply(a, &s), b);
    }

    #[test]
    fn positions_count_utf16_units() {
        let (a, b) = ("𝒳 and é grows", "𝒳 and é shrinks");
        let s = suggestions(a, b);
        assert_eq!(s.len(), 1);
        // 𝒳 is two units
        assert_eq!((s[0].from, s[0].to), (9, 14));
        assert_eq!(apply(a, &s), b);
    }

    #[test]
    fn items_of_the_suggestions() {
        let (a, b) = ("The line retreats rapidly now.", "The line retreats slowly now.");
        let items = as_items(a, &suggestions(a, b), "since v2");
        assert_eq!(items.len(), 1);
        let i = &items[0];
        assert_eq!((i.id.as_str(), i.author.as_str(), i.state), ("history-1", "since v2", State::Open));
        assert_eq!((i.at.from, i.at.to), (18, 25));
        assert_eq!(i.record.exact.as_deref(), Some("rapidly"));
        assert_eq!(i.replacement.as_deref(), Some("slowly"));
        assert_eq!(&a[i.range.clone()], "rapidly");
    }
}
