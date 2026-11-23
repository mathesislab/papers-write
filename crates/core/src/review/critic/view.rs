//! The CriticMarkup of a text as the preview shows it ([`Shown`]): the marks drawn as changes, or the text with every
//! suggestion accepted, or with every one rejected (the text before them).
//!
//! Drawn, a mark is an HTML `<span class="critic-…">` around its text, which stays Markdown (citations, emphasis and
//! math inside it are still read): `critic-ins`, `critic-del` (a substitution is both), `critic-mark` (a highlight)
//! and `critic-note` (a comment). A span is a line at most: a mark over several lines is a span on each, the marker
//! of a heading, a list item or a quote at the start of a line left before it, so the blocks stay what they are.
//! Code (fenced blocks, inline code) is left as it is, and so is a mark that is not closed.

use serde::{Deserialize, Serialize};

/// How the marks are shown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shown {
    /// drawn as changes
    #[default]
    Marked,
    /// every suggestion accepted; highlights and comments left out
    Accepted,
    /// every suggestion rejected; highlights and comments left out
    Original,
}

const MARKS: [(&str, &str); 5] = [("{++", "++}"), ("{--", "--}"), ("{~~", "~~}"), ("{==", "==}"), ("{>>", "<<}")];

/// `text` with its CriticMarkup shown as `how` says.
pub fn shown(text: &str, how: Shown) -> String {
    if !text.contains('{') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    let mut fence: Option<String> = None;
    let mut i = 0;
    while i < text.len() {
        let line_start = i == 0 || text.as_bytes()[i - 1] == b'\n';
        if line_start {
            // a fenced block: its lines as they are
            let line_end = text[i..].find('\n').map_or(text.len(), |n| i + n + 1);
            let line = &text[i..line_end];
            let t = line.trim_start();
            if let Some(f) = &fence {
                if t.starts_with(f.as_str()) && t.trim_start_matches(f.chars().next().unwrap()).trim().is_empty() {
                    fence = None;
                }
                out.push_str(line);
                i = line_end;
                continue;
            }
            if line.len() - t.len() <= 3 && (t.starts_with("```") || t.starts_with("~~~")) {
                let c = t.chars().next().unwrap();
                fence = Some(t.chars().take_while(|&x| x == c).collect());
                out.push_str(line);
                i = line_end;
                continue;
            }
        }
        let c = text[i..].chars().next().unwrap();
        if c == '`' {
            // inline code: up to the same run of backticks
            let run = text[i..].len() - text[i..].trim_start_matches('`').len();
            let after = i + run;
            let ticks = &text[i..after];
            let end = text[after..].find(ticks).map(|n| after + n + run);
            let end = end.unwrap_or(after);
            out.push_str(&text[i..end]);
            i = end;
            continue;
        }
        if c == '{'
            && let Some(&(open, close)) = MARKS.iter().find(|(o, _)| text[i..].starts_with(o))
            && let Some(len) = text[i + open.len()..].find(close)
        {
            let body = &text[i + open.len()..i + open.len() + len];
            let at_line_start = line_start;
            let end = i + open.len() + len + close.len();
            match (open, how) {
                ("{++", Shown::Marked) => wrap(&mut out, body, "critic-ins", at_line_start),
                ("{++", Shown::Accepted) | ("{--", Shown::Original) | ("{==", Shown::Accepted | Shown::Original) => {
                    out.push_str(body)
                }
                ("{--", Shown::Marked) => wrap(&mut out, body, "critic-del", at_line_start),
                ("{~~", _) => match body.split_once("~>") {
                    None => out.push_str(&text[i..end]),
                    Some((old, new)) => match how {
                        Shown::Marked => {
                            wrap(&mut out, old, "critic-del", at_line_start);
                            wrap(&mut out, new, "critic-ins", false);
                        }
                        Shown::Accepted => out.push_str(new),
                        Shown::Original => out.push_str(old),
                    },
                },
                ("{==", Shown::Marked) => wrap(&mut out, body, "critic-mark", at_line_start),
                // a comment's `@` is no citation
                ("{>>", Shown::Marked) => wrap(&mut out, &body.trim().replace('@', "\\@"), "critic-note", at_line_start),
                _ => {}
            }
            i = end;
            continue;
        }
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// `body` in spans of `class`, one a line; `at_line_start`: its first line begins a line of the text.
fn wrap(out: &mut String, body: &str, class: &str, at_line_start: bool) {
    for (n, line) in body.split('\n').enumerate() {
        if n > 0 {
            out.push('\n');
        }
        let (marker, rest) = if n > 0 || at_line_start { split_marker(line) } else { ("", line) };
        out.push_str(marker);
        if !rest.trim().is_empty() {
            out.push_str("<span class=\"");
            out.push_str(class);
            out.push_str("\">");
            out.push_str(rest);
            out.push_str("</span>");
        } else {
            out.push_str(rest);
        }
    }
}

/// The marker of a block at the start of `line` (`#`s, `>`, a bullet or a number, with the spaces around), and the rest.
fn split_marker(line: &str) -> (&str, &str) {
    let b = line.as_bytes();
    let spaces = |k: usize| k + b[k..].iter().take_while(|c| matches!(c, b' ' | b'\t')).count();
    // the end of the markers found so far (their spaces with them)
    let mut taken = 0;
    loop {
        let k = spaces(taken);
        let next = match b.get(k) {
            Some(b'>') => k + 1,
            Some(b'#') => k + b[k..].iter().take_while(|c| **c == b'#').count(),
            Some(b'-' | b'*' | b'+') if b.get(k + 1) == Some(&b' ') => k + 1,
            Some(c) if c.is_ascii_digit() => {
                let d = b[k..].iter().take_while(|c| c.is_ascii_digit()).count();
                if matches!(b.get(k + d), Some(b'.' | b')')) && b.get(k + d + 1) == Some(&b' ') { k + d + 1 } else { k }
            }
            _ => k,
        };
        if next == k {
            return line.split_at(taken);
        }
        taken = spaces(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_drawn_as_spans() {
        let t = "A {++new [@x]++} and {--old--} and {~~a~>b~~} {==hi==}{>>@MP: why<<}.";
        assert_eq!(
            shown(t, Shown::Marked),
            "A <span class=\"critic-ins\">new [@x]</span> and <span class=\"critic-del\">old</span> and \
             <span class=\"critic-del\">a</span><span class=\"critic-ins\">b</span> \
             <span class=\"critic-mark\">hi</span><span class=\"critic-note\">\\@MP: why</span>."
        );
    }

    #[test]
    fn accepted_and_original() {
        let t = "A {++new ++}text {--gone --}{~~a~>b~~} {==hi==}{>>note<<}.";
        assert_eq!(shown(t, Shown::Accepted), "A new text b hi.");
        assert_eq!(shown(t, Shown::Original), "A text gone a hi.");
    }

    #[test]
    fn a_span_a_line_markers_outside() {
        let t = "{++# Title\n\n- one\n- two\n\nPara\nnext++}";
        assert_eq!(
            shown(t, Shown::Marked),
            "# <span class=\"critic-ins\">Title</span>\n\n- <span class=\"critic-ins\">one</span>\n\
             - <span class=\"critic-ins\">two</span>\n\n<span class=\"critic-ins\">Para</span>\n\
             <span class=\"critic-ins\">next</span>"
        );
        // mid-line, a `#` is text
        assert_eq!(shown("a {++# b++}", Shown::Marked), "a <span class=\"critic-ins\"># b</span>");
    }

    #[test]
    fn code_and_unclosed_left() {
        let t = "`{++x++}` and\n```\n{--y--}\n```\n{++open";
        assert_eq!(shown(t, Shown::Marked), t);
        assert_eq!(shown("{~~no arrow~~}", Shown::Accepted), "{~~no arrow~~}");
    }
}
