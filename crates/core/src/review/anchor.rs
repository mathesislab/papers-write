//! A record carried from the version it was made on to the current text.
//!
//! With the base text: the diff between base and current moves the range. Unchanged text is `Exact` (same place) or
//! `Moved`. If the changes touch the range itself, a suggestion is merged three ways (base, current, base with the
//! suggestion) and, when its own changes and the others' do not overlap, still applies: `Moved`, to the merged
//! range. Otherwise `Outdated`, or `Orphaned` when the text is gone. Without the base the quote is searched for.

use super::diff::{Diff, Hunk, hunks};
use super::record::{Kind, Record};
use serde::Serialize;
use std::ops::Range;

/// A range of the current text in UTF-16 code units (what CodeMirror counts).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Anchor {
    pub from: usize,
    pub to: usize,
}

/// How a record sits on the current text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Status {
    /// its text is unchanged and where it was (or the base is the current text)
    Exact,
    /// its text is unchanged but elsewhere, or a suggestion whose own text changed and that merged cleanly
    Moved,
    /// its own text changed (`now` is what is there now) and a suggestion did not merge
    Outdated { now: String },
    /// its text is gone: the range is collapsed
    Orphaned,
}

/// A record placed on the current text, by byte offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Carried {
    pub range: Range<usize>,
    pub status: Status,
    /// for a suggestion, the text to put at `range` (the merged one after a three-way merge)
    pub replacement: Option<String>,
}

fn valid(text: &str, s: usize, e: usize) -> bool {
    s <= e && e <= text.len() && text.is_char_boundary(s) && text.is_char_boundary(e)
}

pub fn carry(rec: &Record, base: Option<&str>, current: &str, current_hash: &str) -> Carried {
    let insert = (rec.kind == Kind::Suggestion).then(|| rec.insert.clone().unwrap_or_default());
    let found = |range: Range<usize>, status: Status| Carried { range, status, replacement: insert.clone() };
    let (Some(s), Some(e)) = (rec.start, rec.end) else {
        return orphan(rec, current, insert);
    };
    if rec.base.as_deref() == Some(current_hash) && valid(current, s, e) {
        return found(s..e, Status::Exact);
    }
    if let Some(base) = base.filter(|b| valid(b, s, e)) {
        return through_diff(base, current, s, e, insert);
    }
    by_quote(rec, current, insert)
}

fn through_diff(base: &str, current: &str, s: usize, e: usize, insert: Option<String>) -> Carried {
    let diff = Diff::new(base, current);
    let touched: Vec<&Hunk> = diff.overlapping(s, e).collect();
    if touched.is_empty() {
        let (ns, ne) = if s == e {
            let p = diff.map_start(s);
            (p, p)
        } else {
            (diff.map_start(s), diff.map_end(e))
        };
        let status = if ns == s { Status::Exact } else { Status::Moved };
        return Carried { range: ns..ne, status, replacement: insert };
    }
    let (ns, ne) = (diff.map_start(s), diff.map_end(e));
    if ns >= ne && s < e {
        return Carried { range: ns..ns, status: Status::Orphaned, replacement: insert };
    }
    if let Some(ins) = &insert
        && let Some((range, replacement)) = merge(base, current, &diff, s, e, ins)
    {
        return Carried { range, status: Status::Moved, replacement: Some(replacement) };
    }
    Carried { range: ns..ne, status: Status::Outdated { now: current[ns..ne].to_string() }, replacement: insert }
}

fn conflict(a: &Range<usize>, b: &Range<usize>) -> bool {
    if a.is_empty() || b.is_empty() { a.start <= b.end && b.start <= a.end } else { a.start < b.end && b.start < a.end }
}

/// The three-way merge restricted to the region of the suggestion: the suggestion's own changes (the word-level
/// diff of `exact` against `insert`) and the changes from base to current must not overlap. The result is the
/// range of the current text from the first to the last of the suggestion's changes, and what to put there.
fn merge(base: &str, current: &str, diff: &Diff, s: usize, e: usize, insert: &str) -> Option<(Range<usize>, String)> {
    let exact = &base[s..e];
    let ours: Vec<(Range<usize>, String)> = if exact.is_empty() || insert.is_empty() {
        vec![(s..e, insert.to_string())]
    } else {
        hunks(exact, insert).into_iter().map(|h| (s + h.old.start..s + h.old.end, insert[h.new].to_string())).collect()
    };
    if ours.is_empty() {
        return None;
    }
    if ours.iter().any(|(o, _)| diff.hunks.iter().any(|t| conflict(o, &t.old))) {
        return None;
    }
    let mut out = String::new();
    let mut first = None;
    let mut prev_end = 0;
    for (o, r) in &ours {
        let (ms, me) = if o.is_empty() {
            let p = diff.map_start(o.start);
            (p, p)
        } else {
            (diff.map_start(o.start), diff.map_end(o.end))
        };
        if ms > me || ms < prev_end || me > current.len() {
            return None;
        }
        if first.is_none() {
            first = Some(ms);
        } else {
            out.push_str(&current[prev_end..ms]);
        }
        out.push_str(r);
        prev_end = me;
    }
    Some((first?..prev_end, out))
}

fn orphan(rec: &Record, current: &str, insert: Option<String>) -> Carried {
    let mut p = rec.start.unwrap_or(0).min(current.len());
    while !current.is_char_boundary(p) {
        p -= 1;
    }
    Carried { range: p..p, status: Status::Orphaned, replacement: insert }
}

fn common_suffix_chars(a: &str, b: &str) -> usize {
    a.chars().rev().zip(b.chars().rev()).take_while(|(x, y)| x == y).count()
}

fn common_prefix_chars(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// Without the base: the quote searched for. The occurrence of `exact` whose surroundings agree best with the
/// recorded `prefix` and `suffix` wins, ties going to the one nearest the old offset. With no occurrence, a place
/// between the (whole) prefix and suffix, if both are there close together, is `Outdated`; else `Orphaned`.
fn by_quote(rec: &Record, current: &str, insert: Option<String>) -> Carried {
    let (exact, prefix, suffix) = (
        rec.exact.as_deref().unwrap_or(""),
        rec.prefix.as_deref().unwrap_or(""),
        rec.suffix.as_deref().unwrap_or(""),
    );
    let old = rec.start.unwrap_or(0);
    let best = |candidates: Vec<usize>, len: usize, lead: usize| -> Option<usize> {
        candidates
            .into_iter()
            .max_by_key(|&i| {
                let at = i + lead;
                let score = common_suffix_chars(prefix, &current[..at]) + common_prefix_chars(suffix, &current[at + len..]);
                (score, std::cmp::Reverse(at.abs_diff(old)))
            })
            .map(|i| i + lead)
    };
    if !exact.is_empty() {
        let occ: Vec<usize> = current.match_indices(exact).map(|(i, _)| i).collect();
        if let Some(at) = best(occ, exact.len(), 0) {
            let status = if at == old { Status::Exact } else { Status::Moved };
            return Carried { range: at..at + exact.len(), status, replacement: insert };
        }
        if !prefix.is_empty() && !suffix.is_empty() {
            let near = |i: usize| i.abs_diff(old);
            if let Some(after) = current.match_indices(prefix).map(|(i, _)| i + prefix.len()).min_by_key(|&i| near(i))
                && let Some(len) = current[after..].find(suffix)
                && len <= exact.len() * 2 + 64
            {
                return Carried {
                    range: after..after + len,
                    status: Status::Outdated { now: current[after..after + len].to_string() },
                    replacement: insert,
                };
            }
        }
        return orphan(rec, current, insert);
    }
    // an insertion point: found by what stood around it
    let around = format!("{prefix}{suffix}");
    if around.is_empty() {
        return orphan(rec, current, insert);
    }
    let occ: Vec<usize> = current.match_indices(around.as_str()).map(|(i, _)| i).collect();
    match best(occ, 0, prefix.len()) {
        Some(at) => Carried { range: at..at, status: if at == old { Status::Exact } else { Status::Moved }, replacement: insert },
        None => orphan(rec, current, insert),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::testutil::make;
    use crate::review::text::blob_hash;

    const BASE: &str = "The grounding line retreats rapidly when the bed slopes inland. We show this in Fig. 3.";

    fn run(rec: &Record, base: Option<&str>, current: &str) -> Carried {
        carry(rec, base, current, &blob_hash(current))
    }

    fn at<'a>(c: &Carried, text: &'a str) -> &'a str {
        &text[c.range.clone()]
    }

    #[test]
    fn same_text_is_exact() {
        let r = make("a-1", Kind::Suggestion, BASE, "retreats rapidly", Some("retreats irreversibly"));
        let c = run(&r, None, BASE);
        assert_eq!(c.status, Status::Exact);
        assert_eq!(at(&c, BASE), "retreats rapidly");
        assert_eq!(c.replacement.as_deref(), Some("retreats irreversibly"));
    }

    #[test]
    fn text_inserted_before_moves_it() {
        let r = make("a-1", Kind::Suggestion, BASE, "retreats rapidly", Some("retreats irreversibly"));
        let cur = format!("Abstract. Here we go.\n\n{BASE}");
        let c = run(&r, Some(BASE), &cur);
        assert_eq!(c.status, Status::Moved);
        assert_eq!(at(&c, &cur), "retreats rapidly");
    }

    #[test]
    fn text_after_and_around_changed() {
        let r = make("a-1", Kind::Comment, BASE, "retreats rapidly", None);
        let cur = BASE.replace("The grounding", "Our grounding").replace("inland", "downstream of the ridge");
        let c = run(&r, Some(BASE), &cur);
        assert_eq!(c.status, Status::Exact, "same place, text unchanged");
        assert_eq!(at(&c, &cur), "retreats rapidly");
        let cur = BASE.replace("The grounding", "The marine grounding");
        let c = run(&r, Some(BASE), &cur);
        assert_eq!(c.status, Status::Moved);
        assert_eq!(at(&c, &cur), "retreats rapidly");
    }

    #[test]
    fn inside_changed_is_outdated_for_a_comment() {
        let r = make("a-1", Kind::Comment, BASE, "retreats rapidly", None);
        let cur = BASE.replace("rapidly", "quickly");
        let c = run(&r, Some(BASE), &cur);
        assert_eq!(c.status, Status::Outdated { now: "retreats quickly".into() });
        assert_eq!(at(&c, &cur), "retreats quickly");
    }

    #[test]
    fn inside_changed_suggestion_merges_when_the_changes_do_not_overlap() {
        let r = make("a-1", Kind::Suggestion, BASE, "retreats rapidly", Some("retreats irreversibly"));
        // the author changed 'retreats', the suggestion changes 'rapidly'
        let cur = BASE.replace("retreats", "recedes");
        let c = run(&r, Some(BASE), &cur);
        assert_eq!(c.status, Status::Moved);
        assert_eq!(at(&c, &cur), "rapidly");
        assert_eq!(c.replacement.as_deref(), Some("irreversibly"));
        // and applied: the author's word and the reviewer's
        let mut out = cur.clone();
        out.replace_range(c.range.clone(), c.replacement.as_deref().unwrap());
        assert!(out.contains("recedes irreversibly when"), "{out}");
    }

    #[test]
    fn inside_changed_suggestion_conflicting_is_outdated() {
        let r = make("a-1", Kind::Suggestion, BASE, "retreats rapidly", Some("retreats irreversibly"));
        let cur = BASE.replace("rapidly", "quickly");
        let c = run(&r, Some(BASE), &cur);
        assert_eq!(c.status, Status::Outdated { now: "retreats quickly".into() });
        assert_eq!(c.replacement.as_deref(), Some("retreats irreversibly"));
    }

    #[test]
    fn insertion_and_deletion_suggestions_merge() {
        let r = make("a-1", Kind::Suggestion, BASE, "", Some("very "));
        // an insertion before 'rapidly'
        let mut r = r;
        let p = BASE.find("rapidly").unwrap();
        r.start = Some(p);
        r.end = Some(p);
        let cur = BASE.replace("The grounding line", "The ice");
        let c = run(&r, Some(BASE), &cur);
        assert_eq!(c.range.start, cur.find("rapidly").unwrap());
        assert_eq!(c.status, Status::Moved);
        let d = make("a-2", Kind::Suggestion, BASE, " rapidly", Some(""));
        let c = run(&d, Some(BASE), &cur);
        assert_eq!(at(&c, &cur), " rapidly");
    }

    #[test]
    fn range_deleted_is_orphaned() {
        let r = make("a-1", Kind::Comment, BASE, "We show this in Fig. 3.", None);
        let cur = BASE.replace(" We show this in Fig. 3.", "");
        let c = run(&r, Some(BASE), &cur);
        assert_eq!(c.status, Status::Orphaned);
        assert!(c.range.is_empty());
        let s = make("a-2", Kind::Suggestion, BASE, "inland", Some("offshore"));
        let cur = BASE.replace(" inland", "");
        assert_eq!(run(&s, Some(BASE), &cur).status, Status::Orphaned);
    }

    #[test]
    fn lost_base_finds_the_quote_with_duplicates() {
        let base = "alpha beta gamma. delta beta epsilon. zeta beta eta.";
        // the second 'beta'
        let mut r = make("a-1", Kind::Comment, base, "beta", None);
        let second = base.match_indices("beta").nth(1).unwrap().0;
        r.start = Some(second);
        r.end = Some(second + 4);
        r.prefix = Some("delta ".into());
        r.suffix = Some(" epsilon".into());
        let cur = format!("PREFACE. {base}");
        let c = run(&r, None, &cur);
        assert_eq!(c.status, Status::Moved);
        assert_eq!(&cur[c.range.start - 6..c.range.end + 8], "delta beta epsilon");
        // context gone for all: the nearest the old offset wins
        r.prefix = Some("???".into());
        r.suffix = Some("???".into());
        let c = run(&r, None, &cur);
        assert_eq!(c.range.start, cur.match_indices("beta").nth(1).unwrap().0, "nearest to {second}");
    }

    #[test]
    fn lost_base_quote_gone_or_edited() {
        let r = make("a-1", Kind::Comment, BASE, "retreats rapidly", None);
        let cur = BASE.replace("retreats rapidly", "retreats quickly");
        let c = run(&r, None, &cur);
        assert_eq!(c.status, Status::Outdated { now: "retreats quickly".into() }, "found between its prefix and suffix");
        let gone = "Something else entirely.";
        assert_eq!(run(&r, None, gone).status, Status::Orphaned);
    }

    #[test]
    fn lost_base_insertion_point() {
        let mut r = make("a-1", Kind::Suggestion, BASE, "", Some("very "));
        let p = BASE.find("rapidly").unwrap();
        r.start = Some(p);
        r.end = Some(p);
        r.prefix = Some(crate::review::text::before(BASE, p, 32).into());
        r.suffix = Some(crate::review::text::after(BASE, p, 32).into());
        let cur = format!("New. {BASE}");
        let c = run(&r, None, &cur);
        assert_eq!(c.range, cur.find("rapidly").unwrap()..cur.find("rapidly").unwrap());
    }

    #[test]
    fn multibyte_text() {
        let base = "Café 😀 retreats rapidly – très.";
        let r = make("a-1", Kind::Suggestion, base, "rapidly", Some("fast"));
        let cur = format!("Ünï 😀😀 {base}");
        let c = run(&r, Some(base), &cur);
        assert_eq!(at(&c, &cur), "rapidly");
        let c = run(&r, None, &cur);
        assert_eq!(at(&c, &cur), "rapidly");
    }
}
