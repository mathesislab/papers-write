//! The references of each document window: the `.bib` files its front matter names and the papers library (or
//! the library its project names in `write.toml`)
//! (`write_core::refs`), loaded when first asked (not as the window opens), kept while the window is open, and
//! read again when one of the files changes or when the front matter names other files (the page sends its text
//! as typed). The page is told (`references-changed`) after a reload it did not ask for.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use papers_core::{BibWatch, Found, Item};
use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, Manager, WebviewWindow};
use write_core::bib::{self, Cited};
use write_core::refs::{References, bibliography_of, project_library_of, sources_in, sources_of};

/// The references of each window, by its label. A window's slot is made when its references are first asked and
/// filled under its own lock, so a second search waits for the load instead of repeating it.
#[derive(Default)]
pub struct Loaded(Mutex<HashMap<String, Arc<Mutex<Option<Slot>>>>>);

/// The references of a window, and what they were read for.
pub struct Slot {
    /// the file of the document the references were read for (none: untitled)
    path: Option<PathBuf>,
    /// the `bibliography` of its front matter, as typed
    named: Vec<String>,
    /// the library its project names (`write.toml`), if it does
    library: Option<PathBuf>,
    references: Arc<References>,
    _watches: Vec<BibWatch>,
}

impl Slot {
    fn new(app: &AppHandle, label: &str, path: Option<PathBuf>, text: &str) -> Slot {
        // untitled: the papers library only
        let library = path.as_deref().and_then(project_library_of);
        let sources = match &path {
            Some(p) => sources_in(p, text, library.as_deref()),
            None => sources_of(Path::new(""), ""),
        };
        let watches = sources
            .iter()
            .filter_map(|bib| {
                let (handle, label) = (app.clone(), label.to_string());
                BibWatch::new(bib.clone(), move || reload(&handle, &label))
                    .map_err(|e| eprintln!("{}: not watched ({e})", bib.display()))
                    .ok()
            })
            .collect();
        Slot {
            named: path.as_deref().map(|p| bibliography_of(p, text)).unwrap_or_default(),
            path,
            library,
            references: Arc::new(References::load(sources)),
            _watches: watches,
        }
    }
}

/// The references of `window` for the document as typed (`text`), loaded or read again if they are not those of
/// its file and front matter.
fn references_of(window: &WebviewWindow, text: &str) -> Arc<References> {
    let app = window.app_handle();
    let label = window.label();
    let path = crate::documents::path_of(app, label);
    let slot = app.state::<Loaded>().0.lock().unwrap().entry(label.to_string()).or_default().clone();
    let mut slot = slot.lock().unwrap();
    let named = path.as_deref().map(|p| bibliography_of(p, text)).unwrap_or_default();
    let library = path.as_deref().and_then(project_library_of);
    let fresh = slot.as_ref().is_some_and(|s| s.path == path && s.named == named && s.library == library);
    if !fresh {
        let before = slot.is_some();
        *slot = Some(Slot::new(app, label, path, text));
        if before {
            tell(app, label);
        }
    }
    slot.as_ref().expect("just filled").references.clone()
}

/// A `.bib` file changed: the references are read again from the same files, and the page told.
fn reload(app: &AppHandle, label: &str) {
    let Some(slot) = app.state::<Loaded>().0.lock().unwrap().get(label).cloned() else {
        return;
    };
    {
        let mut slot = slot.lock().unwrap();
        let Some(slot) = slot.as_mut() else {
            return;
        };
        slot.references = Arc::new(References::load(slot.references.sources().to_vec()));
    }
    tell(app, label);
}

fn tell(app: &AppHandle, label: &str) {
    let _ = app.emit_to(EventTarget::webview(label), "references-changed", ());
}

/// A window gone: its references go.
pub fn unload(app: &AppHandle, label: &str) {
    app.state::<Loaded>().0.lock().unwrap().remove(label);
}

/// The papers matching `query`, at most `limit`.
#[tauri::command]
pub async fn references(window: WebviewWindow, query: String, limit: usize, text: String) -> Result<Found, String> {
    let refs = references_of(&window, &text);
    refs.error().map_or(Ok(()), |e| Err(e.to_string()))?;
    Ok(refs.search(&query, limit))
}

/// The papers closest to `selected` (a reference as written, a title), at most `limit`.
#[tauri::command]
pub async fn closest_references(window: WebviewWindow, selected: String, limit: usize, text: String) -> Result<Found, String> {
    let refs = references_of(&window, &text);
    refs.error().map_or(Ok(()), |e| Err(e.to_string()))?;
    Ok(refs.closest(&selected, limit))
}

/// The `.bib` files the references are read from.
#[tauri::command]
pub async fn reference_sources(window: WebviewWindow, text: String) -> Vec<String> {
    references_of(&window, &text).sources().iter().map(|p| p.to_string_lossy().into_owned()).collect()
}

/// The key cited in the document: in its `.bib`, copied there from the references if it is not.
#[tauri::command]
pub async fn cite_key(window: WebviewWindow, key: String, text: String) -> Result<Cited, String> {
    let Some(path) = crate::documents::path_of(window.app_handle(), window.label()) else {
        return Err("Save the document first, to give it a bibliography".into());
    };
    let refs = references_of(&window, &text);
    bib::cite(&path, &text, &key, &refs).map_err(|e| e.to_string())
}

/// What the citations of the text and the document's own bibliography do not match.
#[derive(Serialize)]
pub struct Check {
    /// keys cited that the bibliography does not have
    missing: Vec<String>,
    /// keys of the bibliography never cited
    unused: Vec<String>,
}

/// The `.bib` files the front matter of `text` names, beside the window's document, that are there (none for an
/// untitled one): the document's own bibliography, not the papers library.
pub(crate) fn own_bibs(window: &WebviewWindow, text: &str) -> Vec<PathBuf> {
    let Some(path) = crate::documents::path_of(window.app_handle(), window.label()) else {
        return Vec::new();
    };
    let folder = path.parent().map(Path::to_path_buf).filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| PathBuf::from("."));
    bibliography_of(&path, text).into_iter().map(|n| folder.join(n)).filter(|f| f.is_file()).collect()
}

/// The references offered for a citation key typed so far: the document's own first, then the keys that start
/// with it, then the other matches (`write_core::refs::candidates`).
#[tauri::command]
pub async fn cite_options(window: WebviewWindow, prefix: String, limit: usize, text: String) -> Vec<write_core::refs::Candidate> {
    let refs = references_of(&window, &text);
    write_core::refs::candidates(&refs, &prefix, &own_bibs(&window, &text), limit)
}

/// The reference of citation key `key` (its author, year, title, container), from the document's bibliography or the
/// library: what the editor shows on hovering a key. None when no file has it.
#[tauri::command]
pub async fn reference_of(window: WebviewWindow, key: String, text: String) -> Option<Item> {
    references_of(&window, &text).entry(&key).map(|e| e.item)
}

/// The keys cited in `text` against those of the `.bib` files its front matter names (not the papers library).
#[tauri::command]
pub async fn bib_check(window: WebviewWindow, text: String) -> Result<Check, String> {
    let own = References::load(own_bibs(&window, &text));
    if let Some(e) = own.error() {
        return Err(e.to_string());
    }
    let keys: Vec<String> = own.library().map(|l| l.items().iter().map(|i| i.key.clone()).collect()).unwrap_or_default();
    // a .tex file cites with commands only (an @ in it is no citation)
    let tex = crate::documents::path_of(window.app_handle(), window.label()).is_some_and(|p| write_core::latex::is_latex(&p));
    if tex {
        let cited = write_core::latex::cited_keys(&text);
        let missing = cited.iter().filter(|k| !keys.contains(k)).cloned().collect();
        let unused = keys.iter().filter(|k| !cited.contains(k)).cloned().collect();
        return Ok(Check { missing, unused });
    }
    Ok(Check { missing: bib::missing(&text, &keys), unused: bib::unused(&text, &keys) })
}
