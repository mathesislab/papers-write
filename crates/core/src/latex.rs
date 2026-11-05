//! The bibliography of a LaTeX file: the `.bib` files named by `\bibliography{a,b}` (BibTeX) and
//! `\addbibresource{x.bib}` (biblatex), the keys cited, and the line that names a `.bib` added to the text.

/// `line` up to its first `%` that is no `\%` (the comment dropped).
fn uncommented(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'%' => return &line[..i],
            _ => {}
        }
        i += 1;
    }
    line
}

/// The text without its comments, line ends kept.
fn without_comments(tex: &str) -> String {
    tex.split_inclusive('\n')
        .map(|l| {
            let body = uncommented(l.trim_end_matches(['\n', '\r']));
            format!("{body}{}", &l[l.trim_end_matches(['\n', '\r']).len()..])
        })
        .collect()
}

/// The arguments of `\name` (`\name*`, optional `[..]` skipped) in `text`, each as written between the braces.
fn arguments<'a>(text: &'a str, name: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let pat = format!("\\{name}");
    let mut from = 0;
    while let Some(i) = text[from..].find(&pat) {
        let mut at = from + i + pat.len();
        from = at;
        // not a longer command (\bibliographystyle)
        if text[at..].starts_with(|c: char| c.is_ascii_alphabetic()) {
            continue;
        }
        let mut rest = text[at..].trim_start_matches('*').trim_start();
        while let Some(opt) = rest.strip_prefix('[') {
            let Some(end) = opt.find(']') else { break };
            rest = opt[end + 1..].trim_start();
        }
        if let Some(body) = rest.strip_prefix('{')
            && let Some(end) = body.find('}')
        {
            out.push(&body[..end]);
            at = text.len() - rest.len() + 1 + end;
            from = at;
        }
    }
    out
}

/// The `.bib` files a LaTeX text names, in order, each once: `\bibliography{a,b}` (`.bib` added to a name without
/// it) and `\addbibresource{x.bib}` (a URL is left out). Commented-out lines are ignored.
pub fn bibliography(tex: &str) -> Vec<String> {
    let text = without_comments(tex);
    let mut names: Vec<String> = Vec::new();
    let mut add = |n: &str| {
        let n = n.trim();
        if n.is_empty() || n.contains("://") {
            return;
        }
        let n = if n.to_ascii_lowercase().ends_with(".bib") { n.to_string() } else { format!("{n}.bib") };
        if !names.contains(&n) {
            names.push(n);
        }
    };
    // in the order of the text
    let mut found: Vec<(usize, &str, bool)> = Vec::new();
    for (cmd, list) in [("bibliography", true), ("addbibresource", false), ("addglobalbib", false)] {
        let pat = format!("\\{cmd}");
        let mut from = 0;
        while let Some(i) = text[from..].find(&pat) {
            found.push((from + i, cmd, list));
            from += i + pat.len();
        }
    }
    found.sort();
    for (pos, cmd, list) in found {
        for arg in arguments(&text[pos..], cmd).into_iter().take(1) {
            if list {
                arg.split(',').for_each(&mut add);
            } else {
                add(arg);
            }
        }
    }
    names
}

/// The keys cited by `\cite`, `\citep[..][..]{a,b}`, `\parencite`... in a LaTeX text, in order, each once; comments
/// ignored.
pub fn cited_keys(tex: &str) -> Vec<String> {
    let text = without_comments(tex);
    let mut keys: Vec<String> = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        let name: String = text[i + 1..].chars().take_while(char::is_ascii_alphabetic).collect();
        let after = i + 1 + name.len();
        i = after.max(i + 1) + usize::from(name.is_empty());
        let is_cite = name.starts_with("cite") || name.ends_with("cite") || name.ends_with("cites");
        if !is_cite || name == "citestyle" {
            continue;
        }
        let mut rest = text[after..].trim_start_matches('*').trim_start();
        while let Some(opt) = rest.strip_prefix('[') {
            let Some(end) = opt.find(']') else { break };
            rest = opt[end + 1..].trim_start();
        }
        if let Some(body) = rest.strip_prefix('{')
            && let Some(end) = body.find('}')
        {
            for k in body[..end].split(',').map(str::trim).filter(|k| !k.is_empty()) {
                if !keys.iter().any(|x| x == k) {
                    keys.push(k.to_string());
                }
            }
        }
    }
    keys
}

/// The text with `bib` (a name with or without `.bib`) named as its bibliography: with biblatex, an
/// `\addbibresource` after its `\usepackage`; else `\bibliographystyle{plain}` and `\bibliography` before
/// `\end{document}` (at the end of a text without one).
pub fn with_bibliography(tex: &str, bib: &str) -> String {
    let stem = bib.strip_suffix(".bib").unwrap_or(bib);
    let mut at = 0;
    for line in tex.split_inclusive('\n') {
        let code = uncommented(line);
        if code.contains("\\usepackage") && code.contains("biblatex") {
            let end = at + line.len();
            let nl = if line.ends_with('\n') { "" } else { "\n" };
            return format!("{}{nl}\\addbibresource{{{stem}.bib}}\n{}", &tex[..end], &tex[end..]);
        }
        at += line.len();
    }
    let mut at = 0;
    let mut close = None;
    for line in tex.split_inclusive('\n') {
        if uncommented(line).contains("\\end{document}") {
            close = Some(at);
        }
        at += line.len();
    }
    let lines = format!("\\bibliographystyle{{plain}}\n\\bibliography{{{stem}}}\n");
    match close {
        Some(i) => format!("{}{lines}\n{}", &tex[..i], &tex[i..]),
        None => format!("{tex}{}{lines}", if tex.ends_with('\n') || tex.is_empty() { "" } else { "\n" }),
    }
}

/// Whether the file is LaTeX by its extension (`.tex`).
pub fn is_latex(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("tex"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bibliography_commands() {
        assert_eq!(bibliography("\\bibliography{a,b}"), ["a.bib", "b.bib"]);
        assert_eq!(bibliography("\\bibliography{ a , refs/b.bib }"), ["a.bib", "refs/b.bib"]);
        assert_eq!(bibliography("\\usepackage{biblatex}\n\\addbibresource{x.bib}\n\\addbibresource[location=remote]{http://e.org/y.bib}\n"), ["x.bib"]);
        assert_eq!(bibliography("\\bibliographystyle{plain}\n\\bibliography{a}\n\\bibliography{a}"), ["a.bib"]);
        assert!(bibliography("no bibliography here").is_empty());
    }

    #[test]
    fn commented_lines_are_ignored() {
        assert_eq!(bibliography("% \\bibliography{old}\n\\bibliography{new} % \\bibliography{older}\n"), ["new.bib"]);
        // an escaped percent is no comment
        assert_eq!(bibliography("50\\% \\bibliography{a}"), ["a.bib"]);
    }

    #[test]
    fn citations() {
        let t = "\\cite{a, b} \\citep[see][p.~2]{c} \\textcite{a} % \\cite{no}\n\\ref{x} \\citestyle{y}";
        assert_eq!(cited_keys(t), ["a", "b", "c"]);
    }

    #[test]
    fn a_bibliography_named() {
        let t = "\\begin{document}\ntext\n\\end{document}\n";
        assert_eq!(with_bibliography(t, "references.bib"), "\\begin{document}\ntext\n\\bibliographystyle{plain}\n\\bibliography{references}\n\n\\end{document}\n");
        assert_eq!(bibliography(&with_bibliography(t, "references.bib")), ["references.bib"]);
        let b = "\\usepackage[style=x]{biblatex}\n\\begin{document}\\end{document}";
        assert_eq!(bibliography(&with_bibliography(b, "r")), ["r.bib"]);
        assert_eq!(with_bibliography("text", "r.bib"), "text\n\\bibliographystyle{plain}\n\\bibliography{r}\n");
    }
}
