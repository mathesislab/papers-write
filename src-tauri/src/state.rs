//! What the app keeps between sessions, in its data folder (`state.json`): the projects opened last, the projects
//! of the project bar, the tabs of each project as last left, and the windows open when the app quit (their
//! projects, to open again at the next start). Not in `write.toml`: what the app shows is not the project's.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE: &str = "state.json";
/// The most projects kept among the recent ones.
const RECENT: usize = 20;
/// The most of them in File ▸ Open Recent.
pub const MENU: usize = 10;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    /// the projects opened, the last first
    pub recent: Vec<PathBuf>,
    /// the projects of the project bar, in order
    pub bar: Vec<PathBuf>,
    /// the tabs of each project as last left
    pub layouts: BTreeMap<PathBuf, Layout>,
    /// the windows open as the app quit, to open again at the next start
    pub windows: Vec<SavedWindow>,
    /// the sidebar of each project, and the last one changed (`sidebars` and `sidebar` in the file)
    #[serde(flatten)]
    pub sidebars: Sidebars,
}

/// A project's tabs, the projects' sidebars, and a window's projects, as papers-tabs saves them.
pub use crate::tabs::{Layout, SavedWindow, Sidebars};

impl Saved {
    /// What `dir` keeps; nothing when the file is missing or broken.
    pub fn load(dir: &Path) -> Saved {
        fs::read_to_string(dir.join(FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Written to `dir` atomically; what went wrong is told on stderr, as nothing depends on it.
    pub fn save(&self, dir: &Path) {
        let written = fs::create_dir_all(dir).and_then(|()| {
            let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
            crate::files::write(&dir.join(FILE), &text)
        });
        if let Err(e) = written {
            eprintln!("{}: {e}", dir.join(FILE).display());
        }
    }

    /// A project opened: first of the recent ones, and in the bar (at the end, unless it is there).
    pub fn opened(&mut self, root: &Path) {
        self.recent.retain(|r| r != root);
        self.recent.insert(0, root.to_path_buf());
        self.recent.truncate(RECENT);
        if !self.bar.iter().any(|b| b == root) {
            self.bar.push(root.to_path_buf());
        }
    }

    /// The projects of File ▸ Open Recent: the last opened first, at most [`MENU`].
    pub fn for_menu(&self) -> &[PathBuf] {
        &self.recent[..self.recent.len().min(MENU)]
    }

    /// Open Recent ▸ Clear Menu: the recent projects forgotten (the bar stays).
    pub fn clear_recent(&mut self) {
        self.recent.clear();
    }

    /// A project out of the bar; it stays among the recent ones.
    pub fn removed(&mut self, root: &Path) {
        self.bar.retain(|b| b != root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sidebars_are_kept_as_two_fields_of_the_file() {
        let json = r#"{"sidebars":{"/a":{"shown":false,"width":300.0,"tab":"outline"}},"sidebar":{"shown":true,"width":250.0,"tab":null}}"#;
        let saved: Saved = serde_json::from_str(json).unwrap();
        assert_eq!(saved.sidebars.sidebars[Path::new("/a")].width, 300.0);
        assert_eq!(saved.sidebars.sidebar.width, 250.0);
        let back = serde_json::to_value(&saved).unwrap();
        assert_eq!(back["sidebars"]["/a"]["tab"], "outline");
        assert_eq!(back["sidebar"]["width"], 250.0);
    }

    #[test]
    fn opened_projects_come_first_once_and_join_the_bar_at_the_end() {
        let mut saved = Saved::default();
        let (a, b) = (Path::new("/a"), Path::new("/b"));
        saved.opened(a);
        saved.opened(b);
        saved.opened(a);
        assert_eq!(saved.recent, [a, b]);
        assert_eq!(saved.bar, [a, b]);
        saved.removed(a);
        assert_eq!(saved.bar, [b]);
        assert_eq!(saved.recent, [a, b]);
    }

    #[test]
    fn the_menu_has_the_last_ten_most_recent_first_and_clearing_it_leaves_the_bar() {
        let mut saved = Saved::default();
        for i in 0..15 {
            saved.opened(Path::new(&format!("/p{i}")));
        }
        let menu = saved.for_menu();
        assert_eq!(menu.len(), MENU);
        assert_eq!((menu[0].as_path(), menu[9].as_path()), (Path::new("/p14"), Path::new("/p5")));
        saved.opened(Path::new("/p0"));
        assert_eq!(saved.for_menu()[0], Path::new("/p0"));
        saved.clear_recent();
        assert!(saved.for_menu().is_empty());
        assert_eq!(saved.bar.len(), 15);
    }

    #[test]
    fn saved_and_loaded_back_a_broken_file_is_nothing() {
        let dir = std::env::temp_dir().join(format!("papers-write-state-{}", std::process::id()));
        let mut saved = Saved::default();
        saved.opened(Path::new("/p"));
        saved.layouts.insert("/p".into(), Layout { files: vec!["/p/a.md".into(), "/p/b.md".into()], front: Some("/p/b.md".into()) });
        saved.windows.push(SavedWindow { projects: vec!["/p".into()] });
        saved.save(&dir);
        assert_eq!(Saved::load(&dir), saved);
        fs::write(dir.join(FILE), "{").unwrap();
        assert_eq!(Saved::load(&dir), Saved::default());
        // a file of a later version, with keys this one does not know
        fs::write(dir.join(FILE), r#"{"bar": ["/q"], "later": {}}"#).unwrap();
        assert_eq!(Saved::load(&dir).bar, [Path::new("/q")]);
        let _ = fs::remove_dir_all(&dir);
    }
}
