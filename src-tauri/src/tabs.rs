//! The windows and their tabs: papers-tabs (`crates/tabs`, shared with Papers Review), given what Papers Write's
//! pages are. A page is a document's (`doc-N`, documents.rs) or a history's (`history-N`, git.rs); its project is the
//! folder of the `write.toml` above its file. With Settings ▸ Open files as tabs off, each page has a window of its
//! own.

use std::path::{Path, PathBuf};

use tauri::AppHandle;

pub use papers_tabs::*;

/// What Papers Write says of its pages to papers-tabs.
pub struct Write;

impl Host for Write {
    fn background(&self, _app: &AppHandle, dark: bool) -> Option<tauri::window::Color> {
        use crate::settings::Appearance;
        // the pages' --bg (theme.css), light or dark as Settings ▸ Window ▸ Appearance says (System: as the window is)
        let dark = match crate::settings::get().appearance {
            Appearance::System => dark,
            Appearance::Light => false,
            Appearance::Dark => true,
        };
        Some(if dark { tauri::window::Color(0x26, 0x26, 0x28, 0xff) } else { tauri::window::Color(0xf6, 0xf6, 0xf8, 0xff) })
    }

    fn style(&self) -> WindowStyle {
        WindowStyle { title: "Papers Write", size: (1000.0, 720.0), least: (520.0, 360.0) }
    }

    fn path(&self, app: &AppHandle, page: &str) -> Option<PathBuf> {
        crate::documents::path_of(app, page)
    }

    fn title(&self, app: &AppHandle, page: &str) -> String {
        crate::documents::title(crate::documents::path_of(app, page).as_deref())
    }

    fn edited(&self, app: &AppHandle, page: &str) -> bool {
        // changes typed that are not saved yet
        crate::documents::is_modified(app, page)
    }

    fn project_of(&self, _app: &AppHandle, file: &Path) -> Option<PathBuf> {
        write_core::project::find(file)
    }

    fn windows(&self, _app: &AppHandle) -> Windows {
        if crate::settings::get().tabs { Windows::Shared } else { Windows::PerPage }
    }

    fn open(&self, app: &AppHandle, _root: &Path, file: Option<&Path>) -> Result<String, String> {
        crate::documents::make(app, file.map(Path::to_path_buf)).map_err(|e| e.to_string())
    }

    fn first_file(&self, _app: &AppHandle, root: &Path) -> Result<Option<PathBuf>, String> {
        let project = write_core::project::Project::read(root).ok();
        // a folder of notes: its note changed last, else none (an empty page, which shows the search of the notes)
        if project.as_ref().is_some_and(|p| p.manifest.kind == Some(write_core::project::Kind::Notes)) {
            return Ok(write_core::notes::search(root, "").into_iter().next().map(|n| n.path));
        }
        let main = project.and_then(|p| p.main());
        match main {
            Some(main) => Ok(Some(main)),
            None => Err(format!("{}: no main document (Project Info…)", root.display())),
        }
    }

    fn asks_before_closing(&self, app: &AppHandle, page: &str) -> bool {
        crate::documents::asks_before_closing(app, page)
    }

    fn focused(&self, app: &AppHandle, page: &str) {
        crate::documents::focused(app, page);
        // Build ▸ Build With and In the Background, for the project of this tab
        crate::profiles::menu_lists(app);
    }

    fn unload_policy(&self, _app: &AppHandle) -> Option<papers_tabs::Unloading> {
        let s = crate::settings::get();
        Some(papers_tabs::Unloading {
            idle: (s.unload_after > 0).then(|| std::time::Duration::from_secs(u64::from(s.unload_after) * 60)),
            most: s.loaded_tabs.max(1) as usize,
            memory: (s.tabs_memory > 0).then(|| u64::from(s.tabs_memory) << 30),
        })
    }

    fn load_all_at_start(&self, _app: &AppHandle) -> bool {
        crate::settings::get().load_all_tabs
    }

    fn companion_kind(&self, _app: &AppHandle, page: &str) -> Option<String> {
        page.starts_with("preview-").then(|| "preview".to_string())
    }

    fn open_companion(&self, app: &AppHandle, beside: &str, kind: &str) -> Result<String, String> {
        match kind {
            // beside the text (View ▸ Panes): the text's page shows it there as it loads
            "preview" if matches!(crate::projects::with_saved(app, |saved| saved.panes.clone()).as_str(), "" | "side") => {
                Err("the preview is beside the text".into())
            }
            "preview" => crate::panetabs::open_preview(app, beside),
            _ => Err(format!("no tab of kind {kind}")),
        }
    }

    fn closed(&self, app: &AppHandle, page: &str) {
        // a tab closed (not the app quitting): its undo history goes
        if !crate::tabs::is_quitting(app)
            && let Some(path) = crate::documents::path_of(app, page)
        {
            crate::undo::drop_history(app, &path);
        }
        crate::documents::closed(app, page);
        crate::git::closed(app, page);
        crate::panetabs::closed(app, page);
        crate::spelling::closed(app, page);
    }

    fn layout(&self, app: &AppHandle, root: &Path) -> Layout {
        crate::projects::layout(app, root)
    }

    fn save_layouts(&self, app: &AppHandle, layouts: Vec<(PathBuf, Layout)>) {
        crate::projects::save_layouts(app, layouts);
    }

    fn saved_windows(&self, app: &AppHandle) -> Vec<SavedWindow> {
        crate::projects::saved_windows(app)
    }

    fn save_windows(&self, app: &AppHandle, windows: Vec<SavedWindow>) {
        crate::projects::save_windows(app, windows);
    }

    fn with_sidebars(&self, app: &AppHandle, f: &mut dyn FnMut(&mut Sidebars)) {
        crate::projects::with_sidebars(app, f);
    }
}
