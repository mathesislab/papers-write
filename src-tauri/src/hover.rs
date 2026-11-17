//! What the editor shows on hovering the text: an equation rendered (`render_math`: pandoc's MathML, the same as the
//! preview's, cached) and a figure's thumbnail (`image_data`: the data URI of the file, as the preview embeds it).
//! The reference of a citation key is `refs::reference_of`.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Manager, Webview};

use crate::{documents, preview};

/// the longest formula rendered
const LONGEST: usize = 20_000;
/// the equations kept
const KEPT: usize = 256;

type Cache = Mutex<HashMap<(String, bool), String>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// The MathML of `tex` (`display`: as `$$…$$`, else `$…$`), the `<math>` element alone; an error when pandoc could
/// not read it as math.
pub fn math_html(tex: &str, display: bool) -> Result<String, String> {
    let tex = tex.trim();
    if tex.is_empty() || tex.len() > LONGEST {
        return Err("no formula".into());
    }
    let key = (tex.to_string(), display);
    if let Some(hit) = cache().lock().unwrap().get(&key) {
        return Ok(hit.clone());
    }
    // a blank line would end the paragraph, and the formula with it
    let body = tex.lines().filter(|l| !l.trim().is_empty()).collect::<Vec<_>>().join("\n");
    let input = if display { format!("$${body}$$") } else { format!("${body}$") };
    let mut child = Command::new(preview::pandoc()?)
        .args(["-f", "markdown", "-t", "html5", "--mathml", "--wrap=none"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("pandoc did not start: {e}"))?;
    let mut stdin = child.stdin.take().expect("piped");
    // the formula is short: it fits the pipe, and pandoc reads it all before it writes
    stdin.write_all(input.as_bytes()).map_err(|e| e.to_string())?;
    drop(stdin);
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let html = String::from_utf8_lossy(&out.stdout);
    let (Some(from), Some(to)) = (html.find("<math"), html.rfind("</math>")) else {
        return Err("not a formula pandoc reads".into());
    };
    let html = html[from..to + "</math>".len()].to_string();
    let mut cache = cache().lock().unwrap();
    if cache.len() >= KEPT {
        cache.clear();
    }
    cache.insert(key, html.clone());
    Ok(html)
}

/// The equation `tex` as MathML for the hover.
#[tauri::command]
pub async fn render_math(tex: String, display: bool) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || math_html(&tex, display)).await.map_err(|e| e.to_string())?
}

/// The image of a path as the document writes it (beside the document, else the home folder for an untitled one), as
/// a data URI; None for a link, a file that is none or too large.
pub fn image_in(dir: &Path, path: &str) -> Option<String> {
    let path = path.trim().trim_matches(['<', '>']);
    if path.is_empty() || path.contains(':') || path.starts_with("//") {
        return None;
    }
    let name = preview::percent_decode(path);
    preview::data_uri(&dir.join(name.split(['?', '#']).next().unwrap_or("")))
}

/// The folder of the document of this window (the home folder for an untitled one), where its images are found.
fn image_dir(window: &Webview) -> PathBuf {
    documents::path_of(window.app_handle(), window.label())
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default())
}

/// The image at `path` of the document of this window, as a data URI.
#[tauri::command]
pub async fn image_data(window: Webview, path: String) -> Option<String> {
    let dir = image_dir(&window);
    tauri::async_runtime::spawn_blocking(move || image_in(&dir, &path)).await.ok().flatten()
}

/// A figure as the page shows it: a data URI, and for a PDF the SHA-1 of its file (`key`), under which the PNG the
/// page draws of it is kept (`keep_figure`); `drawn`: the URI is that PNG, kept from before, not the PDF.
#[derive(Serialize)]
pub struct Figure {
    uri: String,
    key: Option<String>,
    drawn: bool,
}

/// The folder where the PNGs drawn of PDF figures are kept, by the SHA-1 of the PDF: a figure changed is another file.
fn drawn_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_cache_dir().ok().map(|d| d.join("figures"))
}

/// The figure at `path` of the document of this window: a PDF's PNG when it was drawn before, else the file itself.
#[tauri::command]
pub async fn figure_image(window: Webview, path: String) -> Option<Figure> {
    let dir = image_dir(&window);
    let drawn = drawn_dir(window.app_handle());
    tauri::async_runtime::spawn_blocking(move || {
        let uri = image_in(&dir, &path)?;
        if !uri.starts_with("data:application/pdf") {
            return Some(Figure { uri, key: None, drawn: false });
        }
        let name = preview::percent_decode(path.trim().trim_matches(['<', '>']));
        let bytes = std::fs::read(dir.join(name.split(['?', '#']).next().unwrap_or(""))).ok()?;
        let key = sha1_smol::Sha1::from(&bytes).digest().to_string();
        if let Some(png) = drawn.map(|d| d.join(format!("{key}.png"))).and_then(|p| preview::data_uri(&p)) {
            return Some(Figure { uri: png, key: Some(key), drawn: true });
        }
        Some(Figure { uri, key: Some(key), drawn: false })
    })
    .await
    .ok()
    .flatten()
}

/// The PNG the page drew of a PDF figure, kept under the PDF's SHA-1 for the next time (`figure_image`).
#[tauri::command]
pub async fn keep_figure(app: AppHandle, key: String, png: Vec<u8>) -> Result<(), String> {
    if key.len() != 40 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{key}: not a SHA-1"));
    }
    let dir = drawn_dir(&app).ok_or("no cache folder")?;
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&dir)?;
        let part = dir.join(format!("{key}.part"));
        std::fs::write(&part, png)?;
        std::fs::rename(part, dir.join(format!("{key}.png")))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn math_rendered_once() {
        if preview::pandoc().is_err() {
            return;
        }
        let inline = math_html("a^2 + b^2", false).unwrap();
        assert!(inline.starts_with("<math") && inline.ends_with("</math>") && !inline.contains("display=\"block\""), "{inline}");
        let shown = math_html("\\frac{\\partial H}{\\partial t}\n\n= q", true).unwrap();
        assert!(shown.contains("display=\"block\""), "{shown}");
        assert_eq!(math_html("a^2 + b^2", false).unwrap(), inline, "cached");
        let env = math_html("\\begin{align} a &= b \\\\ c &= d \\end{align}", true).unwrap();
        assert!(env.contains("<math"), "{env}");
        assert!(math_html("", false).is_err());
    }

    #[test]
    fn images_found_beside_the_document() {
        let dir = std::env::temp_dir().join(format!("pw-hover-img-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("figures")).unwrap();
        std::fs::write(dir.join("figures/a b.png"), b"Man").unwrap();
        assert_eq!(image_in(&dir, "figures/a%20b.png").as_deref(), Some("data:image/png;base64,TWFu"));
        assert_eq!(image_in(&dir, "<figures/a b.png>").as_deref(), Some("data:image/png;base64,TWFu"));
        assert!(image_in(&dir, "https://x.org/a.png").is_none() && image_in(&dir, "none.png").is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
