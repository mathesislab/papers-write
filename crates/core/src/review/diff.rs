//! The difference between two versions of a text, at the level of words, and positions carried through it.
//!
//! The texts are cut into tokens (a run of letters and digits, a run of white space, or one other character: a
//! punctuation mark) and the tokens diffed (`similar`, Myers, with a deadline for very large texts). The changes
//! are then cleaned up for prose: two changes with only white space between them become one (`a b` to `x y` is one
//! change, not two), so a rewritten phrase is one hunk. Hunks are in byte offsets of both texts.

use similar::{Algorithm, DiffTag, capture_diff_slices_deadline};
use std::ops::Range;
use std::time::{Duration, Instant};

/// A changed stretch: `old` of the first text became `new` of the second (either may be empty).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub old: Range<usize>,
    pub new: Range<usize>,
}

fn tokens(s: &str) -> Vec<&str> {
    #[derive(PartialEq, Clone, Copy)]
    enum Class {
        Word,
        Space,
        Other,
    }
    let class = |c: char| {
        if c.is_alphanumeric() {
            Class::Word
        } else if c.is_whitespace() {
            Class::Space
        } else {
            Class::Other
        }
    };
    let mut out = Vec::new();
    let mut start = 0;
    let mut prev: Option<Class> = None;
    for (i, c) in s.char_indices() {
        let k = class(c);
        if prev.is_some_and(|p| p != k || k == Class::Other) {
            out.push(&s[start..i]);
            start = i;
        }
        prev = Some(k);
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

fn offsets(tokens: &[&str]) -> Vec<usize> {
    let mut at = 0;
    let mut v = vec![0];
    for t in tokens {
        at += t.len();
        v.push(at);
    }
    v
}

/// The changes from `a` to `b`, in order.
pub fn hunks(a: &str, b: &str) -> Vec<Hunk> {
    if a == b {
        return Vec::new();
    }
    let (ta, tb) = (tokens(a), tokens(b));
    let (oa, ob) = (offsets(&ta), offsets(&tb));
    let ops = capture_diff_slices_deadline(Algorithm::Myers, &ta, &tb, Some(Instant::now() + Duration::from_secs(2)));
    let mut out: Vec<Hunk> = Vec::new();
    for op in ops {
        let (tag, o, n) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            continue;
        }
        let h = Hunk { old: oa[o.start]..oa[o.end], new: ob[n.start]..ob[n.end] };
        match out.last_mut() {
            Some(last) if last.old.end == h.old.start && last.new.end == h.new.start => {
                last.old.end = h.old.end;
                last.new.end = h.new.end;
            }
            _ => out.push(h),
        }
    }
    // cleanup: changes separated by white space only are one change
    let mut merged: Vec<Hunk> = Vec::new();
    for h in out {
        match merged.last_mut() {
            Some(last) if !a[last.old.end..h.old.start].is_empty() && a[last.old.end..h.old.start].chars().all(char::is_whitespace) => {
                last.old.end = h.old.end;
                last.new.end = h.new.end;
            }
            _ => merged.push(h),
        }
    }
    merged
}

/// The positions of a text carried onto another through the hunks between them.
pub struct Diff {
    pub hunks: Vec<Hunk>,
}

impl Diff {
    pub fn new(a: &str, b: &str) -> Diff {
        Diff { hunks: hunks(a, b) }
    }

    /// The hunks overlapping the old range `s..e` (for `s == e`: containing that point strictly inside).
    pub fn overlapping(&self, s: usize, e: usize) -> impl Iterator<Item = &Hunk> {
        self.hunks.iter().filter(move |h| h.old.start < e && s < h.old.end)
    }

    fn shift(&self, p: usize, skip_empty_at_p: bool) -> isize {
        self.hunks
            .iter()
            .filter(|h| h.old.end <= p && !(skip_empty_at_p && h.old.is_empty() && h.old.start == p))
            .map(|h| h.new.len() as isize - h.old.len() as isize)
            .sum()
    }

    fn inside(&self, p: usize) -> Option<&Hunk> {
        self.hunks.iter().find(|h| h.old.start < p && p < h.old.end)
    }

    /// Where the start of a range at `p` is now (an insertion at `p` lies before it); inside a changed hunk, the
    /// start of what that hunk became.
    pub fn map_start(&self, p: usize) -> usize {
        match self.inside(p) {
            Some(h) => h.new.start,
            None => (p as isize + self.shift(p, false)) as usize,
        }
    }

    /// Where the end of a range at `p` is now (an insertion at `p` lies after it).
    pub fn map_end(&self, p: usize) -> usize {
        match self.inside(p) {
            Some(h) => h.new.end,
            None => (p as isize + self.shift(p, true)) as usize,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_of_prose() {
        assert_eq!(tokens("the ice-sheet, 2 é."), ["the", " ", "ice", "-", "sheet", ",", " ", "2", " ", "é", "."]);
        assert_eq!(tokens("a\r\n\r\nb"), ["a", "\r\n\r\n", "b"]);
        assert!(tokens("").is_empty());
    }

    #[test]
    fn word_level_hunks() {
        let a = "the grounding line retreats rapidly when the bed slopes";
        let b = "the grounding line retreats irreversibly when the bed slopes";
        let h = hunks(a, b);
        assert_eq!(h.len(), 1);
        assert_eq!(&a[h[0].old.clone()], "rapidly");
        assert_eq!(&b[h[0].new.clone()], "irreversibly");
    }

    #[test]
    fn changes_across_a_space_are_one_hunk() {
        let a = "we saw a big drop here";
        let b = "we saw the huge drop here";
        let h = hunks(a, b);
        assert_eq!(h.len(), 1, "{h:?}");
        assert_eq!(&a[h[0].old.clone()], "a big");
        assert_eq!(&b[h[0].new.clone()], "the huge");
    }

    #[test]
    fn mapping() {
        let a = "one two three four";
        let b = "zero one two 3 four";
        let d = Diff::new(a, b);
        assert_eq!(d.map_start(0), 5, "after the insertion before it");
        assert_eq!(&b[d.map_start(4)..d.map_end(7)], "two");
        // 'three' (8..13) became '3'
        assert_eq!(d.overlapping(8, 13).count(), 1);
        assert_eq!(&b[d.map_start(8)..d.map_end(13)], "3");
        assert_eq!(d.overlapping(0, 3).count(), 0);
    }
}
