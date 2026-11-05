//! The bibliography of a LaTeX file: the `.bib` files named by `\bibliography{a,b}` (BibTeX) and
//! `\addbibresource{x.bib}` (biblatex), the keys cited, and the line that names a `.bib` added to the text.
//! The files it includes (`\input`, `\include`, `\subfile`) are followed ([`included`]): their bibliography,
//! labels and citations belong to the document.

use std::path::{Path, PathBuf};

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

/// The deepest `\input` chain followed.
const MAX_DEPTH: usize = 10;

/// An included file: where it is and its text.
#[derive(Clone, Debug, PartialEq)]
pub struct Included {
    pub path: PathBuf,
    pub text: String,
}

/// The files named by `\input{x}`, `\include{x}` and `\subfile{x}` in `text` (comments ignored), as written.
pub fn input_names(text: &str) -> Vec<String> {
    let text = without_comments(text);
    let mut found: Vec<(usize, String)> = Vec::new();
    for cmd in ["input", "include", "subfile", "subfileinclude"] {
        let pat = format!("\\{cmd}");
        let mut from = 0;
        while let Some(i) = text[from..].find(&pat) {
            let at = from + i;
            from = at + pat.len();
            if let Some(name) = arguments(&text[at..], cmd).into_iter().next() {
                found.push((at, name.trim().to_string()));
            }
        }
    }
    found.sort();
    found.into_iter().map(|(_, n)| n).filter(|n| !n.is_empty()).collect()
}

/// The files `text` (the text of `main`) includes, and those they include, depth first in the order of the text, each
/// once: names relative to the folder of `main` (where LaTeX runs), `.tex` added to a name without it. A file
/// that cannot be read, a cycle (a file including one above it, or `main`), and a chain deeper than ten are left out.
pub fn included(main: &Path, text: &str) -> Vec<Included> {
    let folder = main.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let mut seen = vec![main.to_path_buf()];
    let mut out = Vec::new();
    follow(&folder, text, 1, &mut seen, &mut out);
    out
}

fn follow(folder: &Path, text: &str, depth: usize, seen: &mut Vec<PathBuf>, out: &mut Vec<Included>) {
    if depth > MAX_DEPTH {
        return;
    }
    for name in input_names(text) {
        let name = if name.to_ascii_lowercase().ends_with(".tex") { name } else { format!("{name}.tex") };
        let path = folder.join(&name);
        let key = path.canonicalize().unwrap_or_else(|_| path.clone());
        if seen.iter().any(|s| s == &key || s.canonicalize().is_ok_and(|c| c == key)) {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(&path) else { continue };
        seen.push(key);
        out.push(Included { path, text: body.clone() });
        follow(folder, &body, depth + 1, seen, out);
    }
}

/// [`bibliography`] of `text` and the files it includes, each `.bib` once.
pub fn bibliography_with_inputs(main: &Path, text: &str) -> Vec<String> {
    let mut names = bibliography(text);
    for file in included(main, text) {
        for n in bibliography(&file.text) {
            if !names.contains(&n) {
                names.push(n);
            }
        }
    }
    names
}

/// The keys cited in `text` and the files it includes, in order, each once.
pub fn cited_keys_with_inputs(main: &Path, text: &str) -> Vec<String> {
    let mut keys = cited_keys(text);
    for file in included(main, text) {
        for k in cited_keys(&file.text) {
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
    }
    keys
}

/// The labels (`\label{x}`) of a LaTeX text, in order, each once; comments ignored.
pub fn labels(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in arguments(&without_comments(text), "label").into_iter().map(str::trim).filter(|l| !l.is_empty()) {
        if !out.iter().any(|x| x == l) {
            out.push(l.to_string());
        }
    }
    out
}

/// The labels of the files `text` (the text of `main`) includes, not those of `text` itself.
pub fn included_labels(main: &Path, text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for file in included(main, text) {
        for l in labels(&file.text) {
            if !out.contains(&l) {
                out.push(l);
            }
        }
    }
    out
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

    fn tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pw-latex-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (f, body) in files {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        dir
    }

    #[test]
    fn input_commands() {
        let t = "\\input{a} \\include{chap/b.tex} \\subfile{ c } % \\input{no}\n\\includegraphics{f} \\inputminted{x}{y}";
        assert_eq!(input_names(t), ["a", "chap/b.tex", "c"]);
    }

    #[test]
    fn included_files_are_followed_once_in_order_without_cycles() {
        let dir = tree(
            "follow",
            &[
                ("a.tex", "\\label{la}\\input{sub/b}\\input{a}\\input{main}"),
                ("sub/b.tex", "\\label{lb}\\cite{kb}\\bibliography{b}"),
                ("c.tex", "\\label{lc} \\label{la}"),
            ],
        );
        let main = dir.join("main.tex");
        let text = "\\input{a}\\include{c}\\input{a}\\input{missing}\\bibliography{m}";
        let files: Vec<_> = included(&main, text).into_iter().map(|f| f.path.strip_prefix(&dir).unwrap().to_path_buf()).collect();
        assert_eq!(files, [PathBuf::from("a.tex"), PathBuf::from("sub/b.tex"), PathBuf::from("c.tex")]);
        assert_eq!(bibliography_with_inputs(&main, text), ["m.bib", "b.bib"]);
        assert_eq!(cited_keys_with_inputs(&main, "\\cite{km}\\input{a}"), ["km", "kb"]);
        assert_eq!(included_labels(&main, text), ["la", "lb", "lc"]);
        assert_eq!(labels("\\label{x}\\label{x} % \\label{y}"), ["x"]);
    }

    #[test]
    fn a_chain_deeper_than_ten_stops() {
        let files: Vec<(String, String)> = (1..=14).map(|i| (format!("f{i}.tex"), format!("\\label{{l{i}}}\\input{{f{}}}", i + 1))).collect();
        let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let dir = tree("deep", &refs);
        let found = included(&dir.join("main.tex"), "\\input{f1}");
        assert_eq!(found.len(), MAX_DEPTH);
    }
}
