//! From a PDF to its source: the text clicked in a PDF (⌃-click in the pdf.js view) found in the Markdown and LaTeX
//! files of the PDF's project, and that file opened with the cursor there. The PDF is not asked how it was built: its
//! words are searched in every source file (`write_core::srcmap::find_text`), so chapters, `\include`, `\includeonly`
//! and draft passes need nothing of their own. Where several places match as well (an older copy of a paragraph in
//! notes), the most recently changed file is taken.

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
pub fn find(pdf: &Path, text: &str) -> Option<(PathBuf, usize)> {
    let root = root_of(pdf)?;
    let files = sources(&root);
    let texts: Vec<String> = files.iter().map(|f| std::fs::read_to_string(f).unwrap_or_default()).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let places = srcmap::find_text(&refs, text);
    let modified = |p: &Place| std::fs::metadata(&files[p.text]).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
    let place = places.iter().max_by_key(|p| modified(p))?;
    Some((files[place.text].clone(), offset_of(&texts[place.text], place.line, place.column)))
}

/// ⌃-click in a PDF: the source of the text clicked opened at it (as the sidebar's search opens a match); false
/// when it is found in none of the project's sources.
#[tauri::command]
pub async fn open_source_at(app: AppHandle, pdf: String, text: String) -> Result<bool, String> {
    let found = tauri::async_runtime::spawn_blocking(move || find(Path::new(&pdf), &text)).await.map_err(|e| e.to_string())?;
    match found {
        Some((path, at)) => {
            crate::sidebar::open_at(app, path.to_string_lossy().into_owned(), at, at)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_count_utf16_up_to_the_column() {
        let text = "é first\nsecond line\nthird";
        assert_eq!(offset_of(text, 1, 0), 0);
        assert_eq!(offset_of(text, 2, 0), 8);
        assert_eq!(offset_of(text, 2, 7), 15);
        assert_eq!(offset_of(text, 1, "é ".len()), 2);
    }
}
