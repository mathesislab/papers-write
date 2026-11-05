//! Finding where a quote of plain text stands in Markdown.
//!
//! The text pandoc reads out of a .docx is plain; the document is Markdown, with `*`, `[@key]`, `$x^2$` and
//! `\cite{..}` in it. So the search is by words: a word is a run of letters and digits, and everything else
//! (punctuation, white space, Markdown's markup) is ignored. A quote is the words that changed with the words before
//! and after it; it stands where the changed words are found, the words around it deciding between several places.
//! A quote that is not found, or found in two places that the words around do not tell apart, is not placed.

use std::ops::Range;

/// The words of `text`: runs of letters and digits, by byte range.
pub fn words(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (c.is_alphanumeric(), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push(s..i);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push(s..text.len());
    }
    out
}

/// A text and its words, to search.
pub struct Index<'a> {
    pub text: &'a str,
    pub words: Vec<Range<usize>>,
}

/// How many words of context each side of a quote.
pub const CONTEXT: usize = 8;

/// A stretch of words of the index: `first..end` (an empty one is the gap before word `first`).
pub type Span = (usize, usize);

impl<'a> Index<'a> {
    pub fn new(text: &'a str) -> Self {
        Index { text, words: words(text) }
    }

    fn word(&self, i: usize) -> &str {
        &self.text[self.words[i].clone()]
    }

    /// How many of the words of `prefix`, from its end, stand just before word `at`.
    fn behind(&self, at: usize, prefix: &[&str]) -> usize {
        prefix.iter().rev().enumerate().take_while(|(k, w)| at > *k && self.word(at - 1 - k) == **w).count()
    }

    /// How many of the words of `suffix`, from its start, stand from word `at` on.
    fn ahead(&self, at: usize, suffix: &[&str]) -> usize {
        suffix.iter().enumerate().take_while(|(k, w)| at + k < self.words.len() && self.word(at + k) == **w).count()
    }

    /// The best of `candidates` (the words of the quote start at each, or for an empty quote it is the gap before
    /// each) by the words around it; none when two are as good, or when for a gap the words around agree too little.
    fn best(&self, candidates: impl Iterator<Item = usize>, len: usize, prefix: &[&str], suffix: &[&str]) -> Option<usize> {
        let mut scored: Vec<(usize, usize)> = candidates.map(|i| (self.behind(i, prefix) + self.ahead(i + len, suffix), i)).collect();
        scored.sort_by(|a, b| b.cmp(a));
        let (top, at) = *scored.first()?;
        if len == 0 && top < 2.min(prefix.len() + suffix.len()).max(1) {
            return None;
        }
        (scored.len() == 1 || scored[1].0 < top).then_some(at)
    }

    /// Where the quote is: `exact` (the words changed; none for a point) with the words `prefix` before and `suffix`
    /// after. A long quote that is not there whole is found by its ends.
    pub fn locate(&self, exact: &[&str], prefix: &[&str], suffix: &[&str]) -> Option<Span> {
        let n = self.words.len();
        let len = exact.len();
        if len == 0 {
            return self.best(0..=n, 0, prefix, suffix).map(|g| (g, g));
        }
        let whole = (0..(n + 1).saturating_sub(len)).filter(|&i| (0..len).all(|k| self.word(i + k) == exact[k]));
        if let Some(i) = self.best(whole, len, prefix, suffix) {
            return Some((i, i + len));
        }
        if len < 10 {
            return None;
        }
        // the ends: the first words after the prefix, the last words before the suffix, the last the nearest to where
        // a quote of this length would end
        let (head, tail) = (&exact[..4], &exact[len - 4..]);
        let found = |w: &[&str]| -> Vec<usize> { (0..(n + 1).saturating_sub(w.len())).filter(|&i| (0..w.len()).all(|k| self.word(i + k) == w[k])).collect() };
        let start = self.best(found(head).into_iter(), 4, prefix, &[])?;
        let end = found(tail).into_iter().filter(|&t| t >= start + 4).min_by_key(|&t| (t + 4).abs_diff(start + len))?;
        (suffix.is_empty() || self.ahead(end + 4, suffix) > 0).then_some((start, end + 4))
    }

    /// The byte range of a quote found: the first word to the end of the last.
    pub fn bytes(&self, (a, b): Span) -> Range<usize> {
        self.words[a].start..self.words[b - 1].end
    }

    /// Where an insertion goes, by the gap before word `gap`: after the word before it (and the `*`, `_`, `~` that
    /// close it), unless the text inserted ends in white space and does not begin with it, or there is no word before.
    pub fn point(&self, gap: usize, insert: &str) -> usize {
        let ends_in_space = insert.chars().next_back().is_some_and(char::is_whitespace);
        let begins_with_space = insert.chars().next().is_some_and(char::is_whitespace);
        if gap == 0 || (ends_in_space && !begins_with_space && gap < self.words.len()) {
            return self.words.get(gap).map_or(self.text.len(), |w| w.start);
        }
        let mut p = self.words[gap - 1].end;
        while self.text[p..].starts_with(['*', '_', '~']) {
            p += 1;
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<&str> {
        let r = words(s);
        r.into_iter().map(|r| &s[r]).collect()
    }

    #[test]
    fn words_ignore_markup() {
        assert_eq!(w("The *grounding line* [@smith2020] retreats, $x^2$ é-ü."), ["The", "grounding", "line", "smith2020", "retreats", "x", "2", "é", "ü"]);
    }

    #[test]
    fn a_quote_is_found_through_the_markup() {
        let t = "The *grounding line* retreats [@smith2020] rapidly when the bed slopes.";
        let ix = Index::new(t);
        let span = ix.locate(&["rapidly"], &["retreats", "smith", "2020"], &["when", "the"]).unwrap();
        assert_eq!(&t[ix.bytes(span)], "rapidly");
        // two words across the emphasis
        let span = ix.locate(&["grounding", "line"], &["The"], &["retreats"]).unwrap();
        assert_eq!(&t[ix.bytes(span)], "grounding line");
    }

    #[test]
    fn the_words_around_choose_between_places() {
        let t = "the ice flows. a slow ice stream. the ice melts.";
        let ix = Index::new(t);
        let span = ix.locate(&["ice"], &["a", "slow"], &["stream"]).unwrap();
        assert_eq!(&t[ix.bytes(span)], "ice");
        assert_eq!(span.0, 5);
        assert!(ix.locate(&["ice"], &[], &[]).is_none(), "three places, none better");
        assert!(ix.locate(&["fire"], &[], &[]).is_none());
    }

    #[test]
    fn a_point_needs_words_around() {
        let t = "The *grounding line* retreats now.";
        let ix = Index::new(t);
        let (g, _) = ix.locate(&[], &["grounding", "line"], &["retreats", "now"]).unwrap();
        assert_eq!(g, 3);
        assert_eq!(ix.point(g, " very"), "The *grounding line*".len());
        assert_eq!(ix.point(g, "very "), "The *grounding line* ".len());
        assert!(ix.locate(&[], &["now"], &["zzz"]).is_none(), "one word of two is too little");
    }

    #[test]
    fn a_long_quote_is_found_by_its_ends() {
        let t = "alpha beta gamma delta epsilon zeta (Smith, 2020) eta theta iota kappa lambda mu nu";
        let ix = Index::new(t);
        let exact: Vec<&str> = "beta gamma delta epsilon zeta Smith 2020 eta theta iota kappa lambda".split(' ').collect();
        // the citation as Markdown has it: 'Smith, 2020' stands for it, so make the quote disagree
        let mut exact = exact;
        exact[5] = "Smyth";
        let span = ix.locate(&exact, &["alpha"], &["mu", "nu"]).unwrap();
        assert_eq!(&t[ix.bytes(span)], "beta gamma delta epsilon zeta (Smith, 2020) eta theta iota kappa lambda");
    }
}
