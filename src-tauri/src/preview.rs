//! The quick preview: the Markdown as typed rendered by pandoc to an HTML fragment, for the pane beside the text.
//! MathML for the math (WebKit renders it natively: no MathJax, no network), `--citeproc` for the citations (the
//! default CSL, after a Lua filter that numbers the equations, figures, tables and sections and resolves their
//! references, as pandoc-crossref does: `crates/core/lua/crossref.lua`), run in the document's folder so `bibliography:` and the images resolve. One run per window at a
//! time: a request while one runs supersedes it, and the process of the old one is killed.
//!
//! Images: the local ones are rewritten to base64 data URIs here, after pandoc, when the file is under 5 MB. Not
//! `asset://`: the preview is a sandboxed `srcdoc` frame, whose origin is opaque, so it would need the asset
//! protocol enabled, scoped to every document's folder (it is only known at run time), and a CSP for the frame;
//! data URIs need none of these, and a figure shown stays as it was when the text was typed. A PDF figure, which
//! pandoc writes as `<embed>`, is made an `<img>`: WebKit shows a PDF in one on macOS, and the page draws its first
//! page as a PNG elsewhere (pdfImage.ts).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, Manager, Webview};

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

/// A request's place among its window's (`Runs`): its number, and the generation of the window's latest.
#[derive(Clone, Copy)]
pub struct Turn<'a> {
    generation: &'a AtomicU64,
    mine: u64,
}

impl Turn<'_> {
    /// A newer request was made: this one's pandoc is stopped, its result dropped.
    fn superseded(&self) -> bool {
        self.generation.load(Ordering::SeqCst) != self.mine
    }
}

const TIMEOUT: Duration = Duration::from_secs(10);

/// The table of the labels of a whole document (a thesis or book: its main document and `chapters:`), for a chapter's
/// references to the others: made by the cross-reference filter over the files as they are on disk, kept in a file
/// with what the files were then (`files_stamp`), and made again, aside, when one of them changed.
struct LabelTable {
    stamp: Vec<(PathBuf, Option<std::time::SystemTime>, u64)>,
    file: PathBuf,
    making: bool,
}

static TABLES: std::sync::LazyLock<Mutex<HashMap<PathBuf, LabelTable>>> = std::sync::LazyLock::new(Mutex::default);

/// The file of the table of the document `files` make (none yet: being made), made again aside when they changed;
/// the page `page` told when a new one is there (`preview-labels`: it previews again).
fn label_table(app: &AppHandle, page: &str, files: Vec<PathBuf>) -> Option<PathBuf> {
    let stamp = write_core::refs::files_stamp(&files);
    let main = files.first()?.clone();
    let mut tables = TABLES.lock().unwrap();
    let table = tables.entry(main.clone()).or_insert_with(|| {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        main.hash(&mut h);
        LabelTable { stamp: Vec::new(), file: std::env::temp_dir().join(format!("papers-write-labels-{:016x}.tsv", h.finish())), making: false }
    });
    if table.stamp != stamp && !table.making {
        table.making = true;
        let (app, page, file) = (app.clone(), page.to_string(), table.file.clone());
        std::thread::spawn(move || {
            // once the files are still a moment (an autosave every few seconds while one types: one pass in a pause)
            quiet(&files);
            let stamp = write_core::refs::files_stamp(&files);
            let made = make_table(&files, &file);
            let mut tables = TABLES.lock().unwrap();
            if let Some(table) = tables.get_mut(&main) {
                table.making = false;
                if made {
                    table.stamp = stamp;
                }
            }
            drop(tables);
            if made {
                let _ = app.emit_to(EventTarget::webview(&page), "preview-labels", ());
            }
        });
    }
    table.file.is_file().then(|| table.file.clone())
}

/// How long the files of a document are left unchanged before their table is made again.
const QUIET: std::time::Duration = std::time::Duration::from_secs(5);

/// Back once none of `files` changed for `QUIET` (at most a minute: a file always changing does not hold it back).
fn quiet(files: &[PathBuf]) {
    let start = std::time::Instant::now();
    loop {
        let newest = files.iter().filter_map(|f| std::fs::metadata(f).ok()?.modified().ok()).max();
        let still = newest.and_then(|t| t.elapsed().ok()).unwrap_or(QUIET);
        if still >= QUIET || start.elapsed() > std::time::Duration::from_secs(60) {
            return;
        }
        std::thread::sleep(QUIET - still);
    }
}

/// The table of the labels of the document `files` make written to `file` (the filter's PAPERS_WRITE_LABELS_OUT);
/// whether it was.
fn make_table(files: &[PathBuf], file: &Path) -> bool {
    let (Ok(pandoc), Some(filter)) = (pandoc(), crossref_filter()) else {
        return false;
    };
    let dir = files.first().and_then(|m| m.parent()).map(Path::to_path_buf).unwrap_or_else(home);
    let part = file.with_extension("tsv.part");
    let headings_part = file.with_extension("headings.part");
    let Ok(mut child) = Command::new(pandoc)
        .args(["-f", "markdown", "-t", "json"])
        .arg(format!("--lua-filter={}", filter.display()))
        .env("PAPERS_WRITE_LABELS_OUT", &part)
        .env("PAPERS_WRITE_HEADINGS_OUT", &headings_part)
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let whole = write_core::refs::whole_text(files);
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(whole.as_bytes());
    }
    child.wait().is_ok_and(|s| s.success()) && std::fs::rename(&headings_part, headings_of(file)).is_ok() && std::fs::rename(&part, file).is_ok()
}

/// Where the headings of a table's document are kept, beside its labels.
fn headings_of(table: &Path) -> PathBuf {
    table.with_extension("headings.tsv")
}

/// A heading of a whole document: the file it is in (by its place in `files`), its level, number (none: unnumbered,
/// or the document numbers no sections) and text.
#[derive(Serialize)]
pub struct OutlineHeading {
    file: usize,
    level: u8,
    number: String,
    title: String,
}

/// The outline of the whole document the asking text is part of (a thesis or book: its main document and chapters):
/// its files and their headings, as the last made table has them (made again aside when they changed: the page is
/// told, `preview-labels`). None for a text of no such document, or before a table was made.
#[derive(Serialize)]
pub struct WholeOutline {
    files: Vec<String>,
    headings: Vec<OutlineHeading>,
}

#[tauri::command]
pub fn document_outline(window: Webview) -> Option<WholeOutline> {
    let path = documents::path_of(window.app_handle(), window.label())?;
    let files = write_core::refs::document_files(&path)?;
    let table = label_table(window.app_handle(), window.label(), files.clone())?;
    let text = std::fs::read_to_string(headings_of(&table)).ok()?;
    let headings = text
        .lines()
        .filter_map(|l| {
            let mut f = l.splitn(4, '\t');
            Some(OutlineHeading { file: f.next()?.parse().ok()?, level: f.next()?.parse().ok()?, number: f.next()?.to_string(), title: f.next()?.to_string() })
        })
        .collect();
    Some(WholeOutline { files: files.iter().map(|f| f.to_string_lossy().into_owned()).collect(), headings })
}

/// Where the `nth` heading (from 0) of the text `title` is in the Markdown file `path`, by its line's offset (UTF-16): for a
/// heading of another file of the outline to be gone to. Its `#`s and attributes (`{#sec:x}`) left out; fenced code
/// passed over.
#[tauri::command]
pub fn heading_place(path: String, title: String, nth: usize) -> Option<usize> {
    let text = std::fs::read_to_string(&path).ok()?;
    let want = title.trim();
    let (mut at, mut fenced, mut seen) = (0usize, false, 0usize);
    for line in text.split_inclusive('\n') {
        let t = line.trim_end();
        if t.trim_start().starts_with("```") || t.trim_start().starts_with("~~~") {
            fenced = !fenced;
        } else if !fenced && t.starts_with('#') {
            let body = t.trim_start_matches('#').trim();
            let body = body.rfind('{').filter(|_| body.ends_with('}')).map_or(body, |i| body[..i].trim_end());
            let plain: String = body.chars().filter(|c| !matches!(c, '*' | '_' | '`')).collect();
            if plain.trim() == want || body == want {
                if seen == nth {
                    // as the editor counts (UTF-16), as the project's search gives its places
                    return Some(text[..at].encode_utf16().count());
                }
                seen += 1;
            }
        }
        at += line.len();
    }
    None
}
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
    let (app, label) = (window.app_handle().clone(), window.label().to_string());
    tauri::async_runtime::spawn_blocking(move || {
        let (dir, saved) = match path.as_deref().and_then(Path::parent) {
            Some(dir) => (dir.to_path_buf(), true),
            None => (home(), false),
        };
        // a chapter naming no bibliography: its main document's
        let bibs = path.as_deref().map(|p| write_core::refs::main_bibliography(p, &text)).unwrap_or_default();
        // a chapter of a thesis or book: the labels of the other chapters from the whole document's table
        let labels = path.as_deref().and_then(write_core::refs::document_files).and_then(|files| label_table(&app, &label, files));
        render(&text, &dir, saved, &bibs, labels.as_deref(), Turn { generation: &generation, mine })
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
pub fn render(text: &str, dir: &Path, saved: bool, bibs: &[PathBuf], labels: Option<&Path>, turn: Turn) -> Result<Preview, String> {
    render_with(text, dir, saved, bibs, labels, turn, crossref_filter())
}

/// `render` with the Lua `filter`. What fails is dropped in turn, the filter first (an error that speaks of Lua or
/// a filter), then the citations (a bibliography that is missing, or one that does not parse): the text still shows.
fn render_with(text: &str, dir: &Path, saved: bool, bibs: &[PathBuf], labels: Option<&Path>, turn: Turn, filter: Option<&Path>) -> Result<Preview, String> {
    let start = Instant::now();
    let pandoc = pandoc()?;
    let mut citeproc = saved || text.contains("bibliography") || text.contains("references:");
    let mut filter = filter;
    let mut error = None;
    let html = loop {
        match run(&pandoc, text, dir, filter, citeproc.then_some(bibs), labels, turn) {
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
/// `citeproc`: the citations resolved, with these `.bib` files besides those the text names.
fn run(pandoc: &Path, text: &str, dir: &Path, filter: Option<&Path>, citeproc: Option<&[PathBuf]>, labels: Option<&Path>, turn: Turn) -> Result<String, String> {
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
        // the whole document's labels, for a chapter's references to the others
        if let Some(labels) = labels {
            command.env("PAPERS_WRITE_LABELS", labels);
        }
    }
    if let Some(bibs) = citeproc {
        for bib in bibs {
            command.arg(format!("--bibliography={}", bib.display()));
        }
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
        let stop = if turn.superseded() {
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
    if turn.superseded() {
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

/// The local images of the fragment as data URIs. Pandoc writes a PDF figure as `<embed>`, which shows nothing in the
/// frame: it is made an `<img>` (which WebKit draws a PDF in on macOS, and the page draws elsewhere).
fn embed(html: &str, dir: &Path) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = [rest.find("<img "), rest.find("<embed ")].into_iter().flatten().min() {
        let (before, tag_on) = rest.split_at(at);
        out.push_str(before);
        let end = tag_on.find('>').map_or(tag_on.len(), |i| i + 1);
        let (tag, after) = tag_on.split_at(end);
        let tag = match tag.strip_prefix("<embed ") {
            Some(attributes) => format!("<img alt=\"\" {attributes}"),
            None => tag.to_string(),
        };
        out.push_str(&embed_tag(&tag, dir));
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
    fn a_pdf_figure_is_embedded_as_an_image() {
        let dir = std::env::temp_dir().join(format!("preview-embed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("f.pdf"), b"%PDF").unwrap();
        let html = embed(r#"<figure><embed src="f.pdf" /><figcaption>x</figcaption></figure>"#, &dir);
        assert_eq!(html, r#"<figure><img alt="" src="data:application/pdf;base64,JVBERg==" /><figcaption>x</figcaption></figure>"#);
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
        let p = render(&text, &dir, true, &[], None, Turn { generation: &generation, mine: 1 }).unwrap();
        println!("preview of {} words: {} ms, error {:?}", text.split_whitespace().count(), p.ms, p.error);
        assert!(p.error.is_none(), "{:?}", p.error);
        assert!(p.html.contains("<math"), "MathML");
        assert!(p.html.contains("Schoof"), "citeproc");
        // a missing bibliography: the text still shows, the message too
        let missing = text.replace("refs.bib", "none.bib");
        let p = render(&missing, &dir, true, &[], None, Turn { generation: &generation, mine: 1 }).unwrap();
        assert!(p.error.is_some() && p.html.contains("<h1"));
        // superseded: a newer generation
        assert_eq!(render(&text, &dir, true, &[], None, Turn { generation: &generation, mine: 0 }).unwrap_err(), SUPERSEDED);
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
        let p = render(SAMPLE, &dir, false, &[], None, Turn { generation: &generation, mine: 1 }).unwrap();
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
    fn quarto_crossrefs_with_quarto_words() {
        if pandoc().is_err() {
            return;
        }
        let dir = scratch("quarto");
        let generation = AtomicU64::new(1);
        let text = "# Intro {#sec-intro}\n\n$$\na = b\n$$ {#eq-a}\n\nSee @eq-a, @sec-intro and [@fig-x; @fig-y], @fig-nope.\n\n![X](x.png){#fig-x}\n\n![Y](y.png){#fig-y}\n";
        let p = render(text, &dir, false, &[], None, Turn { generation: &generation, mine: 1 }).unwrap();
        assert!(p.error.is_none(), "{:?}", p.error);
        let h = &p.html;
        assert!(h.contains("Equation\u{a0}<a href=\"#eq-a\">1</a>"), "{h}");
        assert!(h.contains("Section\u{a0}<a href=\"#sec-intro\">1</a>"), "{h}");
        assert!(h.contains("Figures\u{a0}<a href=\"#fig-x\">1</a> and <a href=\"#fig-y\">2</a>"), "{h}");
        assert!(h.contains("Figure 1: X") && h.contains("<strong>??</strong>"), "{h}");
        // the front matter's prefix, as Quarto's
        let text = format!("---\ncrossref:\n  fig-prefix: Fig.\n---\n\n{text}");
        let h = render(&text, &dir, false, &[], None, Turn { generation: &generation, mine: 1 }).unwrap().html;
        assert!(h.contains("Fig.\u{a0}<a href=\"#fig-x\">1</a> and"), "{h}");
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
        let p = render(&text, &dir, false, &[], None, Turn { generation: &generation, mine: 1 }).unwrap();
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
        let p = render(text, &dir, false, &[], None, Turn { generation: &generation, mine: 1 }).unwrap();
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
        let p = render_with("# Title\n\nSome $x^2$ text.\n", &dir, false, &[], None, Turn { generation: &generation, mine: 1 }, Some(&bad)).unwrap();
        assert!(p.html.contains("<h1") && p.html.contains("<math"), "{}", p.html);
        assert!(p.error.is_some(), "the message is shown");
        // an error raised while running, outside the filter's own pcall
        std::fs::write(&bad, "function Pandoc(d) error('boom') end").unwrap();
        let p = render_with("# Title\n", &dir, false, &[], None, Turn { generation: &generation, mine: 1 }, Some(&bad)).unwrap();
        assert!(p.html.contains("<h1") && p.error.as_deref().is_some_and(|e| e.contains("boom")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
