//! The PDF shown beside a text (View ▸ Show PDF): which one, for each source file. Until the project says which PDF
//! its sources make (its build settings), it is chosen by hand and kept per source file in the app's state
//! (`state.json`: `pdfs`), not in the project; without a choice it is `<build>/<name>.pdf` when that exists. The
//! page is offered the project's PDFs to choose from, the most recently written first. The PDF shown is watched:
//! a change (a build, a watch process) comes to the page as `pdf-changed`.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;
use tauri::{AppHandle, Manager, Webview};
use tauri_plugin_dialog::DialogExt;
use write_core::project;

use crate::{documents, files, projects};

/// The most PDFs offered to choose from.
const OFFERED: usize = 30;

/// A PDF to choose: its path, its name relative to the project, when it was written (seconds since 1970).
#[derive(Serialize)]
pub struct Offered {
    path: String,
    name: String,
    modified: u64,
}

/// The PDF beside the page's text, if one is chosen or found, and the project's PDFs to choose from.
#[derive(Serialize)]
pub struct SidePdf {
    pdf: Option<String>,
    offered: Vec<Offered>,
}

/// The key of the watch of a window's side PDF (its own file is watched as its label).
fn key_of(label: &str) -> String {
    format!("{label}#pdf")
}

/// The PDF a source shows beside it: the one chosen, else `<build>/<name>.pdf` when it exists.
pub(crate) fn pdf_of(app: &AppHandle, source: &Path) -> Option<PathBuf> {
    let chosen = projects::with_saved(app, |saved| saved.pdfs.get(source).cloned());
    if let Some(pdf) = chosen.filter(|p| p.is_file()) {
        return Some(pdf);
    }
    // the PDF of the document's build profile
    if let Some(pdf) = crate::profiles::active(app, source).and_then(|(p, root)| p.pdf.map(|pdf| root.join(pdf))).filter(|p| p.is_file()) {
        return Some(pdf);
    }
    let stem = source.file_stem()?;
    let built = project::build_dir_of(source).join(stem).with_extension("pdf");
    built.is_file().then_some(built)
}

/// The PDFs of the folder `root` and of its folders (not hidden ones, three levels down), the most recent first.
fn pdfs_in(root: &Path) -> Vec<Offered> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<(PathBuf, SystemTime)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            match entry.file_type() {
                Ok(t) if t.is_dir() && depth < 3 && entry.file_name() != "node_modules" => walk(&path, depth + 1, out),
                Ok(t) if t.is_file() && documents::is_pdf(&path) => {
                    let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
                    out.push((path, modified));
                }
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    walk(root, 0, &mut found);
    found.sort_by_key(|f| std::cmp::Reverse(f.1));
    found
        .into_iter()
        .take(OFFERED)
        .map(|(path, modified)| Offered {
            name: path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned(),
            modified: modified.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            path: path.to_string_lossy().into_owned(),
        })
        .collect()
}

/// The PDF beside the window's text, watched; and the PDFs to choose from.
#[tauri::command]
pub fn side_pdf(window: Webview) -> Result<SidePdf, String> {
    let app = window.app_handle();
    let source = documents::path_of(app, window.label()).ok_or("Save the document first")?;
    let pdf = pdf_of(app, &source);
    match &pdf {
        Some(pdf) => files::watch_for(app, &key_of(window.label()), window.label(), pdf, "pdf-changed")?,
        None => files::unwatch(app, &key_of(window.label())),
    }
    let root = project::find(&source).unwrap_or_else(|| source.parent().unwrap_or(Path::new("/")).to_path_buf());
    Ok(SidePdf { pdf: pdf.map(|p| p.to_string_lossy().into_owned()), offered: pdfs_in(&root) })
}

/// The PDF to show beside the window's text from now on (none: back to the one found), kept for its source file.
#[tauri::command]
pub fn set_side_pdf(window: Webview, pdf: Option<String>) -> Result<SidePdf, String> {
    let app = window.app_handle();
    let source = documents::path_of(app, window.label()).ok_or("Save the document first")?;
    projects::with_saved(app, |saved| match pdf {
        Some(pdf) => saved.pdfs.insert(source, PathBuf::from(pdf)),
        None => saved.pdfs.remove(&source),
    });
    side_pdf(window)
}

/// Other…: a PDF chosen in a dialog, to show beside the window's text; none when the dialog is cancelled.
#[tauri::command]
pub async fn choose_side_pdf(window: Webview) -> Result<Option<SidePdf>, String> {
    let app = window.app_handle().clone();
    let source = documents::path_of(&app, window.label()).ok_or("Save the document first")?;
    let mut dialog = app.dialog().file().add_filter("PDF", &["pdf"]).set_title("PDF beside the text");
    if let Some(dir) = source.parent() {
        dialog = dialog.set_directory(dir);
    }
    let Some(chosen) = dialog.blocking_pick_file().and_then(|f| f.into_path().ok()) else {
        return Ok(None);
    };
    set_side_pdf(window, Some(chosen.to_string_lossy().into_owned())).map(Some)
}

/// The window's text no longer shows its PDF: its watch given up.
#[tauri::command]
pub fn hide_side_pdf(window: Webview) {
    files::unwatch(window.app_handle(), &key_of(window.label()));
}

/// A window gone: the watch of its PDF too.
pub fn closed(app: &AppHandle, label: &str) {
    files::unwatch(app, &key_of(label));
}
