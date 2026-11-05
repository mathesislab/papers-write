//! A co-author's commits as suggestions: the edits made in git, without the app, from where they started (the
//! merge-base of their branch, or the commit before the first one to review) to where they ended (the branch's tip,
//! or HEAD).
//!
//! The suggestions are the word-level hunks of the diff from the start text to the end text, with positions in the
//! start text and recorded with that text as their base. The review's own anchoring carries them onto the current
//! text, so what the author wrote since the start is never undone by one: only the co-author's edits appear, and
//! where the author changed the same words the suggestion is outdated.
//!
//! Each hunk is put down to the commit that made it, under its author's name, with the commit as the round
//! (`a1b2c3d Tighten the abstract`) and its date as the record's time. Who made what is read from `git blame`
//! ([`Attribution::read`]): of the lines with inserted text, the commit of the range that last wrote them; of deleted
//! lines (`--reverse`), the commit after the last one in which they still existed that touched the file; else the
//! newest commit of the range. Blame knows lines, not words: two commits that changed the same line both come out as
//! the later one.

use super::diff::{Hunk, hunks};
use super::ops::NewRecord;
use super::record::{Kind, Person};
use super::store::Result;
use super::text::{blob_hash, byte_of, rfc3339, slug, utf16_of};
use super::Review;
use std::collections::HashMap;
use std::path::Path;

/// A commit that made changes: who, when, what it says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub author: Person,
    /// the author's date, RFC 3339 in UTC
    pub time: String,
    pub subject: String,
}

impl Commit {
    /// The round of its suggestions: the short hash and the subject, `a1b2c3d Tighten the abstract`.
    pub fn round(&self) -> String {
        let short = &self.hash[..self.hash.len().min(7)];
        format!("{short} {}", self.subject).trim().to_string()
    }
}

/// A suggestion taken from the history: positions in the start text, and who made it (its round and time set).
#[derive(Clone, Debug)]
pub struct Change {
    pub author: Person,
    pub record: NewRecord,
}

/// The suggestions that turn `start` into `end`, in order (the hunks of `compare::suggestions`), with positions in
/// UTF-16 code units of `start`, each put down to the commit `who` names for its hunk (its range in `start` and in
/// `end`, in bytes).
pub fn suggestions(start: &str, end: &str, who: impl Fn(&Hunk) -> Commit) -> Vec<Change> {
    hunks(start, end)
        .into_iter()
        .map(|h| {
            let commit = who(&h);
            let record = NewRecord::suggestion(utf16_of(start, h.old.start), utf16_of(start, h.old.end), &end[h.new.clone()])
                .round(commit.round())
                .at(commit.time.clone());
            Change { author: commit.author, record }
        })
        .collect()
}

/// What [`add`] did: the suggestions written, by whom (names, in the order met), and those the review had already.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Added {
    pub added: usize,
    pub already: usize,
    pub people: Vec<String>,
}

/// `changes` (positions in `start`) written to the review of `doc` with `start` as their base, each in its author's
/// file. A change whose author has the same suggestion on the same base in the same round already is not written
/// again: the same range reviewed twice adds nothing.
pub fn add(doc: &Path, start: &str, changes: &[Change]) -> Result<Added> {
    let review = Review::load(doc)?;
    let base = blob_hash(start);
    let mut done = Added::default();
    for c in changes {
        let r = &c.record;
        let (s, e) = (byte_of(start, r.from), byte_of(start, r.to));
        let slug = slug(&c.author.name);
        let there = review.files.iter().filter(|f| f.slug == slug).flat_map(|f| f.records.iter()).any(|x| {
            x.kind == Kind::Suggestion
                && x.base.as_deref() == Some(base.as_str())
                && x.round == r.round
                && (x.start, x.end) == (Some(s), Some(e))
                && x.insert == r.insert
        });
        if there {
            done.already += 1;
            continue;
        }
        Review::add(doc, &c.author, start, r.clone())?;
        done.added += 1;
        if !done.people.contains(&c.author.name) {
            done.people.push(c.author.name.clone());
        }
    }
    Ok(done)
}

/// Who made the lines of a range of history, from `git blame`.
#[derive(Clone, Debug, Default)]
pub struct Attribution {
    /// the commits of the range that touched the file, oldest first
    pub commits: Vec<Commit>,
    /// for each line of the end text, the commit (an index in `commits`) that last wrote it; none: older than the range
    pub added: Vec<Option<usize>>,
    /// for each line of the start text, the commit that removed or changed it; none: it is still there at the end
    pub removed: Vec<Option<usize>>,
}

/// The line (from 0) that byte `at` of `text` is on.
fn line_of(text: &str, at: usize) -> usize {
    text.as_bytes()[..at.min(text.len())].iter().filter(|&&b| b == b'\n').count()
}

/// The lines a range covers: those of its first and last bytes (an empty range: the line it is on).
fn lines(text: &str, r: &std::ops::Range<usize>) -> std::ops::RangeInclusive<usize> {
    let first = line_of(text, r.start);
    first..=if r.is_empty() { first } else { line_of(text, r.end - 1) }
}

impl Attribution {
    /// The commit that made the hunk `h` of the diff from `start` to `end`: the newest of those that wrote the lines
    /// of its inserted text, else of those that removed the lines of its deleted text, else the newest of the range.
    /// None when the range has no commit.
    pub fn commit_of(&self, start: &str, end: &str, h: &Hunk) -> Option<&Commit> {
        let newest = |of: &[Option<usize>], text: &str, r: &std::ops::Range<usize>| lines(text, r).filter_map(|l| of.get(l).copied().flatten()).max();
        let by_insert = if h.new.is_empty() { None } else { newest(&self.added, end, &h.new) };
        let by_delete = || if h.old.is_empty() { None } else { newest(&self.removed, start, &h.old) };
        let i = by_insert.or_else(by_delete).unwrap_or(self.commits.len().checked_sub(1)?);
        self.commits.get(i)
    }

    /// Who made the changes to `file` from the commit `start` to `end` (anything git takes, `start` an ancestor of
    /// `end`), as `git` (run in the folder `file` is named from) says. A blame that fails (the file is new in the
    /// range, or gone) leaves its side to the newest commit; a range in which no commit names the file, all of it.
    pub fn read(git: impl Fn(&[&str]) -> std::result::Result<String, String>, file: &str, start: &str, end: &str) -> std::result::Result<Attribution, String> {
        let range = format!("{start}..{end}");
        const FORMAT: &str = "--format=%H%x1f%an%x1f%ae%x1f%at%x1f%s";
        let mut commits = parse_log(&git(&["log", "--reverse", "--topo-order", FORMAT, &range, "--", file])?);
        if commits.is_empty() {
            // no commit names the file (it was renamed, say): all of it goes to the newest commit of the range
            commits = parse_log(&git(&["log", "-n", "1", FORMAT, end, "--"])?);
        }
        let index: HashMap<&str, usize> = commits.iter().enumerate().map(|(i, c)| (c.hash.as_str(), i)).collect();
        // every commit of the range, oldest first: a line last seen in one that did not touch the file was removed by
        // the next that did
        let order: Vec<String> = git(&["rev-list", "--reverse", "--topo-order", &range])?.lines().map(str::to_string).collect();
        let tip = git(&["rev-parse", &format!("{end}^{{commit}}")])?.trim().to_string();
        let added = git(&["blame", "--porcelain", &range, "--", file])
            .map(|out| parse_blame(&out).into_iter().map(|(h, boundary)| if boundary { None } else { index.get(h.as_str()).copied() }).collect())
            .unwrap_or_default();
        let removed = git(&["blame", "--porcelain", "--reverse", &range, "--", file])
            .map(|out| {
                parse_blame(&out)
                    .into_iter()
                    .map(|(seen, _)| {
                        if seen == tip {
                            return None;
                        }
                        let after = order.iter().position(|h| *h == seen).map_or(0, |p| p + 1);
                        order[after.min(order.len())..].iter().find_map(|h| index.get(h.as_str()).copied())
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Attribution { commits, added, removed })
    }
}

/// `git log --format=%H%x1f%an%x1f%ae%x1f%at%x1f%s` read.
pub fn parse_log(out: &str) -> Vec<Commit> {
    out.lines()
        .filter_map(|line| {
            let mut f = line.splitn(5, '\u{1f}');
            let (hash, name, email, at, subject) = (f.next()?, f.next()?, f.next()?, f.next()?, f.next().unwrap_or(""));
            (!hash.is_empty()).then(|| Commit {
                hash: hash.to_string(),
                author: Person::new(name, email),
                time: rfc3339(at.trim().parse().unwrap_or(0)),
                subject: subject.to_string(),
            })
        })
        .collect()
}

/// `git blame --porcelain` read: for each line of the file blamed (by its number there), the commit and whether it
/// is the boundary of the range (the line is older than it).
pub fn parse_blame(out: &str) -> Vec<(String, bool)> {
    let mut boundary: HashMap<String, bool> = HashMap::new();
    let mut lines: Vec<(usize, String)> = Vec::new();
    let mut current = String::new();
    for line in out.lines() {
        if line.starts_with('\t') {
            continue;
        }
        let mut f = line.split(' ');
        let first = f.next().unwrap_or("");
        if first.len() >= 40 && first.bytes().all(|b| b.is_ascii_hexdigit()) {
            let _orig = f.next();
            if let Some(n) = f.next().and_then(|n| n.parse::<usize>().ok()).filter(|&n| n > 0) {
                current = first.to_string();
                boundary.entry(current.clone()).or_insert(false);
                lines.push((n, current.clone()));
            }
        } else if line == "boundary" {
            boundary.insert(current.clone(), true);
        }
    }
    let mut out: Vec<(String, bool)> = vec![(String::new(), true); lines.iter().map(|(n, _)| *n).max().unwrap_or(0)];
    for (n, hash) in lines {
        let b = boundary.get(&hash).copied().unwrap_or(false);
        out[n - 1] = (hash, b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::{Status, apply, testutil::doc};
    use std::process::Command;

    fn git_in(dir: &Path) -> impl Fn(&[&str]) -> std::result::Result<String, String> + '_ {
        move |args| {
            let out = Command::new("git").arg("-C").arg(dir).args(args).output().map_err(|e| e.to_string())?;
            if out.status.success() {
                Ok(String::from_utf8_lossy(&out.stdout).into_owned())
            } else {
                Err(String::from_utf8_lossy(&out.stderr).into_owned())
            }
        }
    }

    /// `text` committed as paper.md by `who` on `date`.
    fn commit(dir: &Path, text: &str, who: &str, date: &str, subject: &str) {
        std::fs::write(dir.join("paper.md"), text).unwrap();
        let email = format!("{}@example.org", slug(who));
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["commit", "-q", "-a", "-m", subject])
            .env("GIT_AUTHOR_NAME", who)
            .env("GIT_AUTHOR_EMAIL", &email)
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .status()
            .unwrap()
            .success();
        assert!(ok, "commit {subject}");
    }

    const START: &str = "The grounding line retreats rapidly when the bed slopes inland.\n\
                         Results are robust.\n\
                         We show this in Fig. 3 for all the runs.\n\
                         The ice shelf thins by melting at its base.\n";

    #[test]
    fn a_branch_of_two_authors_as_their_suggestions() {
        let d = doc("paper.md");
        let dir = d.parent().unwrap().to_path_buf();
        let git = git_in(&dir);
        if git(&["init", "-q", "-b", "main"]).is_err() {
            return;
        }
        for (k, v) in [("user.name", "Alex Robinson"), ("user.email", "alex@example.org"), ("commit.gpgsign", "false")] {
            git(&["config", k, v]).unwrap();
        }
        std::fs::write(&d, START).unwrap();
        git(&["add", "paper.md"]).unwrap();
        commit(&dir, START, "Alex Robinson", "2026-10-30T10:00:00Z", "First draft");
        // the co-authors' branch
        git(&["checkout", "-q", "-b", "edits"]).unwrap();
        let jane1 = START.replace("rapidly", "irreversibly");
        commit(&dir, &jane1, "Jane Doe", "2026-11-01T10:00:00Z", "Stronger claim");
        let bob = jane1.replace(" for all the runs", "").replace("at its base", "at its underside");
        commit(&dir, &bob, "Bob Smith", "2026-11-02T10:00:00Z", "Shorter");
        // a commit that does not touch the document between
        std::fs::write(dir.join("notes.txt"), "n\n").unwrap();
        git(&["add", "notes.txt"]).unwrap();
        commit(&dir, &bob, "Bob Smith", "2026-11-02T11:00:00Z", "Notes");
        let end = bob.replace("Results are robust.\n", "");
        commit(&dir, &end, "Jane Doe", "2026-11-03T10:00:00Z", "No claim of robustness");
        // the author goes on on main: the opening, and the same words as Bob's change
        git(&["checkout", "-q", "main"]).unwrap();
        let current = START.replace("The grounding line", "In our model the grounding line").replace("at its base", "at its bottom");
        commit(&dir, &current, "Alex Robinson", "2026-11-04T10:00:00Z", "Opening");

        let base = git(&["merge-base", "HEAD", "edits"]).unwrap().trim().to_string();
        let start = git(&["show", &format!("{base}:paper.md")]).unwrap();
        assert_eq!(start, START);
        let tip = git(&["show", "edits:paper.md"]).unwrap();
        let who = Attribution::read(&git, "paper.md", &base, "edits").unwrap();
        assert_eq!(who.commits.len(), 3, "the commits that touched the file");
        let changes = suggestions(&start, &tip, |h| who.commit_of(&start, &tip, h).unwrap().clone());
        let got: Vec<(&str, &str)> = changes.iter().map(|c| (c.author.name.as_str(), c.record.round.as_deref().unwrap())).collect();
        assert_eq!(got.len(), 4, "{got:?}");
        assert_eq!(got[0].0, "Jane Doe");
        assert!(got[0].1.ends_with(" Stronger claim"), "{got:?}");
        assert_eq!(got[1].0, "Jane Doe");
        assert!(got[1].1.ends_with(" No claim of robustness"), "a line deleted: {got:?}");
        assert_eq!((got[2].0, got[3].0), ("Bob Smith", "Bob Smith"));
        assert!(got[2].1.ends_with(" Shorter") && got[3].1.ends_with(" Shorter"), "the deletion in a line, and a word replaced: {got:?}");
        assert_eq!(changes[0].record.time.as_deref(), Some("2026-11-01T10:00:00Z"));
        assert_eq!(changes[0].author.email, "jane-doe@example.org");

        let added = add(&d, &start, &changes).unwrap();
        assert_eq!(added, Added { added: 4, already: 0, people: vec!["Jane Doe".into(), "Bob Smith".into()] });
        let review = Review::load(&d).unwrap();
        assert_eq!(review.files.len(), 2, "one file per author");
        let items = review.items(&current);
        assert_eq!(items.len(), 4, "the author's own edit on main is not a suggestion");
        let jane = &items[0];
        assert_eq!((jane.author.as_str(), jane.record.time.as_str()), ("Jane Doe", "2026-11-01T10:00:00Z"));
        assert_eq!(jane.status, Status::Moved);
        assert_eq!(apply(&current, jane), current.replace("rapidly", "irreversibly"), "the author's opening stays");
        let underside = items.iter().find(|i| i.record.insert.as_deref() == Some("underside")).unwrap();
        assert_eq!(underside.author, "Bob Smith");
        assert!(matches!(&underside.status, Status::Outdated { now } if now == "bottom"), "{:?}", underside.status);
        let robust = items.iter().find(|i| i.record.exact.as_deref().is_some_and(|e| e.contains("robust"))).unwrap();
        assert_eq!((robust.status.clone(), robust.record.insert.as_deref()), (Status::Moved, Some("")));

        // the same range again adds nothing
        assert_eq!(add(&d, &start, &changes).unwrap(), Added { added: 0, already: 4, people: vec![] });
        assert_eq!(Review::load(&d).unwrap().records().count(), 4);
    }

    #[test]
    fn blame_output_read() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let out = format!(
            "{a} 1 1 1\nauthor Jane\nsummary two\nfilename f.md\n\tl1 X\n{b} 3 2 2\nauthor A\nsummary one\nboundary\nfilename f.md\n\tl3 c\n{b} 4 3\n\tl4 d\n"
        );
        assert_eq!(parse_blame(&out), vec![(a.clone(), false), (b.clone(), true), (b.clone(), true)]);
        let log = parse_log("abc\u{1f}Jane Doe\u{1f}jane@example.org\u{1f}1793534400\u{1f}Tighten: the abstract\n");
        assert_eq!(log[0].author, Person::new("Jane Doe", "jane@example.org"));
        assert_eq!(log[0].time, "2026-11-01T12:00:00Z");
        assert_eq!(log[0].round(), "abc Tighten: the abstract");
    }

    #[test]
    fn a_hunk_falls_back_to_the_newest_commit() {
        let c = |h: &str| Commit { hash: h.into(), author: Person::new(h, ""), time: String::new(), subject: String::new() };
        let who = Attribution { commits: vec![c("one"), c("two")], added: vec![], removed: vec![] };
        let h = Hunk { old: 0..1, new: 0..1 };
        assert_eq!(who.commit_of("a", "b", &h).unwrap().hash, "two");
        assert!(Attribution::default().commit_of("a", "b", &h).is_none());
    }
}
