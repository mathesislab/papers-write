//! The quick preview: the Markdown as typed rendered by pandoc to an HTML fragment, for the pane beside the text.
//! MathML for the math (WebKit renders it natively: no MathJax, no network), `--citeproc` for the citations (the
//! default CSL), run in the document's folder so `bibliography:` and the images resolve. One run per window at a
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
use tauri::{Manager, WebviewWindow};

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
pub async fn preview(window: WebviewWindow, text: String) -> Result<Preview, String> {
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

/// pandoc: on the PATH, else where Homebrew puts it (an app started from the Dock has a short PATH).
fn pandoc() -> Result<PathBuf, String> {
    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    let fixed = ["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from);
    on_path
        .into_iter()
        .chain(fixed)
        .map(|d| d.join("pandoc"))
        .find(|p| p.is_file())
        .ok_or_else(|| "pandoc is not installed: brew install pandoc".to_string())
}

/// The text rendered in `dir`. An untitled document (`saved` false) has no citeproc unless it names a bibliography.
pub fn render(text: &str, dir: &Path, saved: bool, generation: &AtomicU64, mine: u64) -> Result<Preview, String> {
    let start = Instant::now();
    let pandoc = pandoc()?;
    let wants = saved || text.contains("bibliography") || text.contains("references:");
    let mut error = None;
    let mut run = run(&pandoc, text, dir, wants, generation, mine);
    if wants {
        if let Err(e) = &run {
            if e != SUPERSEDED {
                // a bibliography that is missing, or one that does not parse: the text without the citations
                error = Some(e.clone());
                run = self::run(&pandoc, text, dir, false, generation, mine);
            }
        }
    }
    let html = embed(&run?, dir);
    Ok(Preview { html, error, ms: start.elapsed().as_millis() as u64 })
}

/// One run of pandoc: its HTML, or what it said when it failed.
fn run(pandoc: &Path, text: &str, dir: &Path, citeproc: bool, generation: &AtomicU64, mine: u64) -> Result<String, String> {
    let mut command = Command::new(pandoc);
    command
        .args(["-f", "markdown", "-t", "html5", "--mathml", "--wrap=none"])
        .arg(format!("--resource-path={}", dir.display()))
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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
    let mime = match file.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("pdf") => "application/pdf",
        _ => return tag.into(),
    };
    match std::fs::metadata(&file) {
        Ok(m) if m.len() <= LARGEST => {}
        _ => return tag.into(),
    }
    let Ok(bytes) = std::fs::read(&file) else { return tag.into() };
    format!("{}data:{mime};base64,{}{}", &tag[..from], base64(&bytes), &tag[from + len..])
}

fn percent_decode(s: &str) -> String {
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
}
