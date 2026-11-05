//! The line of a Markdown text that a line of its texmark LaTeX came from. texmark keeps no source map, so the
//! line is found by matching: the words of the `.tex` line (LaTeX escapes and markup normalised, so that
//! `\textbf{x}` is `x` and `\citep{k}` is `@k`, as in the Markdown) are looked for in each line of the Markdown, and
//! the line with the longest run of words in common is the one. Where two lines are as good, or none shares a
//! run of two words (one, for a line of one word), there is no line.

/// The words of `text` (Markdown or LaTeX) as they compare: markup and LaTeX escapes taken out, citations as `@key`,
/// math kept as written (its delimiters `\(` `\)` as `$`).
pub fn words(text: &str) -> Vec<String> {
    let mut s = text.to_string();
    for (from, to) in [("\\(", " $ "), ("\\)", " $ "), ("\\[", " $$ "), ("\\]", " $$ "), ("~", " "), ("``", "\""), ("''", "\""), ("\\%", "%"), ("\\&", "&"), ("\\_", "_"), ("\\#", "#")] {
        s = s.replace(from, to);
    }
    // `$` of Markdown math becomes its own word, as `\(` was
    s = s.replace("$$", " $$ ").replace('$', " $ ").replace(" $  $ ", " $$ ");
    let s = citations_of_latex(&s);
    let s = strip_markup(&s);
    s.split_whitespace()
        .map(|w| w.trim_matches(|c: char| matches!(c, '*' | '`' | '[' | ']' | ';' | '(' | ')')).to_string())
        .map(|w| if w.starts_with('@') { w } else { w.trim_matches('_').to_string() })
        .filter(|w| !w.is_empty())
        .collect()
}

/// `\citep[p. 2]{a,b}` (any `\...cite...` command) as ` @a @b `.
fn citations_of_latex(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('\\') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let name: String = after.chars().take_while(char::is_ascii_alphabetic).collect();
        let mut tail = after[name.len()..].trim_start_matches('*');
        if name.contains("cite") {
            while let Some(opt) = tail.strip_prefix('[') {
                match opt.find(']') {
                    Some(e) => tail = &opt[e + 1..],
                    None => break,
                }
            }
            if let Some(body) = tail.strip_prefix('{')
                && let Some(end) = body.find('}')
            {
                for k in body[..end].split(',').map(str::trim).filter(|k| !k.is_empty()) {
                    out.push_str(&format!(" @{k} "));
                }
                rest = &body[end + 1..];
                continue;
            }
        }
        out.push('\\');
        rest = after;
    }
    out.push_str(rest);
    out
}

/// The text with the text-markup commands (`\textbf{`, `\emph{`...) and the braces taken out; other commands stay (an
/// undefined one is what the error is about).
fn strip_markup(text: &str) -> String {
    let mut s = text.to_string();
    for cmd in ["textbf", "textit", "emph", "texttt", "textsc", "underline", "textrm", "textsf", "textsuperscript", "textsubscript"] {
        s = s.replace(&format!("\\{cmd}{{"), "{");
    }
    s.replace(['{', '}'], " ")
}

/// The length of the longest run of words that `a` and `b` have in common.
fn longest_run(a: &[String], b: &[String]) -> usize {
    let mut best = 0;
    let mut prev = vec![0usize; b.len() + 1];
    for x in a {
        let mut row = vec![0usize; b.len() + 1];
        for (j, y) in b.iter().enumerate() {
            if x == y {
                row[j + 1] = prev[j] + 1;
                best = best.max(row[j + 1]);
            }
        }
        prev = row;
    }
    best
}

/// The line (1-based) of `markdown` that the LaTeX `tex_line` came from; none when no line is clearly the one.
pub fn markdown_line(markdown: &str, tex_line: &str) -> Option<u32> {
    let query = words(tex_line);
    if query.is_empty() {
        return None;
    }
    let need = query.len().min(2);
    let mut best: Option<(usize, u32)> = None;
    let mut tie = false;
    for (i, line) in markdown.lines().enumerate() {
        let score = longest_run(&query, &words(line));
        if score < need {
            continue;
        }
        match best {
            Some((b, _)) if score < b => {}
            Some((b, _)) if score == b => tie = true,
            _ => {
                best = Some((score, i as u32 + 1));
                tie = false;
            }
        }
    }
    best.filter(|_| !tie).map(|(_, n)| n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> String {
        words(s).join(" ")
    }

    #[test]
    fn markup_and_escapes_are_normalised_alike() {
        assert_eq!(w("Some \\textbf{bold} text and \\emph{emph} with \\citep{key1} in"), "Some bold text and emph with @key1 in");
        assert_eq!(w("Some **bold** text and *emph* with [@key1] in"), "Some bold text and emph with @key1 in");
        assert_eq!(w("\\citet[p.~2]{a,b}"), "@a @b");
        assert_eq!(w("[@a; @b]"), "@a @b");
        assert_eq!(w("50\\% of Fig.~3"), "50% of Fig. 3");
        assert_eq!(w("the \\(a = b\\) end"), w("the $a = b$ end"));
    }

    #[test]
    fn the_line_with_the_longest_run_of_words_is_the_one() {
        let md = "# Intro\n\nSome **bold** text and *emph* with a citation [@key1] in the first paragraph.\n\nThis paragraph has an \\undefinedmacro problem and more words to follow after it.\n\nThe equation is $a = \\badmath{b}$ in the middle of a sentence here.\n";
        assert_eq!(markdown_line(md, "This paragraph has an \\undefinedmacro problem and more words to follow"), Some(5));
        assert_eq!(markdown_line(md, "The equation is \\(a = \\badmath{b}\\) in the middle of a sentence here."), Some(7));
        assert_eq!(markdown_line(md, "Some \\textbf{bold} text and \\emph{emph} with a citation \\citep{key1} in"), Some(3));
    }

    #[test]
    fn a_tie_or_no_match_is_no_line() {
        let md = "The same words here.\n\nThe same words here.\n\nOther.\n";
        assert_eq!(markdown_line(md, "The same words here."), None);
        assert_eq!(markdown_line(md, "Nothing alike at all"), None);
        assert_eq!(markdown_line(md, ""), None);
        // one shared word of a longer line is no match
        assert_eq!(markdown_line(md, "Other things entirely"), None);
        // a line of one word needs it
        assert_eq!(markdown_line(md, "Other."), Some(5));
    }
}
