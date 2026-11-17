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
        // the pages' --bg (theme.css), light or dark as Settings ▸ Window ▸ Appearance says, else as the system is
        let dark = match crate::settings::get().appearance {
            crate::settings::Appearance::Light => false,
            crate::settings::Appearance::Dark => true,
            crate::settings::Appearance::System => dark,
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
        let title = crate::documents::title(crate::documents::path_of(app, page).as_deref());
        // changes typed that are not saved yet
        if crate::documents::is_modified(app, page) { format!("{title} *") } else { title }
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
    }

    fn closed(&self, app: &AppHandle, page: &str) {
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
