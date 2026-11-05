//! The YAML front matter of a Markdown text (`---` … `---` or `...` at its very start): only its
//! `bibliography` field is read, by a small line parser, and added without touching the rest of the text.

use std::ops::Range;

/// A front matter: the byte ranges of the lines between its fences, and where the closing fence's line starts.
struct Block {
    /// the lines between the fences
    body: Range<usize>,
    /// the start of the closing fence's line
    close: usize,
}

/// Lines of `text` with their byte offsets, the line ending left out.
fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut at = 0;
    text.split_inclusive('\n').map(move |l| {
        let start = at;
        at += l.len();
        (start, l.strip_suffix('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).unwrap_or(l))
    })
}

fn find(text: &str) -> Option<Block> {
    let mut it = lines(text);
    let (_, first) = it.next()?;
    // a byte order mark is allowed before the fence
    if first.trim_start_matches('\u{feff}').trim_end() != "---" {
        return None;
    }
    let body_start = text.find('\n')? + 1;
    for (start, line) in it {
        let t = line.trim_end();
        if t == "---" || t == "..." {
            return Some(Block { body: body_start..start, close: start });
        }
    }
    None
}

/// Where the text after the front matter starts (0 without one).
pub(crate) fn body_start(text: &str) -> usize {
    find(text).map_or(0, |b| text[b.close..].find('\n').map_or(text.len(), |n| b.close + n + 1))
}

/// The names in the `bibliography` field of the front matter of `text`: a string, or a list (`[a.bib, b.bib]`
/// or `- a.bib` lines), quotes taken off. Empty when there is no front matter or no such field.
pub fn bibliography(text: &str) -> Vec<String> {
    let Some(block) = find(text) else {
        return Vec::new();
    };
    let body: Vec<&str> = lines(&text[block.body]).map(|(_, l)| l).collect();
    let Some(at) = body.iter().position(|l| field_value(l).is_some()) else {
        return Vec::new();
    };
    let value = field_value(body[at]).unwrap().trim();
    if let Some(flow) = value.strip_prefix('[') {
        // a flow list, over several lines perhaps
        let mut joined = flow.to_string();
        let mut next = at + 1;
        while !joined.contains(']') && next < body.len() {
            joined.push(' ');
            joined.push_str(body[next]);
            next += 1;
        }
        let inner = joined.split(']').next().unwrap_or("");
        return split_flow(inner).into_iter().map(|s| scalar(&s)).filter(|s| !s.is_empty()).collect();
    }
    if !strip_comment(value).trim().is_empty() {
        let one = scalar(value);
        return if one.is_empty() { Vec::new() } else { vec![one] };
    }
    // a block list: the `- item` lines after the field
    let mut names = Vec::new();
    for line in &body[at + 1..] {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        match t.strip_prefix('-') {
            Some(item) if item.is_empty() || item.starts_with(' ') || item.starts_with('\t') => {
                let name = scalar(item);
                if !name.is_empty() {
                    names.push(name);
                }
            }
            _ => break,
        }
    }
    names
}

/// The value after `bibliography:` of a top-level line, if it is that field's line.
fn field_value(line: &str) -> Option<&str> {
    line.strip_prefix("bibliography:")
        .filter(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
}

/// `text` without a trailing ` # comment` (not inside quotes).
fn strip_comment(text: &str) -> &str {
    let t = text.trim_start();
    if t.starts_with(['"', '\'']) {
        return text;
    }
    let mut prev_space = true;
    for (i, c) in text.char_indices() {
        if c == '#' && prev_space {
            return &text[..i];
        }
        prev_space = c == ' ' || c == '\t';
    }
    text
}

/// A YAML scalar read as a string: quotes off (with their escapes), a comment off.
fn scalar(text: &str) -> String {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(o) => out.push(o),
                    None => {}
                },
                c => out.push(c),
            }
        }
        return out;
    }
    if let Some(rest) = t.strip_prefix('\'') {
        let mut out = String::new();
        let mut chars = rest.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    out.push('\'');
                } else {
                    break;
                }
            } else {
                out.push(c);
            }
        }
        return out;
    }
    strip_comment(t).trim().to_string()
}

/// The items of a flow list's inside, split at commas outside quotes.
fn split_flow(inner: &str) -> Vec<String> {
    let (mut items, mut cur, mut quote) = (Vec::new(), String::new(), None::<char>);
    for c in inner.chars() {
        match (quote, c) {
            (None, ',') => items.push(std::mem::take(&mut cur)),
            (None, '"' | '\'') => {
                quote = Some(c);
                cur.push(c);
            }
            (Some(q), c) if c == q => {
                quote = None;
                cur.push(c);
            }
            _ => cur.push(c),
        }
    }
    items.push(cur);
    items
}

/// `name` as a YAML value: quoted when a plain one would be read as something else.
fn yaml_value(name: &str) -> String {
    let special = name.is_empty()
        || name.trim() != name
        || name.starts_with(['-', '?', '!', '&', '*', '|', '>', '%', '@', '`'])
        || name.contains([':', '#', '[', ']', '{', '}', ',', '"', '\'', '\\']);
    if special {
        format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        name.to_string()
    }
}

/// `text` with `bibliography: <name>` added: after the last field of its front matter, or in a new front
/// matter at its top. The rest is as it was, byte for byte. A text that already has the field is returned as it is.
pub fn with_bibliography(text: &str, name: &str) -> String {
    let line = format!("bibliography: {}", yaml_value(name));
    let Some(block) = find(text) else {
        let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
        return format!("---{eol}{line}{eol}---{eol}{eol}{text}");
    };
    if text[block.body.clone()].lines().any(|l| field_value(l).is_some()) {
        return text.to_string();
    }
    let eol = if text[..block.close].ends_with("\r\n") { "\r\n" } else { "\n" };
    // after the last line of the body that is not blank
    let mut at = block.body.start;
    for (start, l) in lines(&text[block.body.clone()]) {
        if !l.trim().is_empty() {
            at = block.body.start + start + text[block.body.start + start..].find('\n').map_or(l.len(), |n| n + 1);
        }
    }
    let mut out = String::with_capacity(text.len() + line.len() + 2);
    out.push_str(&text[..at]);
    out.push_str(&line);
    out.push_str(eol);
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_front_matter_no_bibliography() {
        assert!(bibliography("").is_empty());
        assert!(bibliography("# Title\n\nbibliography: x.bib\n").is_empty());
        assert!(bibliography("---\ntitle: x\n").is_empty());
        assert!(bibliography("---\ntitle: x\n---\n").is_empty());
    }

    #[test]
    fn a_string() {
        assert_eq!(bibliography("---\ntitle: T\nbibliography: refs.bib\n---\ntext"), v(&["refs.bib"]));
        assert_eq!(bibliography("---\nbibliography: refs.bib   # mine\n---\n"), v(&["refs.bib"]));
    }

    #[test]
    fn quotes() {
        assert_eq!(bibliography("---\nbibliography: \"my refs.bib\"\n---\n"), v(&["my refs.bib"]));
        assert_eq!(bibliography("---\nbibliography: 'it''s # x.bib'\n---\n"), v(&["it's # x.bib"]));
        assert_eq!(bibliography("---\nbibliography: \"a \\\"b\\\".bib\"\n---\n"), v(&["a \"b\".bib"]));
    }

    #[test]
    fn a_flow_list() {
        assert_eq!(bibliography("---\nbibliography: [a.bib, b.bib]\n---\n"), v(&["a.bib", "b.bib"]));
        assert_eq!(bibliography("---\nbibliography: [\"a, x.bib\", 'b.bib']\n---\n"), v(&["a, x.bib", "b.bib"]));
        assert_eq!(bibliography("---\nbibliography: [a.bib,\n   b.bib]\ntitle: T\n---\n"), v(&["a.bib", "b.bib"]));
    }

    #[test]
    fn a_block_list() {
        let text = "---\nbibliography:\n  - a.bib\n  - \"b.bib\"\n- c.bib\ntitle: T\n---\n";
        assert_eq!(bibliography(text), v(&["a.bib", "b.bib", "c.bib"]));
        assert_eq!(bibliography("---\nbibliography:\n- a.bib\n---\n"), v(&["a.bib"]));
        assert!(bibliography("---\nbibliography:\ntitle: T\n---\n").is_empty());
    }

    #[test]
    fn the_end_may_be_dots() {
        assert_eq!(bibliography("---\nbibliography: r.bib\n...\ntext\n"), v(&["r.bib"]));
    }

    #[test]
    fn a_fence_later_in_the_text_is_not_a_front_matter() {
        assert!(bibliography("Intro\n\n---\nbibliography: r.bib\n---\n").is_empty());
        assert!(bibliography("\n---\nbibliography: r.bib\n---\n").is_empty());
        // and a field name only alike
        assert!(bibliography("---\nbibliography_style: r.bib\nxbibliography: a\n---\n").is_empty());
    }

    #[test]
    fn crlf() {
        assert_eq!(bibliography("---\r\ntitle: T\r\nbibliography: r.bib\r\n---\r\ntext\r\n"), v(&["r.bib"]));
        assert_eq!(bibliography("---\r\nbibliography: [a.bib, b.bib]\r\n...\r\n"), v(&["a.bib", "b.bib"]));
    }

    #[test]
    fn added_after_the_last_field() {
        let text = "---\ntitle: T\nauthor:\n  - A\n\n---\n\n# Intro\n";
        assert_eq!(
            with_bibliography(text, "references.bib"),
            "---\ntitle: T\nauthor:\n  - A\nbibliography: references.bib\n\n---\n\n# Intro\n"
        );
        assert_eq!(bibliography(&with_bibliography(text, "references.bib")), v(&["references.bib"]));
    }

    #[test]
    fn added_to_an_empty_front_matter_and_with_crlf() {
        assert_eq!(with_bibliography("---\n---\ntext", "r.bib"), "---\nbibliography: r.bib\n---\ntext");
        assert_eq!(
            with_bibliography("---\r\ntitle: T\r\n---\r\ntext", "r.bib"),
            "---\r\ntitle: T\r\nbibliography: r.bib\r\n---\r\ntext"
        );
        assert_eq!(with_bibliography("---\ntitle: T\n...\ntext", "r.bib"), "---\ntitle: T\nbibliography: r.bib\n...\ntext");
    }

    #[test]
    fn added_in_a_new_front_matter() {
        assert_eq!(with_bibliography("# Intro\n", "references.bib"), "---\nbibliography: references.bib\n---\n\n# Intro\n");
        assert_eq!(with_bibliography("", "r.bib"), "---\nbibliography: r.bib\n---\n\n");
        // a fence later in the text does not count
        let text = "Intro\n\n---\ntitle: x\n---\n";
        assert_eq!(with_bibliography(text, "r.bib"), format!("---\nbibliography: r.bib\n---\n\n{text}"));
    }

    #[test]
    fn a_name_that_needs_quotes_is_quoted() {
        let text = with_bibliography("x", "my: refs.bib");
        assert_eq!(text, "---\nbibliography: \"my: refs.bib\"\n---\n\nx");
        assert_eq!(bibliography(&text), v(&["my: refs.bib"]));
    }

    #[test]
    fn an_existing_field_is_kept() {
        let text = "---\nbibliography: a.bib\n---\n";
        assert_eq!(with_bibliography(text, "b.bib"), text);
    }
}
