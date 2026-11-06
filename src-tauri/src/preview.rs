//! The quick preview: the Markdown as typed rendered by pandoc to an HTML fragment, for the pane beside the text.
//! MathML for the math (WebKit renders it natively: no MathJax, no network), `--citeproc` for the citations (the
//! default CSL, after a Lua filter that numbers the equations, figures, tables and sections and resolves their
//! references, as pandoc-crossref does: `crates/core/lua/crossref.lua`), run in the document's folder so `bibliography:` and the images resolve. One run per window at a
//! time: a request while one runs supersedes it, and the process of the old one is killed.
//!
//! Images: the local ones are rewritten to base64 data URIs here, after pandoc, when the file is under 5 MB. Not
//! `asset://`: the preview is a sandboxed `srcdoc` frame, whose origin is opaque, so it would need the asset
//! protocol enabled, scoped to every document's folder (it is only known at run time), and a CSP for the frame;
//! data URIs need none of these, and a figure shown stays as it was when the text was typed. (WebKit shows a PDF
//! in `<img>`, so figures that are PDFs are embedded as such.)

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{Manager, Webview};

use crate::documents;

/// What a run gives the page.
#[derive(Serialize, Debug)]
pub struct Preview {
    pub html: String,
    /// what pandoc said that went wrong; the html is what could be made of it
    pub error: Option<String>,
    pub ms: u64,
}

/// The generation of the latest request of each window (by label): a run whose number is not it any more is
/// superseded.
#[derive(Default)]
pub struct Runs(Mutex<HashMap<String, Arc<AtomicU64>>>);

const TIMEOUT: Duration = Duration::from_secs(10);
/// the largest image embedded
const LARGEST: u64 = 5 * 1024 * 1024;
pub const SUPERSEDED: &str = "superseded";

/// The preview of `text`, as the document of this window reads it.
#[tauri::command]
pub async fn preview(window: Webview, text: String) -> Result<Preview, String> {
    let runs = window.state::<Runs>();
    let generation = runs.0.lock().unwrap().entry(window.label().to_string()).or_default().clone();
    let mine = generation.fetch_add(1, Ordering::SeqCst) + 1;
    let path = documents::path_of(window.app_handle(), window.label());
    tauri::async_runtime::spawn_blocking(move || {
        let (dir, saved) = match path.as_deref().and_then(Path::parent) {
            Some(dir) => (dir.to_path_buf(), true),
            None => (home(), false),
        };
        render(&text, &dir, saved, &generation, mine)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// pandoc: the file of the settings (given to the core as they change), else searched.
pub(crate) use write_core::exchange::{crossref_filter, pandoc};

/// The text rendered in `dir`. An untitled document (`saved` false) has no citeproc unless it names a bibliography.
pub fn render(text: &str, dir: &Path, saved: bool, generation: &AtomicU64, mine: u64) -> Result<Preview, String> {
    render_with(text, dir, saved, generation, mine, crossref_filter())
}

/// `render` with the Lua `filter`. What fails is dropped in turn, the filter first (an error that speaks of Lua or
/// a filter), then the citations (a bibliography that is missing, or one that does not parse): the text still shows.
fn render_with(text: &str, dir: &Path, saved: bool, generation: &AtomicU64, mine: u64, filter: Option<&Path>) -> Result<Preview, String> {
    let start = Instant::now();
    let pandoc = pandoc()?;
    let mut citeproc = saved || text.contains("bibliography") || text.contains("references:");
    let mut filter = filter;
    let mut error = None;
    let html = loop {
        match run(&pandoc, text, dir, filter, citeproc, generation, mine) {
            Ok(html) => break html,
            Err(e) if e == SUPERSEDED => return Err(e),
            Err(e) => {
                let lower = e.to_lowercase();
                let filtered = filter.is_some() && (lower.contains("lua") || lower.contains("filter"));
                if error.is_none() {
                    error = Some(e.clone());
                }
                if filtered {
                    filter = None;
                } else if citeproc {
                    citeproc = false;
                } else {
                    return Err(e);
                }
            }
        }
    };
    Ok(Preview { html: embed(&html, dir), error, ms: start.elapsed().as_millis() as u64 })
}

/// One run of pandoc: its HTML, or what it said when it failed.
fn run(pandoc: &Path, text: &str, dir: &Path, filter: Option<&Path>, citeproc: bool, generation: &AtomicU64, mine: u64) -> Result<String, String> {
    let mut command = Command::new(pandoc);
    command
        .args(["-f", "markdown", "-t", "html5", "--mathml", "--wrap=none"])
        .arg(format!("--resource-path={}", dir.display()))
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // the filter before citeproc: the references are resolved before citeproc takes them for citations
    if let Some(filter) = filter {
        command.arg(format!("--lua-filter={}", filter.display()));
    }
    if citeproc {
        command.arg("--citeproc");
    }
    let mut child = command.spawn().map_err(|e| format!("pandoc did not start: {e}"))?;
    // the pipes are served by threads, or a full one would stop pandoc while this waits
    let mut stdin = child.stdin.take().unwrap();
    let input = text.as_bytes().to_vec();
    let feed = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let out = drain(child.stdout.take().unwrap());
    let err = drain(child.stderr.take().unwrap());
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }
        let stop = if generation.load(Ordering::SeqCst) != mine {
            Some(SUPERSEDED.to_string())
        } else if start.elapsed() > TIMEOUT {
            Some(format!("pandoc took more than {} s", TIMEOUT.as_secs()))
        } else {
            None
        };
        if let Some(why) = stop {
            let _ = child.kill();
            let _ = child.wait();
            return Err(why);
        }
        std::thread::sleep(Duration::from_millis(4));
    };
    let _ = feed.join();
    let (out, err) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
    if generation.load(Ordering::SeqCst) != mine {
        return Err(SUPERSEDED.into());
    }
    if status.success() { Ok(out) } else { Err(err.trim().to_string()) }
}

fn drain(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// The local images of the fragment as data URIs.
fn embed(html: &str, dir: &Path) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find("<img ") {
        let (before, tag_on) = rest.split_at(at);
        out.push_str(before);
        let end = tag_on.find('>').map_or(tag_on.len(), |i| i + 1);
        let (tag, after) = tag_on.split_at(end);
        out.push_str(&embed_tag(tag, dir));
        rest = after;
    }
    out.push_str(rest);
    out
}

fn embed_tag(tag: &str, dir: &Path) -> String {
    let Some(from) = tag.find(" src=\"").map(|i| i + 6) else { return tag.into() };
    let Some(len) = tag[from..].find('"') else { return tag.into() };
    let src = &tag[from..from + len];
    if src.is_empty() || src.contains(':') || src.starts_with("//") {
        return tag.into();
    }
    let name = percent_decode(&src.replace("&amp;", "&"));
    let file = dir.join(name.split(['?', '#']).next().unwrap_or(""));
    match data_uri(&file) {
        Some(uri) => format!("{}{uri}{}", &tag[..from], &tag[from + len..]),
        None => tag.into(),
    }
}

/// An image file as a data URI: a known type (a PDF too), under 5 MB; None for any other file or one that cannot be read.
pub(crate) fn data_uri(file: &Path) -> Option<String> {
    let mime = match file.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("pdf") => "application/pdf",
        _ => return None,
    };
    if std::fs::metadata(file).ok()?.len() > LARGEST {
        return None;
    }
    Some(format!("data:{mime};base64,{}", base64(&std::fs::read(file).ok()?)))
}

pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%' && i + 2 < b.len())
            .then(|| std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()))
            .flatten();
        match hex {
            Some(v) => (out.push(v), i += 3),
            None => (out.push(b[i]), i += 1),
        };
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads() {
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
    }

    #[test]
    fn images_embedded() {
        let dir = std::env::temp_dir().join(format!("pw-preview-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a b.png"), b"Man").unwrap();
        let html = embed(r#"<p><img src="a%20b.png" alt="x" /><img src="https://x/y.png" /><img src="none.png" /></p>"#, &dir);
        assert!(html.contains(r#"src="data:image/png;base64,TWFu""#));
        assert!(html.contains("https://x/y.png") && html.contains("none.png"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn paper_with_math_and_citation() {
        if pandoc().is_err() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("pw-preview-doc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("refs.bib"),
            "@article{schoof2007,\n author={Schoof, Christian},\n title={Ice sheet grounding line dynamics},\n journal={J. Geophys. Res.},\n year={2007}\n}\n",
        )
        .unwrap();
        let mut text = String::from("---\nbibliography: refs.bib\n---\n\n# Intro\n\nIce flows [@schoof2007]. Inline $a^2$.\n\n$$\\frac{\\partial H}{\\partial t} = -\\nabla\\cdot q$$\n\n");
        for i in 0..400 {
            text.push_str(&format!("Paragraph {i} of words repeated to make a long text, with a citation @schoof2007 and some math $x_{i}$.\n\n"));
        }
        let generation = AtomicU64::new(1);
        let p = render(&text, &dir, true, &generation, 1).unwrap();
        println!("preview of {} words: {} ms, error {:?}", text.split_whitespace().count(), p.ms, p.error);
        assert!(p.error.is_none(), "{:?}", p.error);
        assert!(p.html.contains("<math"), "MathML");
        assert!(p.html.contains("Schoof"), "citeproc");
        // a missing bibliography: the text still shows, the message too
        let missing = text.replace("refs.bib", "none.bib");
        let p = render(&missing, &dir, true, &generation, 1).unwrap();
        assert!(p.error.is_some() && p.html.contains("<h1"));
        // superseded: a newer generation
        assert_eq!(render(&text, &dir, true, &generation, 0).unwrap_err(), SUPERSEDED);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    const SAMPLE: &str = r#"# Intro {#sec:intro}

$$
\frac{\partial H}{\partial t} = -\nabla\cdot(\mathbf{u}H) + \dot{a}
$$ {#eq:mass}

\begin{equation}
  \tau_b = C |u_b|^{m-1} u_b \label{eq:friction}
\end{equation}

Equation @eq:mass and \eqref{eq:friction} give Fig. @fig:loop, see @sec:intro and @tbl:params.

![The hysteresis loop](figures/loop.png){#fig:loop}

| a | b |
|---|---|
| 1 | 2 |

: Parameters {#tbl:params}

Unknown @fig:nope, and a range [@fig:loop; @fig:second].

![Another](figures/second.png){#fig:second}
"#;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pw-preview-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn crossrefs_resolved() {
        if pandoc().is_err() {
            return;
        }
        let dir = scratch("xref");
        let generation = AtomicU64::new(1);
        let p = render(SAMPLE, &dir, false, &generation, 1).unwrap();
        println!("cross-reference sample: {} ms", p.ms);
        assert!(p.error.is_none(), "{:?}", p.error);
        let h = &p.html;
        assert!(!h.contains("eq:mass?") && !h.contains("fig:loop?"), "{h}");
        assert!(h.contains(r##"<a href="#eq:mass">(1)</a>"##) && !h.contains("Eq.") && h.contains(r#"<span class="eqno">(2)</span>"#), "{h}");
        assert!(h.contains(r#"<span class="eqno">(1)</span>"#) && h.contains(r##"<a href="#eq:friction">(2)</a>"##), "{h}");
        assert!(!h.contains("\\begin{equation}") && !h.contains("{#eq:mass}") && !h.contains("\\label"), "{h}");
        assert_eq!(h.matches("<math").count(), 2, "both equations are MathML: {h}");
        assert!(h.contains("Figure 1: The hysteresis loop") && h.contains("Table 1: Parameters"), "{h}");
        assert!(h.contains(r##"Fig. <a href="#fig:loop">1</a>"##) && h.contains(r##"see <a href="#sec:intro">1</a>"##), "bare numbers: {h}");
        assert!(h.contains("<strong>??</strong>"), "{h}");
        assert!(h.contains(r##"<a href="#fig:loop">1</a> and <a href="#fig:second">2</a>"##), "{h}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn crossref_words_come_from_the_front_matter() {
        if pandoc().is_err() {
            return;
        }
        let dir = scratch("words");
        let generation = AtomicU64::new(1);
        let text = format!("---\ncrossref: {{fig: Fig., figs: Figs., eq: Eq., sec: Sect., tbl: \"\"}}\n---\n\n{SAMPLE}\nThree: [@fig:loop; @fig:second; @fig:loop] and @eq:mass, [@eq:mass; @eq:friction].\n");
        let p = render(&text, &dir, false, &generation, 1).unwrap();
        assert!(p.error.is_none(), "{:?}", p.error);
        let h = &p.html;
        assert!(h.contains("Fig.\u{a0}<a href=\"#fig:loop\">1</a>"), "{h}");
        assert!(h.contains("Sect.\u{a0}<a href=\"#sec:intro\">1</a>"), "{h}");
        assert!(h.contains("Eq.\u{a0}<a href=\"#eq:mass\">(1)</a>"), "{h}");
        // the plural word for several; a plural not given (eqs) is the singular; an empty word (tbl) is a bare number
        assert!(h.contains("Figs.\u{a0}<a href=\"#fig:loop\">1</a> and <a href=\"#fig:second\">2</a>"), "{h}");
        assert!(h.contains("Figs.\u{a0}<a href=\"#fig:loop\">1</a>, <a href=\"#fig:second\">2</a> and <a href=\"#fig:loop\">1</a>"), "{h}");
        assert!(h.contains("Eq.\u{a0}<a href=\"#eq:mass\">(1)</a> and <a href=\"#eq:friction\">(2)</a>"), "{h}");
        assert!(h.contains("and <a href=\"#tbl:params\">1</a>"), "{h}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn environments_and_refs() {
        if pandoc().is_err() {
            return;
        }
        let dir = scratch("env");
        let generation = AtomicU64::new(1);
        let text = "\\begin{align}\n a &= b \\label{eq:a} \\nonumber\\\\\n c &= d\n\\end{align}\n\n\\begin{gather*}\n x = 1\n\\end{gather*}\n\nSee \\ref{eq:a} and \\eqref{eq:a}, \\ref{nope}.\n";
        let p = render(text, &dir, false, &generation, 1).unwrap();
        let h = &p.html;
        assert_eq!(h.matches("<math").count(), 2, "{h}");
        assert!(h.contains(r#"<span class="eqno">(1)</span>"#) && !h.contains(r#"eqno">(2)"#), "gather* is not numbered: {h}");
        assert!(h.contains(r##"<a href="#eq:a">1</a> and <a href="#eq:a">(1)</a>"##) && h.contains("<strong>??</strong>"), "{h}");
        assert!(!h.contains("\\label") && !h.contains("\\nonumber"), "{h}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn broken_filter_still_gives_html() {
        if pandoc().is_err() {
            return;
        }
        let dir = scratch("broken");
        let bad = dir.join("bad.lua");
        std::fs::write(&bad, "this is not lua (").unwrap();
        let generation = AtomicU64::new(1);
        let p = render_with("# Title\n\nSome $x^2$ text.\n", &dir, false, &generation, 1, Some(&bad)).unwrap();
        assert!(p.html.contains("<h1") && p.html.contains("<math"), "{}", p.html);
        assert!(p.error.is_some(), "the message is shown");
        // an error raised while running, outside the filter's own pcall
        std::fs::write(&bad, "function Pandoc(d) error('boom') end").unwrap();
        let p = render_with("# Title\n", &dir, false, &generation, 1, Some(&bad)).unwrap();
        assert!(p.html.contains("<h1") && p.error.as_deref().is_some_and(|e| e.contains("boom")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
