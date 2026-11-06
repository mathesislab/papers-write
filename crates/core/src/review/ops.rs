//! Writing to a review (always by appending a record to the person's own file) and applying suggestions.

use super::fold::Item;
use super::record::{Decision, Kind, Person, Record};
use super::store::{self, Error, Result};
use super::text::{after, before, byte_of, slug, utf16_of};
use super::Review;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// The records written one at a time: the next id is read from the person's file and the record appended to it with
/// no other write in between (the app writes several at once: Replace All in suggest mode).
static WRITING: Mutex<()> = Mutex::new(());

fn one_at_a_time() -> MutexGuard<'static, ()> {
    WRITING.lock().unwrap_or_else(|e| e.into_inner())
}

/// What a person has just made: a suggestion or a comment on a range of the text it is made on. `from` and `to` are
/// UTF-16 code units of that text (the current text; for a counter-suggestion, the text with the first one applied).
#[derive(Clone, Debug)]
pub struct NewRecord {
    pub kind: Kind,
    pub from: usize,
    pub to: usize,
    /// a suggestion: what replaces the range (empty: a deletion; `from == to`: an insertion)
    pub insert: Option<String>,
    /// a comment: the body; a suggestion: its note
    pub text: Option<String>,
    pub round: Option<String>,
    /// when it was made, RFC 3339 (none: now); a change taken from git is dated by its commit
    pub time: Option<String>,
}

impl NewRecord {
    pub fn suggestion(from: usize, to: usize, insert: impl Into<String>) -> Self {
        Self { kind: Kind::Suggestion, from, to, insert: Some(insert.into()), text: None, round: None, time: None }
    }

    pub fn comment(from: usize, to: usize, text: impl Into<String>) -> Self {
        Self { kind: Kind::Comment, from, to, insert: None, text: Some(text.into()), round: None, time: None }
    }

    pub fn note(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn round(mut self, round: impl Into<String>) -> Self {
        self.round = Some(round.into());
        self
    }

    pub fn at(mut self, time: impl Into<String>) -> Self {
        self.time = Some(time.into());
        self
    }
}

/// The next id of `slug`'s file: `<slug>-<n>` with `n` above every one there.
fn next_id(doc: &Path, slug: &str) -> Result<String> {
    let path = store::person_path(doc, slug);
    let n = if path.exists() {
        let file = store::parse(&path)?;
        let through = file.person.map_or(0, |p| p.through);
        file.record.iter().filter_map(|r| Record::number(&r.id)).max().unwrap_or(0).max(through)
    } else {
        0
    };
    Ok(format!("{slug}-{}", n + 1))
}

/// A record of `new.kind` anchored on `text` (written as a base when new), with its quote.
fn anchored(doc: &Path, id: String, text: &str, new: &NewRecord) -> Result<Record> {
    if !matches!(new.kind, Kind::Suggestion | Kind::Comment) {
        return Err(Error::Invalid("only a suggestion or a comment is made on a range of the text".into()));
    }
    let len16 = utf16_of(text, text.len());
    if new.from > new.to || new.to > len16 {
        return Err(Error::Invalid(format!("the range {}..{} is not in the text ({len16} units)", new.from, new.to)));
    }
    let (start, end) = (byte_of(text, new.from), byte_of(text, new.to));
    let hash = store::write_base(doc, text)?;
    let mut r = Record::blank(id, new.kind);
    if let Some(time) = &new.time {
        r.time = time.clone();
    }
    r.round = new.round.clone();
    r.base = Some(hash);
    r.start = Some(start);
    r.end = Some(end);
    r.prefix = Some(before(text, start, 32).to_string());
    r.exact = Some(text[start..end].to_string());
    r.suffix = Some(after(text, end, 32).to_string());
    if new.kind == Kind::Suggestion {
        r.insert = Some(new.insert.clone().unwrap_or_default());
    }
    r.text = new.text.clone();
    Ok(r)
}

fn write(doc: &Path, me: &Person, record: Record) -> Result<Record> {
    store::append(&store::person_path(doc, &slug(&me.name)), me, &record)?;
    Ok(record)
}

impl Review {
    /// A new suggestion or comment of `me` on `current_text`: its base written if new, the record appended to
    /// `me`'s file.
    pub fn add(doc: &Path, me: &Person, current_text: &str, new: NewRecord) -> Result<Record> {
        let _turn = one_at_a_time();
        let id = next_id(doc, &slug(&me.name))?;
        let r = anchored(doc, id, current_text, &new)?;
        write(doc, me, r)
    }

    /// A reply of `me` to the record `on`.
    pub fn reply(doc: &Path, me: &Person, on: &str, text: &str) -> Result<Record> {
        let _turn = one_at_a_time();
        Self::exists(doc, on)?;
        let mut r = Record::blank(next_id(doc, &slug(&me.name))?, Kind::Reply);
        r.on = Some(on.to_string());
        r.text = Some(text.to_string());
        write(doc, me, r)
    }

    /// A decision of `me` on the suggestion or comment `on`: accepted, rejected, resolved, reopened or withdrawn.
    /// (Accepting also needs the new text written: that is the caller's.)
    pub fn decide(doc: &Path, me: &Person, on: &str, decision: Decision) -> Result<Record> {
        let _turn = one_at_a_time();
        Self::exists(doc, on)?;
        let mut r = Record::blank(next_id(doc, &slug(&me.name))?, Kind::Decision);
        r.on = Some(on.to_string());
        r.decision = Some(decision);
        write(doc, me, r)
    }

    /// `me` edits their own suggestion or comment `old_id`: a new record, anchored on `current_text`, that
    /// supersedes it. It keeps the kind, the thread it is in (`on`) and, unless `new` gives one, the round.
    pub fn supersede(doc: &Path, me: &Person, current_text: &str, old_id: &str, new: NewRecord) -> Result<Record> {
        let _turn = one_at_a_time();
        let slug = slug(&me.name);
        let path = store::person_path(doc, &slug);
        let old = if path.exists() { store::parse(&path)?.record.into_iter().find(|r| r.id == old_id) } else { None };
        let old = old.ok_or_else(|| Error::Invalid(format!("{old_id} is not a record of {}", me.name)))?;
        if old.kind != new.kind {
            return Err(Error::Invalid(format!("{old_id} is a {:?}, not a {:?}", old.kind, new.kind)));
        }
        let mut r = anchored(doc, next_id(doc, &slug)?, current_text, &new)?;
        r.on = old.on;
        r.round = new.round.or(old.round);
        r.supersedes = Some(old_id.to_string());
        write(doc, me, r)
    }

    /// A counter-suggestion of `me` to the suggestion `on_suggestion`: made on the text as that suggestion would
    /// leave it (`apply` of it onto `current_text`), which is written as its base. `new.from` and `new.to` are
    /// positions in that text, not in `current_text`.
    pub fn counter(doc: &Path, me: &Person, current_text: &str, on_suggestion: &str, new: NewRecord) -> Result<Record> {
        let _turn = one_at_a_time();
        if new.kind != Kind::Suggestion {
            return Err(Error::Invalid("a counter-suggestion is a suggestion".into()));
        }
        let items = Review::load(doc)?.items(current_text);
        let item = items
            .iter()
            .find(|i| i.id == on_suggestion && i.kind == Kind::Suggestion)
            .ok_or_else(|| Error::NotFound(on_suggestion.to_string()))?;
        let applied = apply(current_text, item);
        let mut r = anchored(doc, next_id(doc, &slug(&me.name))?, &applied, &new)?;
        r.on = Some(item.id.clone());
        write(doc, me, r)
    }

    fn exists(doc: &Path, id: &str) -> Result<()> {
        let review = Review::load(doc)?;
        if review.records().any(|r| r.id == id && matches!(r.kind, Kind::Suggestion | Kind::Comment | Kind::Reply)) {
            Ok(())
        } else {
            Err(Error::NotFound(id.to_string()))
        }
    }
}

fn applicable(text: &str, item: &Item) -> bool {
    item.kind == Kind::Suggestion
        && item.replacement.is_some()
        && item.range.start <= item.range.end
        && item.range.end <= text.len()
        && text.is_char_boundary(item.range.start)
        && text.is_char_boundary(item.range.end)
}

/// `current_text` with the suggestion `item` applied at its carried range (what Accept writes, and the base of a
/// counter-suggestion). Whatever its status: an outdated one replaces the range it is shown on. A comment changes
/// nothing. `current_text` must be the text the item was folded onto.
pub fn apply(current_text: &str, item: &Item) -> String {
    apply_all(current_text, std::slice::from_ref(item))
}

/// Several suggestions applied, from the end of the text backwards so the ranges stay good. One that overlaps a
/// later one already applied is left out (two suggestions on the same text: accepting one leaves the other).
pub fn apply_all(current_text: &str, items: &[Item]) -> String {
    let mut chosen: Vec<&Item> = items.iter().filter(|i| applicable(current_text, i)).collect();
    chosen.sort_by_key(|i| std::cmp::Reverse((i.range.start, i.range.end)));
    let mut out = current_text.to_string();
    let mut limit = usize::MAX;
    for i in chosen {
        if i.range.end > limit {
            continue;
        }
        out.replace_range(i.range.clone(), i.replacement.as_deref().unwrap_or(""));
        limit = i.range.start;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::fold::State;
    use crate::review::testutil::doc;
    use crate::review::Status;

    const TEXT: &str = "The grounding line retreats rapidly when the bed slopes inland.\nWe show this in Fig. 3.";

    fn jane() -> Person {
        Person::new("Jane Doe", "jane@example.org")
    }

    fn alex() -> Person {
        Person::new("Alex Robinson", "alex@example.org")
    }

    fn range_of(text: &str, needle: &str) -> (usize, usize) {
        let b = text.find(needle).unwrap();
        (utf16_of(text, b), utf16_of(text, b + needle.len()))
    }

    #[test]
    fn suggestions_made_at_once_have_ids_of_their_own() {
        let d = doc("paper.md");
        let words = ["grounding", "line", "retreats", "rapidly", "bed", "slopes", "inland", "show"];
        let ids: Vec<String> = std::thread::scope(|s| {
            let made: Vec<_> = words
                .iter()
                .map(|w| {
                    let d = &d;
                    s.spawn(move || {
                        let (f, t) = range_of(TEXT, w);
                        Review::add(d, &jane(), TEXT, NewRecord::suggestion(f, t, "")).unwrap().id
                    })
                })
                .collect();
            made.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), words.len(), "{ids:?}");
    }

    #[test]
    fn add_writes_base_and_appends_with_increasing_ids() {
        let d = doc("paper.md");
        let (f, t) = range_of(TEXT, "rapidly");
        let r1 = Review::add(&d, &jane(), TEXT, NewRecord::suggestion(f, t, "irreversibly").note("stronger").round("second draft")).unwrap();
        let (f, t) = range_of(TEXT, "Fig. 3");
        let r2 = Review::add(&d, &jane(), TEXT, NewRecord::comment(f, t, "Which figure?")).unwrap();
        assert_eq!((r1.id.as_str(), r2.id.as_str()), ("jane-doe-1", "jane-doe-2"));
        assert_eq!(r1.exact.as_deref(), Some("rapidly"));
        assert_eq!(r1.prefix.as_deref(), Some("The grounding line retreats "));
        assert_eq!(r1.suffix.as_deref(), Some(" when the bed slopes inland.\nWe "));
        assert_eq!(r1.base, r2.base);
        assert_eq!(store::read_base(&d, r1.base.as_ref().unwrap()).as_deref(), Some(TEXT));
        let review = Review::load(&d).unwrap();
        assert_eq!(review.files[0].person.as_ref().unwrap().email, "jane@example.org");
        let items = review.items(TEXT);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].record.round.as_deref(), Some("second draft"));
        assert_eq!(items[0].record.text.as_deref(), Some("stronger"));
        assert!(items.iter().all(|i| i.status == Status::Exact && i.state == State::Open));
        // another person starts at 1 in their own file
        let r3 = Review::add(&d, &alex(), TEXT, NewRecord::comment(0, 3, "x")).unwrap();
        assert_eq!(r3.id, "alex-robinson-1");
    }

    #[test]
    fn add_checks_the_range() {
        let d = doc("paper.md");
        assert!(Review::add(&d, &jane(), "abc", NewRecord::comment(2, 9, "x")).is_err());
        assert!(Review::add(&d, &jane(), "abc", NewRecord::comment(2, 1, "x")).is_err());
        assert!(!store::review_dir(&d).exists() || Review::load(&d).unwrap().records().count() == 0);
    }

    #[test]
    fn utf16_positions_with_accents_emoji_and_crlf() {
        let d = doc("paper.md");
        let text = "Café 😀 très\r\nretreats rapidly\r\nend";
        let (f, t) = range_of(text, "rapidly");
        assert!(text.encode_utf16().count() < text.len());
        let r = Review::add(&d, &jane(), text, NewRecord::suggestion(f, t, "fast")).unwrap();
        assert_eq!(r.exact.as_deref(), Some("rapidly"));
        assert_eq!(r.start, text.find("rapidly"));
        let items = Review::load(&d).unwrap().items(text);
        assert_eq!((items[0].at.from, items[0].at.to), (f, t));
        assert_eq!(apply(text, &items[0]), text.replace("rapidly", "fast"));
        // an emoji selected whole
        let (f, t) = range_of(text, "😀");
        assert_eq!(t - f, 2);
        let r = Review::add(&d, &jane(), text, NewRecord::comment(f, t, "x")).unwrap();
        assert_eq!(r.exact.as_deref(), Some("😀"));
    }

    #[test]
    fn reply_decide_and_unknown_targets() {
        let d = doc("paper.md");
        let (f, t) = range_of(TEXT, "rapidly");
        let s = Review::add(&d, &jane(), TEXT, NewRecord::suggestion(f, t, "fast")).unwrap();
        Review::reply(&d, &alex(), &s.id, "why?").unwrap();
        Review::decide(&d, &alex(), &s.id, Decision::Rejected).unwrap();
        let items = Review::load(&d).unwrap().items(TEXT);
        assert_eq!(items[0].state, State::Rejected);
        assert_eq!(items[0].thread.len(), 1);
        assert!(matches!(Review::reply(&d, &alex(), "nope-1", "x"), Err(Error::NotFound(_))));
        assert!(matches!(Review::decide(&d, &alex(), "nope-1", Decision::Accepted), Err(Error::NotFound(_))));
    }

    #[test]
    fn supersede_replaces_and_only_by_the_author() {
        let d = doc("paper.md");
        let (f, t) = range_of(TEXT, "rapidly");
        let s = Review::add(&d, &jane(), TEXT, NewRecord::suggestion(f, t, "fast").round("r1")).unwrap();
        Review::reply(&d, &alex(), &s.id, "ok").unwrap();
        let s2 = Review::supersede(&d, &jane(), TEXT, &s.id, NewRecord::suggestion(f, t, "irreversibly")).unwrap();
        assert_eq!(s2.supersedes.as_deref(), Some("jane-doe-1"));
        assert_eq!(s2.round.as_deref(), Some("r1"));
        let items = Review::load(&d).unwrap().items(TEXT);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].record.insert.as_deref(), Some("irreversibly"));
        assert_eq!(items[0].thread.len(), 1);
        assert!(Review::supersede(&d, &alex(), TEXT, &s.id, NewRecord::suggestion(f, t, "x")).is_err());
        assert!(Review::supersede(&d, &jane(), TEXT, &s.id, NewRecord::comment(f, t, "x")).is_err());
    }

    #[test]
    fn counter_suggestion_is_made_on_the_text_with_the_first_applied() {
        let d = doc("paper.md");
        let (f, t) = range_of(TEXT, "rapidly");
        let s = Review::add(&d, &jane(), TEXT, NewRecord::suggestion(f, t, "irreversibly")).unwrap();
        let applied = TEXT.replace("rapidly", "irreversibly");
        let (f2, t2) = range_of(&applied, "irreversibly");
        let c = Review::counter(&d, &alex(), TEXT, &s.id, NewRecord::suggestion(f2, t2, "slowly")).unwrap();
        assert_eq!(c.on.as_deref(), Some(s.id.as_str()));
        assert_eq!(c.exact.as_deref(), Some("irreversibly"));
        assert_eq!(store::read_base(&d, c.base.as_ref().unwrap()).as_deref(), Some(applied.as_str()));
        let items = Review::load(&d).unwrap().items(TEXT);
        let first = items.iter().find(|i| i.id == s.id).unwrap();
        assert_eq!(first.thread.len(), 1);
        // the counter, carried onto the current text (where the first is not applied): its text is not there
        let counter = items.iter().find(|i| i.id == c.id).unwrap();
        assert_ne!(counter.status, Status::Exact);
        // once the first is accepted the counter sits exactly on the new text
        let now = apply(TEXT, first);
        let items = Review::load(&d).unwrap().items(&now);
        let counter = items.iter().find(|i| i.id == c.id).unwrap();
        assert_eq!(counter.status, Status::Exact);
        assert_eq!(apply(&now, counter), TEXT.replace("rapidly", "slowly"));
        assert!(matches!(Review::counter(&d, &alex(), TEXT, "nope", NewRecord::suggestion(0, 1, "x")), Err(Error::NotFound(_))));
    }

    #[test]
    fn apply_and_apply_all() {
        let d = doc("paper.md");
        let (f, t) = range_of(TEXT, "rapidly");
        Review::add(&d, &jane(), TEXT, NewRecord::suggestion(f, t, "fast")).unwrap();
        let (f, t) = range_of(TEXT, "The ");
        Review::add(&d, &jane(), TEXT, NewRecord::suggestion(f, t, "")).unwrap();
        let (f, _) = range_of(TEXT, "inland");
        Review::add(&d, &alex(), TEXT, NewRecord::suggestion(f, f, "far ")).unwrap();
        let (f, t) = range_of(TEXT, "Fig. 3");
        Review::add(&d, &alex(), TEXT, NewRecord::comment(f, t, "a comment")).unwrap();
        // two on the same text: only one applies
        let (f, t) = range_of(TEXT, "rapidly when");
        Review::add(&d, &alex(), TEXT, NewRecord::suggestion(f, t, "when")).unwrap();
        let items = Review::load(&d).unwrap().items(TEXT);
        let all = apply_all(TEXT, &items);
        assert!(all.starts_with("grounding line retreats "), "{all}");
        assert!(all.contains("far inland"), "{all}");
        assert!(all.ends_with("Fig. 3."));
        assert!(all.contains("retreats when the bed") != all.contains("retreats fast when the bed"), "exactly one of the two: {all}");
        // one alone
        let one = items.iter().find(|i| i.record.insert.as_deref() == Some("far ")).unwrap();
        assert_eq!(apply(TEXT, one), TEXT.replace("inland", "far inland"));
        let comment = items.iter().find(|i| i.kind == Kind::Comment).unwrap();
        assert_eq!(apply(TEXT, comment), TEXT);
    }

    #[test]
    fn suggestion_survives_the_author_working_on() {
        let d = doc("paper.md");
        let (f, t) = range_of(TEXT, "rapidly");
        Review::add(&d, &jane(), TEXT, NewRecord::suggestion(f, t, "irreversibly")).unwrap();
        // the author rewrites the opening and the next sentence meanwhile
        let now = TEXT.replace("The grounding line", "In our model the grounding line").replace("We show", "Figure 3 shows");
        let items = Review::load(&d).unwrap().items(&now);
        assert_eq!(items[0].status, Status::Moved);
        assert_eq!(apply(&now, &items[0]), now.replace("rapidly", "irreversibly"));
    }

    #[test]
    fn concurrent_branches_never_conflict() {
        // two clones; each person appends to their own file; the "merge" is the union of the files
        let main = doc("paper.md");
        let a = doc("paper.md");
        let b = doc("paper.md");
        let (f, t) = range_of(TEXT, "rapidly");
        Review::add(&a, &jane(), TEXT, NewRecord::suggestion(f, t, "fast")).unwrap();
        Review::add(&a, &jane(), TEXT, NewRecord::comment(0, 3, "first words")).unwrap();
        Review::add(&b, &alex(), TEXT, NewRecord::comment(f, t, "agree")).unwrap();
        for src in [&a, &b] {
            let from = store::review_dir(src);
            let to = store::review_dir(&main);
            std::fs::create_dir_all(to.join("bases")).unwrap();
            for e in std::fs::read_dir(&from).unwrap().flatten() {
                if e.path().is_file() {
                    std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
                }
            }
            for e in std::fs::read_dir(from.join("bases")).unwrap().flatten() {
                std::fs::copy(e.path(), to.join("bases").join(e.file_name())).unwrap(); // the same file: same name, same bytes
            }
        }
        let review = Review::load(&main).unwrap();
        assert!(review.errors.is_empty());
        let items = review.items(TEXT);
        let mut ids: Vec<_> = items.iter().map(|i| i.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["alex-robinson-1", "jane-doe-1", "jane-doe-2"]);
        // and the same person on both sides of a merge would be one file appended twice (not our case: one app each)
    }

    #[test]
    fn the_base_lost_the_quote_is_searched() {
        let d = doc("paper.md");
        let (f, t) = range_of(TEXT, "rapidly");
        let r = Review::add(&d, &jane(), TEXT, NewRecord::suggestion(f, t, "fast")).unwrap();
        std::fs::remove_file(store::base_path(&d, r.base.as_ref().unwrap())).unwrap();
        let now = format!("Preface.\n\n{TEXT}");
        let items = Review::load(&d).unwrap().items(&now);
        assert_eq!(items[0].status, Status::Moved);
        assert_eq!(apply(&now, &items[0]), now.replace("rapidly", "fast"));
    }

    #[test]
    fn from_git_reads_the_configuration() {
        let d = doc("paper.md");
        let dir = d.parent().unwrap();
        let ok = std::process::Command::new("git").arg("-C").arg(dir).arg("init").arg("-q").status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            return;
        }
        for (k, v) in [("user.name", "Test Person"), ("user.email", "t@example.org")] {
            std::process::Command::new("git").arg("-C").arg(dir).args(["config", k, v]).status().unwrap();
        }
        assert_eq!(Person::from_git(dir), Some(Person::new("Test Person", "t@example.org")));
        assert_eq!(Person::from_git(Path::new("/nonexistent-folder-xyz")), None);
    }
}
