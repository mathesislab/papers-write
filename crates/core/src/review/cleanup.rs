//! Clean Up Review: the review of a document shorn of what is done with.
//!
//! The rule, which keeps "a person's file is written only by that person" and "git merges the files without
//! conflict":
//!
//! - `me` rewrites only their own file, and removes from it only
//!   - the records of items that are resolved (accepted, rejected, resolved or withdrawn): their suggestions and
//!     comments (an edited one with all its older records), their replies in the thread of such an item,
//!     unless an open counter-suggestion still hangs on the item;
//!   - their decisions and replies that name a record no file has any more.
//!
//!   A decision of theirs on an item that is still in someone else's file stays: without it the item would be open
//!   again. It goes when the item's owner has cleaned up, and the next clean-up of its writer finds it naming
//!   nothing.
//! - Nothing in anybody else's file is touched: they clean up their own. The fold already leaves decided items out,
//!   so the review looks the same before and after, for everybody.
//! - A base (`bases/<hash>`) that no record of any file names, and that is not the text of the document now, is
//!   deleted: bases belong to nobody, and a base nothing needs is only weight. The document's own text is kept so
//!   that a suggestion made at this moment, in another clone, still finds its base after the merge.
//! - With a review file that cannot be read nothing is done: what it names is not known.
//! - Ids removed are not given again: the person's header says through which number (`through`), so a decision of
//!   someone else on a removed suggestion never lands on a new one.
//! - Git keeps everything that was in the files and bases, in its history: it is a matter of what is carried, not
//!   of what is lost. A `.review` package made earlier can bring removed records back when it
//!   is merged: they are decided ones, which show as they did.

use super::Review;
use super::fold::State;
use super::record::{Kind, Person, Record};
use super::store::{self, Error, Result};
use super::text::{blob_hash, slug};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// What a clean-up removes (`cleanup_plan`: what it would), or removed (`cleanup`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Cleanup {
    /// the ids of the records of the user's file taken out of it
    pub records: Vec<String>,
    /// the hashes of the bases nothing needs
    pub bases: Vec<String>,
    /// the resolved items of other people, left in their files until they clean up
    pub others: usize,
}

impl Cleanup {
    /// Nothing to remove.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty() && self.bases.is_empty()
    }
}

fn slug_of(id: &str) -> &str {
    id.rsplit_once('-').map_or(id, |(s, _)| s)
}

/// What a clean-up by `me` would remove, and nothing is written.
pub fn cleanup_plan(doc: &Path, me: &Person) -> Result<Cleanup> {
    plan(doc, me).map(|p| p.cleanup)
}

/// The clean-up by `me` of the review of the document at `doc`: their own file rewritten without what is resolved
/// (atomically; the one time a file is not appended to), the bases nothing needs deleted. What was removed.
pub fn cleanup(doc: &Path, me: &Person) -> Result<Cleanup> {
    let p = plan(doc, me)?;
    if !p.cleanup.records.is_empty() {
        let path = store::person_path(doc, &slug(&me.name));
        let mut person = p.person.clone();
        let gone = p.cleanup.records.iter().filter_map(|id| Record::number(id)).max().unwrap_or(0);
        person.through = person.through.max(gone);
        let kept: Vec<&Record> = p.mine.iter().filter(|r| !p.cleanup.records.contains(&r.id)).collect();
        store::rewrite(&path, &person, &kept)?;
    }
    for hash in &p.cleanup.bases {
        let path = store::base_path(doc, hash);
        fs::remove_file(&path).map_err(|e| Error::Io(path, e))?;
    }
    Ok(p.cleanup)
}

struct Plan {
    cleanup: Cleanup,
    person: Person,
    mine: Vec<Record>,
}

fn plan(doc: &Path, me: &Person) -> Result<Plan> {
    let review = Review::load(doc)?;
    if let Some(e) = review.errors.first() {
        return Err(Error::Invalid(format!("not cleaned up: {} cannot be read ({})", e.file.display(), e.message)));
    }
    let mine_slug = slug(&me.name);
    let own = review.files.iter().find(|f| f.slug == mine_slug);
    let mine: Vec<Record> = own.map(|f| f.records.clone()).unwrap_or_default();
    let person = own.and_then(|f| f.person.clone()).unwrap_or_else(|| me.clone());

    // the states do not depend on the text: the document as it is, or none
    let text = fs::read_to_string(doc).unwrap_or_default();
    let items = review.items(&text);
    let state_of: HashMap<&str, State> = items.iter().map(|i| (i.id.as_str(), i.state)).collect();
    // suggestions that an open counter-suggestion answers
    let held: HashSet<&str> = items.iter().filter(|i| i.state == State::Open).filter_map(|i| i.on.as_deref()).collect();
    let by_id: HashMap<&str, &Record> = review.records().map(|r| (r.id.as_str(), r)).collect();
    let newer: HashMap<&str, &str> = review.records().filter_map(|r| Some((r.supersedes.as_deref()?, r.id.as_str()))).collect();
    let canon = |id: &str| -> String {
        let mut id = id;
        for _ in 0..=newer.len() {
            match newer.get(id) {
                Some(n) => id = n,
                None => break,
            }
        }
        id.to_string()
    };
    // the suggestion or comment a record belongs to (a reply: the one at the root of its replies)
    let item_of = |r: &Record| -> Option<String> {
        match r.kind {
            Kind::Suggestion | Kind::Comment => Some(canon(&r.id)),
            Kind::Reply => {
                let mut on = r.on.as_deref()?;
                for _ in 0..by_id.len() {
                    match by_id.get(on) {
                        Some(t) if t.kind == Kind::Reply => on = t.on.as_deref()?,
                        _ => break,
                    }
                }
                Some(canon(on))
            }
            Kind::Decision => None,
        }
    };
    let resolved = |item: &str| state_of.get(item).is_some_and(|s| *s != State::Open) && !held.contains(item);

    let mut gone: HashSet<&str> = mine.iter().filter(|r| item_of(r).is_some_and(|i| resolved(&i))).map(|r| r.id.as_str()).collect();
    // what names a record no file has any more
    let present = |id: &str, gone: &HashSet<&str>| by_id.contains_key(id) && !gone.contains(id);
    let dangling: Vec<&str> = mine
        .iter()
        .filter(|r| matches!(r.kind, Kind::Decision | Kind::Reply) && !gone.contains(r.id.as_str()))
        .filter(|r| r.on.as_deref().is_some_and(|on| !present(on, &gone)))
        .map(|r| r.id.as_str())
        .collect();
    gone.extend(dangling);

    let records: Vec<String> = mine.iter().filter(|r| gone.contains(r.id.as_str())).map(|r| r.id.clone()).collect();

    // the bases some record still names, and the one of the text now
    let mut needed: HashSet<String> = review
        .records()
        .filter(|r| !gone.contains(r.id.as_str()))
        .filter_map(|r| r.base.clone())
        .collect();
    needed.insert(blob_hash(&text));
    let mut bases: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(store::review_dir(doc).join("bases")) {
        for entry in entries.flatten() {
            let path = entry.path();
            let hash = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            if path.is_file() && !needed.contains(&hash) {
                bases.push(hash);
            }
        }
    }
    bases.sort();

    let others = items.iter().filter(|i| i.state != State::Open && slug_of(&i.id) != mine_slug).count();
    Ok(Plan { cleanup: Cleanup { records, bases, others }, person, mine })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::fold::State;
    use crate::review::ops::NewRecord;
    use crate::review::record::Decision;
    use crate::review::testutil::doc;

    const TEXT: &str = "The grounding line retreats rapidly when the bed slopes inland. We show this in Fig. 3.";

    fn jane() -> Person {
        Person::new("Jane Doe", "jane@example.org")
    }

    fn alex() -> Person {
        Person::new("Alex Robinson", "alex@example.org")
    }

    fn range(text: &str, needle: &str) -> (usize, usize) {
        let b = text.find(needle).unwrap();
        (b, b + needle.len())
    }

    /// A document on disk with `TEXT`.
    fn setup() -> std::path::PathBuf {
        let d = doc("paper.md");
        fs::write(&d, TEXT).unwrap();
        d
    }

    fn suggest(d: &Path, who: &Person, text: &str, needle: &str, insert: &str) -> String {
        let (f, t) = range(text, needle);
        Review::add(d, who, text, NewRecord::suggestion(f, t, insert)).unwrap().id
    }

    fn ids(d: &Path, who: &str) -> Vec<String> {
        let r = Review::load(d).unwrap();
        r.files.iter().find(|f| f.slug == who).map(|f| f.records.iter().map(|r| r.id.clone()).collect()).unwrap_or_default()
    }

    fn base_files(d: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(store::review_dir(d).join("bases")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn an_own_resolved_item_goes_with_its_replies_and_decisions() {
        let d = setup();
        let s1 = suggest(&d, &jane(), TEXT, "rapidly", "fast");
        let s2 = suggest(&d, &jane(), TEXT, "Fig. 3", "Fig. 4");
        Review::reply(&d, &jane(), &s1, "because").unwrap();
        Review::decide(&d, &jane(), &s1, Decision::Withdrawn).unwrap();
        let plan = cleanup_plan(&d, &jane()).unwrap();
        assert_eq!(plan.records, ["jane-doe-1", "jane-doe-3", "jane-doe-4"]);
        assert_eq!(ids(&d, "jane-doe").len(), 4, "a plan writes nothing");
        let done = cleanup(&d, &jane()).unwrap();
        assert_eq!(done, plan);
        assert_eq!(ids(&d, "jane-doe"), std::slice::from_ref(&s2));
        // the open one is as it was
        let items = Review::load(&d).unwrap().items(TEXT);
        assert_eq!(items.len(), 1);
        assert_eq!((items[0].id.as_str(), items[0].state), (s2.as_str(), State::Open));
        // a second time: nothing
        assert!(cleanup(&d, &jane()).unwrap().is_empty());
    }

    #[test]
    fn ids_removed_are_not_given_again() {
        let d = setup();
        let s = suggest(&d, &jane(), TEXT, "rapidly", "fast");
        Review::decide(&d, &jane(), &s, Decision::Rejected).unwrap();
        cleanup(&d, &jane()).unwrap();
        assert!(ids(&d, "jane-doe").is_empty());
        let again = suggest(&d, &jane(), TEXT, "rapidly", "slowly");
        assert_eq!(again, "jane-doe-3");
        let text = fs::read_to_string(store::person_path(&d, "jane-doe")).unwrap();
        assert!(text.contains("through = 2"), "{text}");
        assert!(text.contains("name = \"Jane Doe\""));
    }

    #[test]
    fn another_persons_file_is_never_touched_and_a_decision_on_an_item_still_there_stays() {
        let d = setup();
        let s = suggest(&d, &jane(), TEXT, "rapidly", "fast");
        Review::decide(&d, &alex(), &s, Decision::Accepted).unwrap();
        let (jane_before, alex_before) = (fs::read(store::person_path(&d, "jane-doe")).unwrap(), fs::read(store::person_path(&d, "alex-robinson")).unwrap());
        // Alex: his only record is the decision on Jane's suggestion, which is still in her file
        let p = cleanup_plan(&d, &alex()).unwrap();
        assert!(p.records.is_empty());
        assert_eq!(p.others, 1);
        cleanup(&d, &alex()).unwrap();
        assert_eq!(fs::read(store::person_path(&d, "alex-robinson")).unwrap(), alex_before);
        assert_eq!(fs::read(store::person_path(&d, "jane-doe")).unwrap(), jane_before);
        // Jane cleans: her suggestion goes, Alex's decision is left naming nothing (hers is not touched by his)
        assert_eq!(cleanup(&d, &jane()).unwrap().records, std::slice::from_ref(&s));
        assert_eq!(fs::read(store::person_path(&d, "alex-robinson")).unwrap(), alex_before);
        // and now Alex's goes too
        assert_eq!(cleanup(&d, &alex()).unwrap().records, ["alex-robinson-1"]);
        assert!(Review::load(&d).unwrap().items(TEXT).is_empty());
    }

    #[test]
    fn a_suggestion_with_an_open_counter_suggestion_stays() {
        let d = setup();
        let s = suggest(&d, &jane(), TEXT, "rapidly", "fast");
        let c = Review::counter(&d, &alex(), TEXT, &s, NewRecord::suggestion(0, 4, "slowly")).unwrap();
        Review::decide(&d, &jane(), &s, Decision::Rejected).unwrap();
        // the counter-suggestion is still open (rejecting through the app rejects it too; here only the first was)
        let items = Review::load(&d).unwrap().items(TEXT);
        assert_eq!(items.iter().find(|i| i.id == c.id).unwrap().state, State::Open);
        assert!(cleanup(&d, &jane()).unwrap().records.is_empty());
        // decided too: both go
        Review::decide(&d, &alex(), &c.id, Decision::Rejected).unwrap();
        assert_eq!(cleanup(&d, &jane()).unwrap().records, [s, "jane-doe-2".to_string()]);
        assert_eq!(cleanup(&d, &alex()).unwrap().records.len(), 2, "the counter-suggestion and its decision");
    }

    #[test]
    fn an_edited_suggestion_goes_with_all_its_records() {
        let d = setup();
        let s = suggest(&d, &jane(), TEXT, "rapidly", "fast");
        let (f, t) = range(TEXT, "rapidly");
        let newer = Review::supersede(&d, &jane(), TEXT, &s, NewRecord::suggestion(f, t, "quickly")).unwrap();
        Review::decide(&d, &jane(), &newer.id, Decision::Accepted).unwrap();
        assert_eq!(cleanup(&d, &jane()).unwrap().records.len(), 3);
        assert!(ids(&d, "jane-doe").is_empty());
    }

    #[test]
    fn a_comment_resolved_and_one_open() {
        let d = setup();
        let (f, t) = range(TEXT, "Fig. 3");
        let c1 = Review::add(&d, &jane(), TEXT, NewRecord::comment(f, t, "which?")).unwrap().id;
        let c2 = Review::add(&d, &jane(), TEXT, NewRecord::comment(0, 3, "the?")).unwrap().id;
        Review::reply(&d, &alex(), &c1, "this one").unwrap();
        Review::decide(&d, &jane(), &c1, Decision::Resolved).unwrap();
        // Alex's reply is in a resolved thread: his to clear
        assert_eq!(cleanup(&d, &alex()).unwrap().records, ["alex-robinson-1"]);
        assert_eq!(cleanup(&d, &jane()).unwrap().records, [c1, "jane-doe-3".to_string()]);
        assert_eq!(ids(&d, "jane-doe"), [c2]);
    }

    #[test]
    fn bases_that_nothing_needs_go_the_one_of_the_text_stays() {
        let d = setup();
        let s = suggest(&d, &jane(), TEXT, "rapidly", "fast");
        let edited = TEXT.replace("rapidly", "fast");
        let kept = suggest(&d, &alex(), &edited, "fast", "quick");
        Review::decide(&d, &jane(), &s, Decision::Accepted).unwrap();
        assert_eq!(base_files(&d).len(), 2);
        fs::write(&d, &edited).unwrap();
        let done = cleanup(&d, &jane()).unwrap();
        assert_eq!(done.bases, [blob_hash(TEXT)]);
        assert_eq!(base_files(&d), [format!("{}.md", blob_hash(&edited))]);
        // Alex's suggestion still finds its base
        let items = Review::load(&d).unwrap().items(&edited);
        assert_eq!((items[0].id.as_str(), items[0].state), (kept.as_str(), State::Open));
    }

    #[test]
    fn nothing_is_done_while_a_file_cannot_be_read() {
        let d = setup();
        let s = suggest(&d, &jane(), TEXT, "rapidly", "fast");
        Review::decide(&d, &jane(), &s, Decision::Accepted).unwrap();
        fs::write(store::person_path(&d, "broken"), "[[record]\nid = ").unwrap();
        assert!(cleanup(&d, &jane()).is_err());
        assert_eq!(ids(&d, "jane-doe").len(), 2);
    }

    #[test]
    fn the_file_is_rewritten_as_appending_would_have_left_it() {
        let d = setup();
        let s = suggest(&d, &jane(), TEXT, "rapidly", "fast");
        let keep = suggest(&d, &jane(), TEXT, "Fig. 3", "\"Fig. 4\"\nsecond line");
        Review::decide(&d, &jane(), &s, Decision::Accepted).unwrap();
        cleanup(&d, &jane()).unwrap();
        let path = store::person_path(&d, "jane-doe");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("[person]\nname = \"Jane Doe\"\nemail = \"jane@example.org\"\nthrough = 3\n\n[[record]]\nid = \"jane-doe-2\""), "{text}");
        assert!(!path.with_extension("toml.tmp").exists());
        // and appending on goes on as before
        let after = suggest(&d, &jane(), TEXT, "slopes", "tilts");
        assert_eq!(after, "jane-doe-4");
        assert_eq!(ids(&d, "jane-doe"), [keep, after]);
    }
}
