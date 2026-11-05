//! What a span cannot be put inside when the Markdown goes to pandoc: the front matter, code, math, raw LaTeX and
//! HTML comments. The scan is that of pandoc's Markdown reader, near enough: it errs on the side of protecting.

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    FrontMatter,
    Code,
    Math,
    Latex,
    Comment,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::FrontMatter => "the front matter",
            Kind::Code => "code",
            Kind::Math => "math",
            Kind::Latex => "raw LaTeX",
            Kind::Comment => "an HTML comment",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    pub range: Range<usize>,
    pub kind: Kind,
    /// it stands alone in its lines (a fenced block, display math, an environment): a note about it is a paragraph of
    /// its own before it; else the note goes just after it
    pub block: bool,
}

impl Region {
    /// Whether a suggestion or comment on `s..e` (`s == e`: a point) touches this region.
    pub fn touches(&self, s: usize, e: usize) -> bool {
        if s == e { self.range.start < s && s < self.range.end } else { s < self.range.end && self.range.start < e }
    }
}

fn line_end(text: &str, from: usize) -> usize {
    text[from..].find('\n').map_or(text.len(), |n| from + n)
}

/// The end of the fenced block that opens on the line starting at `i`, if one does (the closing line's end, else the
/// end of the text).
fn fence(text: &str, i: usize) -> Option<usize> {
    let end = line_end(text, i);
    let line = text[i..end].trim_start_matches(' ');
    if text[i..end].len() - line.len() > 3 {
        return None;
    }
    let c = line.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = line.chars().take_while(|x| *x == c).count();
    if n < 3 || (c == '`' && line[n..].contains('`')) {
        return None;
    }
    let mut at = end;
    while at < text.len() {
        let start = at + 1;
        let stop = line_end(text, start);
        let l = text[start..stop].trim_start_matches(' ');
        if l.starts_with(c) && l.chars().take_while(|x| *x == c).count() >= n && l.trim_start_matches(c).trim().is_empty() {
            return Some(stop);
        }
        at = stop;
    }
    Some(text.len())
}

fn front_matter(text: &str) -> Option<usize> {
    if text.lines().next()?.trim_end() != "---" {
        return None;
    }
    let mut at = line_end(text, 0);
    while at < text.len() {
        let start = at + 1;
        let stop = line_end(text, start);
        if matches!(text[start..stop].trim_end(), "---" | "...") {
            return Some(stop);
        }
        at = stop;
    }
    None
}

/// The end of the balanced `open ... close` group that starts at `i` (`text[i]` is `open`).
fn group(text: &str, i: usize, open: u8, close: u8) -> Option<usize> {
    let b = text.as_bytes();
    let mut depth = 0;
    let mut j = i;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 1,
            c if c == open => depth += 1,
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(j + 1);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// The regions of `text`, in order.
pub fn regions(text: &str) -> Vec<Region> {
    let b = text.as_bytes();
    let mut out: Vec<Region> = Vec::new();
    let mut i = 0;
    if let Some(end) = front_matter(text) {
        out.push(Region { range: 0..end, kind: Kind::FrontMatter, block: true });
        i = end;
    }
    let at_line_start = |i: usize| i == 0 || b[i - 1] == b'\n';
    let push = |out: &mut Vec<Region>, start: usize, end: usize, kind: Kind, display: bool| {
        let block = at_line_start(start) && (display || kind == Kind::Code) || (at_line_start(start) && (end >= b.len() || b[end] == b'\n'));
        out.push(Region { range: start..end, kind, block });
    };
    while i < b.len() {
        if at_line_start(i)
            && let Some(end) = fence(text, i)
        {
            push(&mut out, i, end, Kind::Code, true);
            i = end;
            continue;
        }
        match b[i] {
            b'`' => {
                let n = b[i..].iter().take_while(|c| **c == b'`').count();
                let ticks = "`".repeat(n);
                let found = text[i + n..].match_indices(ticks.as_str()).map(|(k, _)| i + n + k).find(|k| b.get(k + n) != Some(&b'`') && b[k - 1] != b'`');
                match found {
                    Some(k) => {
                        push(&mut out, i, k + n, Kind::Code, false);
                        i = k + n;
                    }
                    None => i += n,
                }
            }
            b'$' => {
                if b.get(i + 1) == Some(&b'$') {
                    match text[i + 2..].find("$$") {
                        Some(k) => {
                            push(&mut out, i, i + 2 + k + 2, Kind::Math, true);
                            i += 2 + k + 2;
                        }
                        None => i += 2,
                    }
                } else if text[i + 1..].chars().next().is_some_and(|c| !c.is_whitespace()) {
                    // the closing $: after a non-space, not before a digit, in the same paragraph
                    let mut j = i + 1;
                    let mut end = None;
                    while j < b.len() {
                        match b[j] {
                            b'\\' => j += 1,
                            b'\n' if b.get(j + 1) == Some(&b'\n') => break,
                            b'$' if !b[j - 1].is_ascii_whitespace() && !b.get(j + 1).is_some_and(u8::is_ascii_digit) => {
                                end = Some(j + 1);
                                break;
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                    match end {
                        Some(end) => {
                            push(&mut out, i, end, Kind::Math, false);
                            i = end;
                        }
                        None => i += 1,
                    }
                } else {
                    i += 1;
                }
            }
            b'<' if text[i..].starts_with("<!--") => {
                let end = text[i..].find("-->").map_or(text.len(), |k| i + k + 3);
                push(&mut out, i, end, Kind::Comment, false);
                i = end;
            }
            b'\\' => {
                let next = b.get(i + 1).copied().unwrap_or(0);
                if next == b'[' || next == b'(' {
                    let close = if next == b'[' { "\\]" } else { "\\)" };
                    match text[i + 2..].find(close) {
                        Some(k) => {
                            push(&mut out, i, i + 2 + k + 2, Kind::Math, next == b'[');
                            i += 2 + k + 2;
                        }
                        None => i += 2,
                    }
                } else if next.is_ascii_alphabetic() {
                    let mut j = i + 1;
                    while j < b.len() && b[j].is_ascii_alphabetic() {
                        j += 1;
                    }
                    let name = &text[i + 1..j];
                    if name == "begin"
                        && b.get(j) == Some(&b'{')
                        && let Some(close) = group(text, j, b'{', b'}')
                    {
                        let env = &text[j..close];
                        let end_tag = format!("\\end{env}");
                        if let Some(k) = text[close..].find(&end_tag) {
                            let end = close + k + end_tag.len();
                            push(&mut out, i, end, Kind::Latex, true);
                            i = end;
                            continue;
                        }
                    }
                    // a command and the groups it takes
                    loop {
                        let (open, shut) = match b.get(j) {
                            Some(b'{') => (b'{', b'}'),
                            Some(b'[') => (b'[', b']'),
                            _ => break,
                        };
                        match group(text, j, open, shut) {
                            Some(end) => j = end,
                            None => break,
                        }
                    }
                    push(&mut out, i, j, Kind::Latex, false);
                    i = j;
                } else {
                    i += 2;
                }
            }
            _ => i += 1,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<(&str, Kind, bool)> {
        regions(text).into_iter().map(|r| (&text[r.range], r.kind, r.block)).collect()
    }

    #[test]
    fn math_code_and_latex() {
        let t = "A $x^2$ and `code` and \\cite{a}[p. 2] cost $5 and $6.\n\n$$E=mc^2$$\n\n```\nint x;\n```\n";
        let r = found(t);
        assert_eq!(r[0], ("$x^2$", Kind::Math, false));
        assert_eq!(r[1], ("`code`", Kind::Code, false));
        assert_eq!(r[2], ("\\cite{a}[p. 2]", Kind::Latex, false));
        // "$5 and $6": the closing dollar is before a digit: not math
        assert_eq!(r[3], ("$$E=mc^2$$", Kind::Math, true));
        assert_eq!(r[4], ("```\nint x;\n```", Kind::Code, true));
        assert_eq!(r.len(), 5);
    }

    #[test]
    fn environments_front_matter_and_comments() {
        let t = "---\ntitle: T\n---\n\nText <!-- a note -->\n\n\\begin{align}\na&=b\n\\end{align}\n";
        let r = found(t);
        assert_eq!(r[0].1, Kind::FrontMatter);
        assert_eq!(r[1].0, "<!-- a note -->");
        assert_eq!(r[2], ("\\begin{align}\na&=b\n\\end{align}", Kind::Latex, true));
    }

    #[test]
    fn touching() {
        let t = "ab $x y$ cd";
        let r = &regions(t)[0];
        assert!(r.touches(4, 5));
        assert!(!r.touches(0, 3), "ends where the math starts");
        assert!(!r.touches(3, 3), "a point at its start");
        assert!(r.touches(5, 5));
        assert!(!r.touches(8, 11));
    }
}
