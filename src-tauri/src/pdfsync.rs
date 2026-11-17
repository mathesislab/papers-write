//! From a PDF to its source: the place clicked in a PDF (⌃-click in the pdf.js view), and that file opened with the
//! cursor there. Exactly where the build left a map beside the PDF: texmark's `<pdf>.srcmap.json` (`texmark sync
//! backward`, from the PDF to its Markdown), else SyncTeX's `<pdf>.synctex.gz` (`synctex edit`, to its LaTeX).
//! Otherwise by the words clicked, searched in every source file of the project (`write_core::srcmap::find_text`), so
//! chapters, `\include`, `\includeonly` and draft passes need nothing of their own; where several places match as
//! well (an older copy of a paragraph in notes), the most recently changed file is taken.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tauri::AppHandle;
use write_core::project;
use write_core::srcmap::{self, Place};

/// The folder whose sources a PDF is searched in: its project's, else the PDF's folder, or the one above a build
/// folder (`build`, `build-full`, …).
fn root_of(pdf: &Path) -> Option<PathBuf> {
    if let Some(root) = project::find(pdf) {
        return Some(root);
    }
    let dir = pdf.parent()?;
    let built = dir.file_name().is_some_and(|n| n.to_string_lossy().starts_with("build"));
    Some(if built { dir.parent()?.to_path_buf() } else { dir.to_path_buf() })
}

/// The source files of `root`: its Markdown and LaTeX documents, not those of build folders (`build*`: the LaTeX
/// texmark writes there would match as well as its Markdown).
fn sources(root: &Path) -> Vec<PathBuf> {
    project::documents(root)
        .into_iter()
        .filter(|rel| !rel.components().any(|c| c.as_os_str().to_string_lossy().starts_with("build")))
        .map(|rel| root.join(rel))
        .collect()
}

/// The UTF-16 offset (as the editor counts) of `column` (bytes) in line `line` (1-based) of `text`.
fn offset_of(text: &str, line: u32, column: usize) -> usize {
    let mut at = 0;
    for (i, l) in text.split_inclusive('\n').enumerate() {
        if i as u32 + 1 == line {
            let column = column.min(l.len());
            let column = (0..=column).rev().find(|c| l.is_char_boundary(*c)).unwrap_or(0);
            return at + l[..column].encode_utf16().count();
        }
        at += l.encode_utf16().count();
    }
    at
}

/// Where the text clicked in `pdf` is in its sources: the file, and the UTF-16 offset of the words found.
pub fn find(pdf: &Path, text: &str, word: &str) -> Option<(PathBuf, usize, usize)> {
    let root = root_of(pdf)?;
    let files = sources(&root);
    let texts: Vec<String> = files.iter().map(|f| std::fs::read_to_string(f).unwrap_or_default()).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let places = srcmap::find_text(&refs, text);
    let modified = |p: &Place| std::fs::metadata(&files[p.text]).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
    let place = places.iter().max_by_key(|p| modified(p))?;
    let (from, to) = span_of(&texts[place.text], place.line, place.column, word);
    Some((files[place.text].clone(), from, to))
}

/// What to mark in line `line` (1-based) of `text`, from byte `column` of it on: the word clicked, where it is
/// there (its first place from `column`, else from the line's start); else the word at `column`. Its start and end as
/// UTF-16 offsets of `text`, as the editor counts.
fn span_of(text: &str, line: u32, column: usize, word: &str) -> (usize, usize) {
    let content = text.split('\n').nth(line.saturating_sub(1) as usize).unwrap_or("");
    let column = column.min(content.len());
    let word = word.trim_matches(|c: char| !c.is_alphanumeric());
    let found = (!word.is_empty())
        .then(|| content.get(column..).and_then(|rest| rest.find(word)).map(|i| column + i).or_else(|| content.find(word)))
        .flatten();
    let (start, end) = match found {
        Some(i) => (i, i + word.len()),
        None => {
            let rest = content.get(column..).unwrap_or("");
            let len = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            (column, column + len)
        }
    };
    (offset_of(text, line, start), offset_of(text, line, end))
}

/// A place in a PDF: its page (from 1) and a point of it in PDF points from the page's top left, as SyncTeX takes it.
#[derive(serde::Deserialize, Clone, Copy)]
pub struct At {
    page: u32,
    x: f64,
    y: f64,
}

/// The beside-file of `pdf` with `suffix` (`.srcmap.json`, `.synctex.gz`), if the build left one.
fn beside(pdf: &Path, suffix: &str) -> Option<PathBuf> {
    let file = pdf.with_file_name(format!("{}{suffix}", pdf.file_stem()?.to_string_lossy()));
    file.is_file().then_some(file)
}

/// The source file and line (1-based) of the place `at` of `pdf`, from the map the build left beside it; none
/// without one, or when it says nothing there.
fn exact(pdf: &Path, at: At) -> Option<(PathBuf, u32)> {
    if beside(pdf, ".srcmap.json").is_some() {
        let texmark = crate::build::find_tool("texmark").ok()?;
        let out = std::process::Command::new(texmark)
            .args(["sync", "backward", "--pdf"])
            .arg(pdf)
            .args([at.page.to_string(), at.x.to_string(), at.y.to_string()])
            .env("PATH", crate::build::search_path(None))
            .output()
            .ok()?;
        let found: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
        let file = PathBuf::from(found.get("file")?.as_str()?);
        let line = found.get("line")?.as_u64()? as u32;
        return file.is_file().then_some((file, line));
    }
    beside(pdf, ".synctex.gz")?;
    let synctex = crate::build::find_tool("synctex").ok()?;
    let out = std::process::Command::new(synctex)
        .arg("edit")
        .arg("-o")
        .arg(format!("{}:{}:{}:{}", at.page, at.x, at.y, pdf.display()))
        .env("PATH", crate::build::search_path(None))
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let field = |name: &str| text.lines().find_map(|l| l.strip_prefix(name)).map(str::trim);
    let file = PathBuf::from(field("Input:")?);
    let file = if file.is_absolute() { file } else { pdf.parent()?.join(file) };
    let line: u32 = field("Line:")?.parse().ok()?;
    (file.is_file() && line > 0).then_some((file, line))
}

/// A box of a PDF: its page, its left and baseline in PDF points from the page's top left, its size.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy)]
pub struct Box {
    page: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Where line `line` (1-based) of `source` is in `pdf`, from the map the build left beside it: texmark's
/// (`texmark sync forward`) for a Markdown source, SyncTeX's (`synctex view`) for a LaTeX one; none without one.
#[tauri::command]
pub async fn pdf_boxes(pdf: String, source: String, line: u32) -> Option<Vec<Box>> {
    tauri::async_runtime::spawn_blocking(move || {
        let pdf = Path::new(&pdf);
        if beside(pdf, ".srcmap.json").is_some() && !source.ends_with(".tex") {
            let texmark = crate::build::find_tool("texmark").ok()?;
            let out = std::process::Command::new(texmark)
                .args(["sync", "forward", &format!("{source}:{line}"), "--pdf"])
                .arg(pdf)
                .env("PATH", crate::build::search_path(None))
                .output()
                .ok()?;
            let found: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
            let boxes = if found.is_array() { found } else { serde_json::Value::Array(vec![found]) };
            return serde_json::from_value(boxes).ok();
        }
        beside(pdf, ".synctex.gz")?;
        let synctex = crate::build::find_tool("synctex").ok()?;
        let out = std::process::Command::new(synctex)
            .args(["view", "-i", &format!("{line}:0:{source}"), "-o"])
            .arg(pdf)
            .env("PATH", crate::build::search_path(None))
            .output()
            .ok()?;
        // records of `Page:`, `x:`, `y:`, `W:`, `H:` lines (`h:` and `v:` before them are the same, unrounded)
        let text = String::from_utf8_lossy(&out.stdout);
        let mut boxes = Vec::new();
        let mut page = None;
        let mut fields = std::collections::HashMap::new();
        for l in text.lines() {
            let Some((k, v)) = l.split_once(':') else { continue };
            match k {
                "Page" => page = v.trim().parse().ok(),
                "h" | "v" | "W" | "H" => {
                    fields.insert(k, v.trim().parse::<f64>().unwrap_or(0.0));
                }
                _ => {}
            }
            if k == "H"
                && let (Some(page), Some(&x), Some(&y), Some(&w), Some(&h)) = (page, fields.get("h"), fields.get("v"), fields.get("W"), fields.get("H"))
            {
                boxes.push(Box { page, x, y, width: w, height: h });
            }
        }
        (!boxes.is_empty()).then_some(boxes)
    })
    .await
    .ok()
    .flatten()
}

/// ⌃-click in a PDF: the source of the place clicked opened at it (as the sidebar's search opens a match), exactly
/// from the build's map when there is one, else by the words clicked; false when it is found in none of the sources.
#[tauri::command]
pub async fn open_source_at(app: AppHandle, pdf: String, text: String, at: Option<At>, word: Option<String>) -> Result<bool, String> {
    let word = word.unwrap_or_default();
    let found = tauri::async_runtime::spawn_blocking(move || {
        let pdf = Path::new(&pdf);
        if let Some((file, line)) = at.and_then(|at| exact(pdf, at)) {
            let source = std::fs::read_to_string(&file).unwrap_or_default();
            let (from, to) = span_of(&source, line, 0, &word);
            return Some((file, from, to));
        }
        find(pdf, &text, &word)
    })
    .await
    .map_err(|e| e.to_string())?;
    match found {
        Some((path, from, to)) => {
            crate::sidebar::open_marked(&app, &path, from, to)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_word_clicked_is_marked_where_it_is_in_the_line_found() {
        let text = "intro\nThe sea level rises, as the sea warms.\n";
        let line2 = 6;
        // after the run found (from its column), else from the start
        assert_eq!(span_of(text, 2, "The sea level rises, as ".len(), "sea"), (line2 + 28, line2 + 31));
        assert_eq!(span_of(text, 2, 0, "level"), (line2 + 8, line2 + 13));
        // a word not there: the word at the column
        assert_eq!(span_of(text, 2, 4, "ocean"), (line2 + 4, line2 + 7));
        // punctuation around the word clicked is not its
        assert_eq!(span_of(text, 2, 0, "rises,"), (line2 + 14, line2 + 19));
    }

    #[test]
    fn offsets_count_utf16_up_to_the_column() {
        let text = "é first\nsecond line\nthird";
        assert_eq!(offset_of(text, 1, 0), 0);
        assert_eq!(offset_of(text, 2, 0), 8);
        assert_eq!(offset_of(text, 2, 7), 15);
        assert_eq!(offset_of(text, 1, "é ".len()), 2);
    }
}
