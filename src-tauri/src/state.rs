//! What the app keeps between sessions, in its data folder (`state.json`): the projects opened last and the
//! projects of the project bar. Not in `write.toml`: what the app shows is not the project's.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE: &str = "state.json";
/// The most projects kept among the recent ones.
const RECENT: usize = 20;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    /// the projects opened, the last first
    pub recent: Vec<PathBuf>,
    /// the projects of the project bar, in order
    pub bar: Vec<PathBuf>,
}

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

    /// A project out of the bar; it stays among the recent ones.
    pub fn removed(&mut self, root: &Path) {
        self.bar.retain(|b| b != root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn saved_and_loaded_back_a_broken_file_is_nothing() {
        let dir = std::env::temp_dir().join(format!("papers-write-state-{}", std::process::id()));
        let mut saved = Saved::default();
        saved.opened(Path::new("/p"));
        saved.save(&dir);
        assert_eq!(Saved::load(&dir), saved);
        fs::write(dir.join(FILE), "{").unwrap();
        assert_eq!(Saved::load(&dir), Saved::default());
        // a file of a later version, with keys this one does not know
        fs::write(dir.join(FILE), r#"{"bar": ["/q"], "layouts": {}}"#).unwrap();
        assert_eq!(Saved::load(&dir).bar, [Path::new("/q")]);
        let _ = fs::remove_dir_all(&dir);
    }
}
