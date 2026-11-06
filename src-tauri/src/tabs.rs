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
    fn style(&self) -> WindowStyle {
        WindowStyle { title: "Papers Write", size: (1000.0, 720.0), least: (520.0, 360.0) }
    }

    fn path(&self, app: &AppHandle, page: &str) -> Option<PathBuf> {
        crate::documents::path_of(app, page)
    }

    fn title(&self, app: &AppHandle, page: &str) -> String {
        crate::documents::title(crate::documents::path_of(app, page).as_deref())
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
        let main = write_core::project::Project::read(root).ok().and_then(|p| p.main());
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
    }

    fn closed(&self, app: &AppHandle, page: &str) {
        crate::documents::closed(app, page);
        crate::git::closed(app, page);
        crate::spelling::closed(app, page);
    }

    fn none_left(&self, app: &AppHandle) {
        crate::documents::regular(app, false);
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
}
