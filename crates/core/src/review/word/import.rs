//! A .docx read back: its tracked changes and comments as suggestions and comments on the Markdown.
//!
//! pandoc reads the .docx three ways, and the three come here: `--track-changes=all` as JSON (who made each
//! insertion and deletion, the comments and the text they cover), and `--track-changes=reject` and `=accept` as plain
//! text (what Word had before the tracked changes, and after them).
//!
//! - The tracked changes are the word-level difference from the reject text to the accept text. Each hunk is placed
//!   in the Markdown by its quote (see `places`), and given to the author of the insertion or deletion of the JSON
//!   that has its words, else "Word".
//! - What was changed with tracking off is the difference from the Markdown to the reject text, both as plain text
//!   (`current_plain`, the Markdown rendered by pandoc). Those hunks are given to the author the importer names.
//! - A comment is placed by the words it covers, and is made as it was written.
//!
//! What cannot be placed (the words are not found, or are in two places that the words around do not tell apart) is
//! listed, not guessed.

use serde_json::Value;
use super::places::{CONTEXT, Index, words};
use crate::review::diff::{Hunk, hunks};
use crate::review::ops::NewRecord;
use crate::review::text::utf16_of;
use std::ops::Range;

/// The author of a tracked change that the document does not name.
pub const WORD: &str = "Word";

/// What a Word review comes to.
#[derive(Clone, Debug, Default)]
pub struct Imported {
    /// the author, and the suggestion (positions in UTF-16 units of the Markdown)
    pub suggestions: Vec<(String, NewRecord)>,
    pub comments: Vec<(String, NewRecord)>,
    /// what could not be placed, one line each
    pub unplaced: Vec<String>,
}

/// A tracked insertion or deletion of the JSON.
struct Tracked {
    insertion: bool,
    author: String,
    words: Vec<String>,
}

struct Comment {
    author: String,
    text: String,
    /// the words of the reject stream it covers
    from: usize,
    to: usize,
}

/// The JSON read: the words of the document as Word had it before the changes (what is outside insertions), the
/// tracked changes, the comments.
#[derive(Default)]
struct Stream {
    words: Vec<String>,
    tracked: Vec<Tracked>,
    comments: Vec<Comment>,
    /// comments begun, by id, not yet ended: the index in `comments`
    open: Vec<(String, usize)>,
}

#[derive(Clone, Copy)]
enum Mode {
    Normal,
    Insertion(usize),
    Deletion(usize),
}

fn attr<'a>(attrs: &'a Value, key: &str) -> Option<&'a str> {
    attrs.as_array().into_iter().flatten().find(|kv| kv.get(0).and_then(Value::as_str) == Some(key)).and_then(|kv| kv.get(1)?.as_str())
}

fn plain(v: &Value, out: &mut String) {
    match v {
        Value::Array(a) => a.iter().for_each(|x| plain(x, out)),
        Value::Object(_) => match v.get("t").and_then(Value::as_str) {
            Some("Str") => out.push_str(v.get("c").and_then(Value::as_str).unwrap_or("")),
            Some("Space" | "SoftBreak") => out.push(' '),
            Some("LineBreak") => out.push('\n'),
            Some("Para" | "Plain") => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push_str("\n\n");
                }
                if let Some(c) = v.get("c") {
                    plain(c, out);
                }
            }
            Some("Math" | "Code" | "CodeBlock") => out.push_str(v.get("c").and_then(|c| c.get(1)).and_then(Value::as_str).unwrap_or("")),
            _ => {
                if let Some(c) = v.get("c") {
                    plain(c, out);
                }
            }
        },
        _ => {}
    }
}

impl Stream {
    fn say(&mut self, text: &str, mode: Mode) {
        for r in words(text) {
            let w = text[r].to_string();
            match mode {
                Mode::Normal => self.words.push(w),
                Mode::Insertion(t) => self.tracked[t].words.push(w),
                Mode::Deletion(t) => {
                    self.tracked[t].words.push(w.clone());
                    self.words.push(w);
                }
            }
        }
    }

    fn walk(&mut self, v: &Value, mode: Mode) {
        match v {
            Value::Array(a) => a.iter().for_each(|x| self.walk(x, mode)),
            Value::Object(o) => {
                let Some(t) = v.get("t").and_then(Value::as_str) else {
                    // the document: its blocks
                    if let Some(blocks) = o.get("blocks") {
                        self.walk(blocks, mode);
                    }
                    return;
                };
                let c = v.get("c");
                match t {
                    "Str" => self.say(c.and_then(Value::as_str).unwrap_or(""), mode),
                    "Math" | "Code" | "CodeBlock" => self.say(c.and_then(|c| c.get(1)).and_then(Value::as_str).unwrap_or(""), mode),
                    "Span" | "Div" => {
                        let (Some(attrs), Some(content)) = (c.and_then(|c| c.get(0)), c.and_then(|c| c.get(1))) else { return };
                        let class = |name: &str| attrs.get(1).is_some_and(|cl| cl.as_array().into_iter().flatten().any(|x| x.as_str() == Some(name)));
                        let kvs = attrs.get(2).unwrap_or(&Value::Null);
                        let author = attr(kvs, "author").filter(|a| !a.trim().is_empty()).unwrap_or(WORD).to_string();
                        if class("comment-start") {
                            let mut text = String::new();
                            plain(content, &mut text);
                            let at = self.words.len();
                            self.comments.push(Comment { author, text: text.trim().to_string(), from: at, to: at });
                            let id = attr(kvs, "id").unwrap_or("").to_string();
                            self.open.push((id, self.comments.len() - 1));
                        } else if class("comment-end") {
                            let id = attr(kvs, "id").unwrap_or("");
                            if let Some(k) = self.open.iter().position(|(i, _)| i == id) {
                                let (_, n) = self.open.remove(k);
                                self.comments[n].to = self.words.len();
                            }
                        } else if class("insertion") || class("deletion") {
                            let insertion = class("insertion");
                            self.tracked.push(Tracked { insertion, author, words: Vec::new() });
                            let t = self.tracked.len() - 1;
                            self.walk(content, if insertion { Mode::Insertion(t) } else { Mode::Deletion(t) });
                        } else {
                            self.walk(content, mode);
                        }
                    }
                    _ => {
                        if let Some(c) = c {
                            self.walk(c, mode);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Runs of spaces and tabs are one space (Word and pandoc differ in how much white space they keep).
fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c == ' ' || c == '\t' || c == '\u{a0}' {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    out
}

fn short(s: &str) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 50 { format!("{}…", flat.chars().take(50).collect::<String>()) } else { flat }
}

fn say_change(old: &str, new: &str) -> String {
    match (old.trim().is_empty(), new.trim().is_empty()) {
        (true, _) => format!("insert “{}”", short(new)),
        (_, true) => format!("delete “{}”", short(old)),
        _ => format!("replace “{}” with “{}”", short(old), short(new)),
    }
}

fn pieces<'a>(text: &'a str, ws: &[Range<usize>]) -> Vec<&'a str> {
    ws.iter().map(|r| &text[r.clone()]).collect()
}

/// A change of `source` (a plain text: the Markdown rendered, or what Word had) into `target`, placed in the Markdown
/// by `index`; the suggestion, or what to say of it.
fn placed(index: &Index, source: &str, target: &str, hunk: &Hunk, floor: usize) -> Result<NewRecord, String> {
    let (mut old, mut new) = (hunk.old.clone(), hunk.new.clone());
    let src_words = words(source);
    // a change of punctuation or white space only: the word before it goes in with it, to be found by
    if old.start < old.end && !src_words.iter().any(|w| w.start >= old.start && w.end <= old.end) {
        let before = src_words.iter().rev().find(|w| w.end <= old.start && w.start >= floor).ok_or("no word to place it by")?;
        let back = old.start - before.start;
        new.start = new.start.checked_sub(back).ok_or("no word to place it by")?;
        old.start = before.start;
    }
    let inside: Vec<Range<usize>> = src_words.iter().filter(|w| w.start >= old.start && w.end <= old.end).cloned().collect();
    let before: Vec<Range<usize>> = src_words.iter().filter(|w| w.end <= old.start).cloned().collect();
    let after: Vec<Range<usize>> = src_words.iter().filter(|w| w.start >= old.end).cloned().collect();
    let prefix = pieces(source, &before[before.len().saturating_sub(CONTEXT)..]);
    let suffix = pieces(source, &after[..after.len().min(CONTEXT)]);
    let exact = pieces(source, &inside);
    let found = index.locate(&exact, &prefix, &suffix).ok_or("its place is not found in the text")?;
    let insert = target[new].to_string();
    let range = if found.0 == found.1 {
        let p = index.point(found.0, &insert);
        p..p
    } else {
        let mut r = index.bytes(found);
        let (old_text, text) = (&source[old.clone()], index.text);
        if old_text.starts_with(char::is_whitespace)
            && let Some(c) = text[..r.start].chars().next_back().filter(|c| c.is_whitespace())
        {
            r.start -= c.len_utf8();
        }
        if old_text.ends_with(char::is_whitespace)
            && let Some(c) = text[r.end..].chars().next().filter(|c| c.is_whitespace())
        {
            r.end += c.len_utf8();
        }
        r
    };
    Ok(NewRecord::suggestion(utf16_of(index.text, range.start), utf16_of(index.text, range.end), insert))
}

/// The author of the tracked change whose words a hunk has, taking it (and the others of that author that fit) out of
/// the account.
fn author_of(spans: &mut [(Tracked, bool)], old: &[&str], new: &[&str]) -> String {
    let score = |t: &Tracked| {
        let ws = if t.insertion { new } else { old };
        ws.iter().filter(|w| t.words.iter().any(|x| x == *w)).count()
    };
    let best = spans.iter().filter(|(_, used)| !used).map(|(t, _)| score(t)).max().filter(|s| *s > 0);
    let Some(best) = best else { return WORD.into() };
    let author = spans.iter().find(|(t, used)| !used && score(t) == best).map(|(t, _)| t.author.clone()).unwrap_or_default();
    for (t, used) in spans.iter_mut() {
        if !*used && t.author == author && score(t) > 0 {
            *used = true;
        }
    }
    author
}

/// The Word review in `docx_all_json`, `docx_reject_text` and `docx_accept_text` as suggestions and comments on
/// `current`. `current_plain` is `current` rendered to plain text by pandoc (the same way as the docx's text; empty:
/// changes made with tracking off are not looked for), `importer` the author of those.
pub fn import(current: &str, docx_all_json: &str, docx_reject_text: &str, docx_accept_text: &str, current_plain: &str, importer: &str) -> Imported {
    let mut out = Imported::default();
    let mut stream = Stream::default();
    match serde_json::from_str::<Value>(docx_all_json) {
        Ok(json) => stream.walk(&json, Mode::Normal),
        Err(e) => out.unplaced.push(format!("the document's own record of the changes could not be read ({e}): authors are \"{WORD}\"")),
    }
    let index = Index::new(current);
    let (reject, accept, plain_current) = (collapse(docx_reject_text), collapse(docx_accept_text), collapse(current_plain));

    // what was changed with tracking on
    let mut spans: Vec<(Tracked, bool)> = std::mem::take(&mut stream.tracked).into_iter().map(|t| (t, false)).collect();
    let mut floor = 0;
    for h in hunks(&reject, &accept) {
        let (old_words, new_words) = (words(&reject[h.old.clone()]), words(&accept[h.new.clone()]));
        let old_text: Vec<&str> = pieces(&reject[h.old.clone()], &old_words);
        let new_text: Vec<&str> = pieces(&accept[h.new.clone()], &new_words);
        let author = author_of(&mut spans, &old_text, &new_text);
        match placed(&index, &reject, &accept, &h, floor) {
            Ok(s) => out.suggestions.push((author, s)),
            Err(why) => out.unplaced.push(format!("{author}: {} ({why})", say_change(&reject[h.old.clone()], &accept[h.new.clone()]))),
        }
        floor = h.old.end;
    }

    // what was changed with tracking off
    if !plain_current.trim().is_empty() {
        let who = if importer.trim().is_empty() { WORD } else { importer.trim() };
        let mut floor = 0;
        for h in hunks(&plain_current, &reject) {
            let after_previous = std::mem::replace(&mut floor, h.old.end);
            // punctuation and white space alone: what pandoc's plain text of the two differs in
            if words(&plain_current[h.old.clone()]).is_empty() && words(&reject[h.new.clone()]).is_empty() {
                continue;
            }
            match placed(&index, &plain_current, &reject, &h, after_previous) {
                Ok(s) => out.suggestions.push((who.to_string(), s)),
                Err(why) => out.unplaced.push(format!("{who}: {} ({why}; changed with tracking off)", say_change(&plain_current[h.old.clone()], &reject[h.new.clone()]))),
            }
        }
    }

    // the comments
    for c in &stream.comments {
        let (to, from) = (c.to.max(c.from), c.from);
        let at = |i: usize| stream.words[i].as_str();
        let exact: Vec<&str> = (from..to).map(at).collect();
        let prefix: Vec<&str> = (from.saturating_sub(CONTEXT)..from).map(at).collect();
        let suffix: Vec<&str> = (to..(to + CONTEXT).min(stream.words.len())).map(at).collect();
        match index.locate(&exact, &prefix, &suffix) {
            Some(span) => {
                let range = if span.0 == span.1 {
                    let p = index.point(span.0, "");
                    p..p
                } else {
                    index.bytes(span)
                };
                let record = NewRecord::comment(utf16_of(current, range.start), utf16_of(current, range.end), c.text.clone());
                out.comments.push((c.author.clone(), record));
            }
            None => out.unplaced.push(format!("{}: comment “{}” on “{}” (its place is not found in the text)", c.author, short(&c.text), short(&exact.join(" ")))),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::export::markdown_with_changes;
    use super::super::testing::{docx, docx_cited, pandoc, plain, plain_cited, read};
    use super::*;
    use crate::review::fold::Item;
    use crate::review::ops::NewRecord;
    use crate::review::record::Person;
    use crate::review::testutil::doc;
    use crate::review::{Review, byte_of};

    fn dir(name: &str) -> std::path::PathBuf {
        let d = doc("x.md").parent().unwrap().join(name);
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// The text of `current` a suggestion or comment stands on.
    fn on<'a>(current: &'a str, r: &NewRecord) -> &'a str {
        &current[byte_of(current, r.from)..byte_of(current, r.to)]
    }

    fn apply(current: &str, list: &[(String, NewRecord)]) -> String {
        let mut sorted: Vec<&NewRecord> = list.iter().map(|(_, r)| r).collect();
        sorted.sort_by_key(|r| std::cmp::Reverse((r.from, r.to)));
        let mut out = current.to_string();
        for r in sorted {
            out.replace_range(byte_of(current, r.from)..byte_of(current, r.to), r.insert.as_deref().unwrap_or(""));
        }
        out
    }

    const CURRENT: &str = "The *grounding line* retreats rapidly [@smith2020] when the bed slopes.\n\nThe ice shelf thins and the ice sheet stays, as $x^2$ shows.\n";

    #[test]
    fn a_word_docx_comes_back_at_the_right_places() {
        if pandoc().is_none() {
            return;
        }
        let tracked = r#"The *grounding line* retreats [rapidly]{.deletion author="Jane Doe" date="2026-11-05T14:00:00Z"}[quickly]{.insertion author="Jane Doe" date="2026-11-05T14:00:00Z"} [@smith2020] when the bed slopes.

The ice shelf [thins]{.deletion author="Bob" date="2026-11-05T14:00:00Z"}[thickens]{.insertion author="Bob" date="2026-11-05T14:00:00Z"} and the [Cite X here?]{.comment-start id="1" author="Carol" date="2026-11-05T14:00:00Z"}ice sheet[]{.comment-end id="1"} stays, as $x^2$ shows.
"#;
        let file = docx(tracked, &dir("a"));
        let (json, reject, accept) = read(&file);
        let got = import(CURRENT, &json, &reject, &accept, &plain(CURRENT), "Me");
        assert!(got.unplaced.is_empty(), "{:?}", got.unplaced);
        assert_eq!(got.suggestions.len(), 2, "{:?}", got.suggestions);
        let (author, s) = &got.suggestions[0];
        assert_eq!((author.as_str(), on(CURRENT, s), s.insert.as_deref()), ("Jane Doe", "rapidly", Some("quickly")));
        let (author, s) = &got.suggestions[1];
        assert_eq!((author.as_str(), on(CURRENT, s), s.insert.as_deref()), ("Bob", "thins", Some("thickens")));
        assert_eq!(got.comments.len(), 1);
        let (author, c) = &got.comments[0];
        assert_eq!((author.as_str(), on(CURRENT, c), c.text.as_deref()), ("Carol", "ice sheet", Some("Cite X here?")));
    }

    #[test]
    fn an_insertion_and_a_deletion_are_placed_by_the_words_around() {
        if pandoc().is_none() {
            return;
        }
        let tracked = r#"The *grounding line* retreats rapidly [@smith2020] when the bed [very]{.insertion author="Jane Doe"} slopes.

The ice shelf thins [and the ice sheet]{.deletion author="Bob"} stays, as $x^2$ shows.
"#;
        let file = docx(tracked, &dir("b"));
        let (json, reject, accept) = read(&file);
        let got = import(CURRENT, &json, &reject, &accept, &plain(CURRENT), "Me");
        assert!(got.unplaced.is_empty(), "{:?}", got.unplaced);
        let authors: Vec<&str> = got.suggestions.iter().map(|(a, _)| a.as_str()).collect();
        assert_eq!(authors, ["Jane Doe", "Bob"]);
        let applied = apply(CURRENT, &got.suggestions);
        let words = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(words(&applied).contains("when the bed very slopes."), "{applied}");
        assert!(words(&applied).contains("thins stays, as $x^2$"), "{applied}");
    }

    #[test]
    fn changes_made_with_tracking_off_are_the_importers() {
        if pandoc().is_none() {
            return;
        }
        // no spans: Word with Track Changes off
        let edited = CURRENT.replace("rapidly", "slowly");
        let file = docx(&edited, &dir("c"));
        let (json, reject, accept) = read(&file);
        assert_eq!(reject, accept);
        let got = import(CURRENT, &json, &reject, &accept, &plain(CURRENT), "Dana");
        assert!(got.unplaced.is_empty(), "{:?}", got.unplaced);
        assert_eq!(got.suggestions.len(), 1, "{:?}", got.suggestions);
        let (author, s) = &got.suggestions[0];
        assert_eq!((author.as_str(), on(CURRENT, s), s.insert.as_deref()), ("Dana", "rapidly", Some("slowly")));
    }

    #[test]
    fn what_cannot_be_placed_is_listed() {
        if pandoc().is_none() {
            return;
        }
        let tracked = r#"A different text altogether, where [one]{.deletion author="Bob"}[two]{.insertion author="Bob"} word changes.
"#;
        let file = docx(tracked, &dir("d"));
        let (json, reject, accept) = read(&file);
        let got = import(CURRENT, &json, &reject, &accept, "", "Me");
        assert!(got.suggestions.is_empty());
        assert_eq!(got.unplaced.len(), 1);
        assert!(got.unplaced[0].starts_with("Bob: replace “one” with “two”"), "{:?}", got.unplaced);
    }

    /// The items of a review of `text`: suggestions by Jane at the needles (the quarry: needle, replacement) and a
    /// comment by Bob.
    fn review(text: &str, suggest: &[(&str, &str)], comment: &[(&str, &str)]) -> Vec<Item> {
        let d = doc("paper.md");
        let at = |needle: &str| {
            let s = text.find(needle).unwrap();
            (crate::review::utf16_of(text, s), crate::review::utf16_of(text, s + needle.len()))
        };
        for (n, r) in suggest {
            let (a, b) = at(n);
            Review::add(&d, &Person::new("Jane Doe", ""), text, NewRecord::suggestion(a, b, *r)).unwrap();
        }
        for (n, c) in comment {
            let (a, b) = at(n);
            Review::add(&d, &Person::new("Bob", ""), text, NewRecord::comment(a, b, *c)).unwrap();
        }
        Review::load(&d).unwrap().items(text)
    }

    #[test]
    fn the_round_trip_gives_the_same_suggestions() {
        if pandoc().is_none() {
            return;
        }
        let suggest = [("rapidly", "quickly"), ("bed slopes", "bed slopes inland"), ("The ice shelf thins", "The shelf thins"), ("stays", "")];
        let items = review(CURRENT, &suggest, &[("grounding line", "Cite X here?")]);
        let made = markdown_with_changes(CURRENT, &items);
        assert!(made.skipped.is_empty(), "{:?}", made.skipped);
        let file = docx(&made.markdown, &dir("e"));
        let (json, reject, accept) = read(&file);
        let got = import(CURRENT, &json, &reject, &accept, &plain(CURRENT), "Importer");
        assert!(got.unplaced.is_empty(), "{:?}", got.unplaced);
        assert!(got.suggestions.iter().all(|(a, _)| a == "Jane Doe"), "nothing untracked: {:?}", got.suggestions);
        // the same text comes of them
        let words = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        let wanted: Vec<(String, NewRecord)> = items
            .iter()
            .filter(|i| i.kind == crate::review::Kind::Suggestion)
            .map(|i| ("Jane Doe".to_string(), NewRecord::suggestion(i.at.from, i.at.to, i.replacement.clone().unwrap_or_default())))
            .collect();
        assert_eq!(words(&apply(CURRENT, &got.suggestions)), words(&apply(CURRENT, &wanted)));
        assert_eq!(got.comments.len(), 1);
        let (author, c) = &got.comments[0];
        assert_eq!((author.as_str(), on(CURRENT, c), c.text.as_deref()), ("Bob", "grounding line", Some("Cite X here?")));
    }

    #[test]
    fn citations_rendered_by_citeproc_do_not_stand_in_the_way() {
        if pandoc().is_none() {
            return;
        }
        let d = dir("f");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("refs.bib"), "@article{smith2020, author={Smith, Jane}, title={Ice}, journal={J. Ice}, year={2020}}\n").unwrap();
        let current = "---\nbibliography: refs.bib\n---\n\nThe *grounding line* retreats rapidly [@smith2020] when the *bed* slopes, as shown before [@smith2020, p. 3] in the ice.\n";
        let suggest = [("rapidly", "quickly"), ("in the ice", "in the ice sheet")];
        let items = review(current, &suggest, &[("when the *bed* slopes", "Why?")]);
        let made = markdown_with_changes(current, &items);
        let file = docx_cited(&made.markdown, &d);
        let (json, reject, accept) = read(&file);
        assert!(reject.contains("Smith"), "citeproc ran: {reject}");
        let got = import(current, &json, &reject, &accept, &plain_cited(current, &d), "Importer");
        assert!(got.unplaced.is_empty(), "{:?}", got.unplaced);
        assert_eq!(got.suggestions.len(), 2, "{:?}", got.suggestions);
        assert_eq!(on(current, &got.suggestions[0].1), "rapidly");
        assert_eq!(on(current, &got.comments[0].1), "when the *bed* slopes");
    }
}
