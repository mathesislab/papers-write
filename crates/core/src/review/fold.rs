//! The records of all the files folded into items, each carried onto the current text.

use super::anchor::{Anchor, Carried, Status, carry};
use super::record::{Decision, Kind, Record};
use super::text::{blob_hash, utf16_of};
use super::Review;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Open,
    Accepted,
    Rejected,
    Resolved,
    Withdrawn,
}

/// A suggestion or comment (a counter-suggestion too, with `on` set) as it stands on the current text.
#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    /// the name in the person's file header, else the file name
    pub author: String,
    /// the suggestion this one answers, if it is a counter-suggestion
    pub on: Option<String>,
    /// the latest record of it (after edits by its author)
    pub record: Record,
    /// replies and counter-suggestions to it, all depths, in time order
    pub thread: Vec<Record>,
    pub state: State,
    /// in UTF-16 code units of the current text
    pub at: Anchor,
    pub status: Status,
    /// for a suggestion, what goes at `at` (differs from `record.insert` after a three-way merge)
    pub replacement: Option<String>,
    /// `at` as byte offsets of the current text
    #[serde(skip)]
    pub range: Range<usize>,
}

impl Review {
    /// The items of the review carried onto `current_text`, in the order of their place in it.
    pub fn items(&self, current_text: &str) -> Vec<Item> {
        let cur_hash = blob_hash(current_text);
        let mut author_of: HashMap<&str, String> = HashMap::new();
        for f in &self.files {
            let name = f.person.as_ref().map_or_else(|| f.slug.clone(), |p| p.name.clone());
            for r in &f.records {
                author_of.insert(r.id.as_str(), name.clone());
            }
        }
        // an edited suggestion: the newer record stands for the older (and for what was said of it)
        let mut newer: HashMap<&str, &str> = HashMap::new();
        for r in self.records() {
            if let Some(old) = r.supersedes.as_deref() {
                newer.insert(old, r.id.as_str());
            }
        }
        let canon = |id: &str| -> String {
            let mut id = id;
            for _ in 0..newer.len() + 1 {
                match newer.get(id) {
                    Some(n) => id = n,
                    None => break,
                }
            }
            id.to_string()
        };
        // decisions: the latest of each wins
        let mut decisions: HashMap<String, &Record> = HashMap::new();
        for r in self.records().filter(|r| r.kind == Kind::Decision) {
            let Some(on) = r.on.as_deref() else { continue };
            let slot = decisions.entry(canon(on)).or_insert(r);
            if r.order_key() > slot.order_key() {
                *slot = r;
            }
        }
        // children: replies and counter-suggestions, by the item they answer
        let mut children: HashMap<String, Vec<&Record>> = HashMap::new();
        for r in self.records() {
            let child = match r.kind {
                Kind::Reply => true,
                Kind::Suggestion | Kind::Comment => r.on.is_some() && !newer.contains_key(r.id.as_str()),
                Kind::Decision => false,
            };
            if let (true, Some(on)) = (child, r.on.as_deref()) {
                children.entry(canon(on)).or_default().push(r);
            }
        }
        let thread_of = |id: &str| -> Vec<Record> {
            let mut out: Vec<&Record> = Vec::new();
            let mut seen: HashSet<&str> = HashSet::from([id]);
            let mut stack = vec![id.to_string()];
            while let Some(top) = stack.pop() {
                for c in children.get(&top).into_iter().flatten() {
                    if seen.insert(c.id.as_str()) {
                        out.push(c);
                        stack.push(c.id.clone());
                    }
                }
            }
            out.sort_by_key(|r| r.order_key());
            out.into_iter().cloned().collect()
        };

        let mut items: Vec<Item> = self
            .records()
            .filter(|r| matches!(r.kind, Kind::Suggestion | Kind::Comment) && !newer.contains_key(r.id.as_str()))
            .map(|r| {
                let base = r.base.as_deref().and_then(|h| if h == cur_hash { None } else { self.base(h) });
                let Carried { range, status, replacement } = carry(r, base.as_deref(), current_text, &cur_hash);
                let state = match decisions.get(&r.id).and_then(|d| d.decision) {
                    Some(Decision::Accepted) => State::Accepted,
                    Some(Decision::Rejected) => State::Rejected,
                    Some(Decision::Resolved) => State::Resolved,
                    Some(Decision::Withdrawn) => State::Withdrawn,
                    Some(Decision::Reopened) | None => State::Open,
                };
                Item {
                    id: r.id.clone(),
                    kind: r.kind,
                    author: author_of.get(r.id.as_str()).cloned().unwrap_or_default(),
                    on: r.on.as_deref().map(&canon),
                    record: r.clone(),
                    thread: thread_of(&r.id),
                    state,
                    at: Anchor { from: utf16_of(current_text, range.start), to: utf16_of(current_text, range.end) },
                    status,
                    replacement,
                    range,
                }
            })
            .collect();
        items.sort_by(|a, b| (a.at.from, a.at.to, a.record.order_key()).cmp(&(b.at.from, b.at.to, b.record.order_key())));
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::record::Person;
    use crate::review::testutil::{doc, make};
    use crate::review::{store, Review};
    use std::path::Path;

    const BASE: &str = "The grounding line retreats rapidly when the bed slopes inland. We show this in Fig. 3.";

    fn put(d: &Path, name: &str, rec: &Record) {
        let me = Person::new(name, "");
        store::append(&store::person_path(d, &super::super::text::slug(name)), &me, rec).unwrap();
    }

    fn at_time(mut r: Record, t: &str) -> Record {
        r.time = format!("2026-11-05T{t}Z");
        r
    }

    fn reply(id: &str, on: &str, t: &str) -> Record {
        let mut r = Record::blank(id.into(), Kind::Reply);
        r.on = Some(on.into());
        r.text = Some(format!("reply {id}"));
        at_time(r, t)
    }

    fn decision(id: &str, on: &str, d: Decision, t: &str) -> Record {
        let mut r = Record::blank(id.into(), Kind::Decision);
        r.on = Some(on.into());
        r.decision = Some(d);
        at_time(r, t)
    }

    fn setup() -> std::path::PathBuf {
        let d = doc("paper.md");
        store::write_base(&d, BASE).unwrap();
        d
    }

    #[test]
    fn replies_and_counter_suggestions_in_the_thread_in_time_order() {
        let d = setup();
        let s = at_time(make("jane-1", Kind::Suggestion, BASE, "retreats rapidly", Some("retreats irreversibly")), "10:00:00");
        put(&d, "Jane", &s);
        // a counter-suggestion made on the text with the first applied
        let applied = BASE.replace("retreats rapidly", "retreats irreversibly");
        store::write_base(&d, &applied).unwrap();
        let mut c = at_time(make("alex-1", Kind::Suggestion, &applied, "irreversibly", Some("slowly")), "11:00:00");
        c.on = Some("jane-1".into());
        put(&d, "Alex", &c);
        put(&d, "Alex", &reply("alex-2", "jane-1", "10:30:00"));
        put(&d, "Jane", &reply("jane-2", "alex-1", "12:00:00"));
        let items = Review::load(&d).unwrap().items(BASE);
        assert_eq!(items.len(), 2);
        let first = items.iter().find(|i| i.id == "jane-1").unwrap();
        let ids: Vec<_> = first.thread.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["alex-2", "alex-1", "jane-2"], "replies and counters, all depths, by time");
        assert_eq!(first.author, "Jane");
        assert_eq!(first.on, None);
        let counter = items.iter().find(|i| i.id == "alex-1").unwrap();
        assert_eq!(counter.on.as_deref(), Some("jane-1"));
        assert_eq!(counter.thread.len(), 1);
        // the counter is on the text as the first leaves it: carried onto the current text it is outdated or merged
        assert_ne!(counter.status, Status::Exact);
    }

    #[test]
    fn decisions_latest_wins_whatever_the_file_order() {
        let d = setup();
        put(&d, "Jane", &at_time(make("jane-1", Kind::Suggestion, BASE, "rapidly", Some("fast")), "10:00:00"));
        // alex.toml sorts before jane.toml: the later decision is in the earlier file
        put(&d, "Alex", &decision("alex-1", "jane-1", Decision::Rejected, "13:00:00"));
        put(&d, "Jane", &decision("jane-2", "jane-1", Decision::Withdrawn, "11:00:00"));
        let state = |d: &Path| Review::load(d).unwrap().items(BASE)[0].state;
        assert_eq!(state(&d), State::Rejected);
        put(&d, "Jane", &decision("jane-3", "jane-1", Decision::Reopened, "14:00:00"));
        assert_eq!(state(&d), State::Open);
        put(&d, "Alex", &decision("alex-2", "jane-1", Decision::Accepted, "15:00:00"));
        assert_eq!(state(&d), State::Accepted);
        put(&d, "Alex", &decision("alex-3", "jane-1", Decision::Resolved, "16:00:00"));
        assert_eq!(state(&d), State::Resolved);
        put(&d, "Jane", &decision("jane-4", "jane-1", Decision::Withdrawn, "17:00:00"));
        assert_eq!(state(&d), State::Withdrawn);
    }

    #[test]
    fn supersedes_keeps_the_latest_and_what_was_said_of_the_old() {
        let d = setup();
        put(&d, "Jane", &at_time(make("jane-1", Kind::Suggestion, BASE, "rapidly", Some("fast")), "10:00:00"));
        put(&d, "Alex", &reply("alex-1", "jane-1", "10:10:00"));
        let mut v2 = at_time(make("jane-2", Kind::Suggestion, BASE, "rapidly", Some("quickly")), "10:20:00");
        v2.supersedes = Some("jane-1".into());
        put(&d, "Jane", &v2);
        let mut v3 = at_time(make("jane-3", Kind::Suggestion, BASE, "rapidly", Some("irreversibly")), "10:30:00");
        v3.supersedes = Some("jane-2".into());
        put(&d, "Jane", &v3);
        put(&d, "Alex", &decision("alex-2", "jane-1", Decision::Accepted, "11:00:00"));
        let items = Review::load(&d).unwrap().items(BASE);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "jane-3");
        assert_eq!(items[0].record.insert.as_deref(), Some("irreversibly"));
        assert_eq!(items[0].thread.len(), 1, "the reply to the first version");
        assert_eq!(items[0].state, State::Accepted, "a decision on the first version stands for the latest");
    }

    #[test]
    fn two_people_two_files_all_show_together_and_in_text_order() {
        let d = setup();
        put(&d, "Jane", &at_time(make("jane-1", Kind::Comment, BASE, "We show this", None), "10:00:00"));
        put(&d, "Alex", &at_time(make("alex-1", Kind::Suggestion, BASE, "The grounding", Some("A grounding")), "09:00:00"));
        // a git merge of two branches that both appended: the two files are just there
        let items = Review::load(&d).unwrap().items(BASE);
        let ids: Vec<_> = items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["alex-1", "jane-1"]);
        assert_eq!(items[0].author, "Alex");
        assert!(items.iter().all(|i| i.status == Status::Exact));
    }

    #[test]
    fn items_are_carried_onto_the_current_text_in_utf16() {
        let d = setup();
        put(&d, "Jane", &at_time(make("jane-1", Kind::Suggestion, BASE, "rapidly", Some("fast")), "10:00:00"));
        let cur = format!("é😀 {BASE}");
        let items = Review::load(&d).unwrap().items(&cur);
        let i = &items[0];
        assert_eq!(i.status, Status::Moved);
        let from = "é😀 ".encode_utf16().count() + "The grounding line retreats ".len();
        assert_eq!((i.at.from, i.at.to), (from, from + "rapidly".len()));
        assert_eq!(&cur[i.range.clone()], "rapidly");
    }

    #[test]
    fn a_broken_file_does_not_stop_the_others() {
        let d = setup();
        put(&d, "Jane", &at_time(make("jane-1", Kind::Comment, BASE, "rapidly", None), "10:00:00"));
        std::fs::write(store::person_path(&d, "zed"), "not toml [[").unwrap();
        let r = Review::load(&d).unwrap();
        assert_eq!(r.errors.len(), 1);
        assert_eq!(r.items(BASE).len(), 1);
    }

    #[test]
    fn serializes_for_the_app() {
        let d = setup();
        put(&d, "Jane", &at_time(make("jane-1", Kind::Suggestion, BASE, "rapidly", Some("fast")), "10:00:00"));
        let cur = BASE.replace("rapidly", "quickly");
        let items = Review::load(&d).unwrap().items(&cur);
        let json = toml::Value::try_from(&items[0]).unwrap().to_string();
        assert!(json.contains("type = \"outdated\""), "{json}");
        assert!(json.contains("state = \"open\""), "{json}");
    }
}
