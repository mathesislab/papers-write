//! The tabs of the windows as plain data: a window's sets of tabs, one per project, the one shown first; each set's
//! pages in order and the one in front. What a window does when a tab comes, is chosen or goes, without the windows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A window's sets of tabs, the one shown first.
#[derive(Debug, Default)]
pub struct Win {
    pub sets: Vec<Set>,
}

/// The tabs of one project in a window (none: files outside any project): their pages in order, the one in front.
#[derive(Debug, Default)]
pub struct Set {
    pub project: Option<PathBuf>,
    pub pages: Vec<String>,
    pub front: Option<String>,
    /// the names of pages that are not files (a history)
    pub titles: HashMap<String, String>,
}

/// What is left of a window once a page went: the page to bring to the front (the one shown went), and whether no
/// tab is left.
#[derive(Debug, PartialEq)]
pub struct Removed {
    pub next: Option<String>,
    pub empty: bool,
}

impl Win {
    pub fn set_of(&self, page: &str) -> Option<usize> {
        self.sets.iter().position(|s| s.pages.iter().any(|p| p == page))
    }

    pub fn set_for(&self, project: Option<&Path>) -> Option<usize> {
        self.sets.iter().position(|s| s.project.as_deref() == project)
    }

    /// The set `i` shown first.
    pub fn raise(&mut self, i: usize) {
        let set = self.sets.remove(i);
        self.sets.insert(0, set);
    }

    pub fn pages(&self) -> impl Iterator<Item = &String> {
        self.sets.iter().flat_map(|s| s.pages.iter())
    }

    /// The project shown.
    pub fn shown(&self) -> Option<&Path> {
        self.sets.first()?.project.as_deref()
    }

    /// The set of `project` shown first, made when new; a loose file (no project) joins whatever is shown.
    pub fn show(&mut self, project: Option<&Path>) {
        let i = match project {
            None if !self.sets.is_empty() => 0,
            _ => match self.set_for(project) {
                Some(i) => i,
                None => {
                    self.sets.insert(0, Set { project: project.map(Path::to_path_buf), ..Set::default() });
                    0
                }
            },
        };
        self.raise(i);
    }

    /// A page as a tab of the set shown, after its tab in front; named `title` if it is not a file.
    pub fn insert(&mut self, page: &str, title: Option<String>) {
        if self.sets.is_empty() {
            self.sets.push(Set::default());
        }
        let set = &mut self.sets[0];
        let at = set.front.as_ref().and_then(|f| set.pages.iter().position(|p| p == f)).map_or(set.pages.len(), |i| i + 1);
        set.pages.insert(at, page.to_string());
        if let Some(title) = title {
            set.titles.insert(page.to_string(), title);
        }
    }

    /// A page to the front of its set, the set shown.
    pub fn bring(&mut self, page: &str) {
        if let Some(i) = self.set_of(page) {
            self.raise(i);
            self.sets[0].front = Some(page.to_string());
        }
    }

    /// A page gone: the tab after it in front of its set, else the one before; a set emptied goes, and with it the
    /// project shown, the next set shown then.
    pub fn remove(&mut self, page: &str) -> Removed {
        let Some(s) = self.set_of(page) else {
            return Removed { next: None, empty: self.sets.is_empty() };
        };
        let set = &mut self.sets[s];
        let i = set.pages.iter().position(|p| p == page).unwrap_or(0);
        let was_front = set.front.as_deref() == Some(page);
        set.pages.retain(|p| p != page);
        set.titles.remove(page);
        if was_front {
            set.front = set.pages.get(i).or_else(|| set.pages.last()).cloned();
        }
        let emptied = set.pages.is_empty();
        if emptied {
            self.sets.remove(s);
        }
        let next = if s == 0 && (was_front || emptied) { self.sets.first().and_then(|s| s.front.clone()) } else { None };
        Removed { next, empty: self.sets.is_empty() }
    }

    /// A set of loose files with `page` alone given to `project`, the file it now shows being of it.
    pub fn adopt(&mut self, page: &str, project: &Path) -> bool {
        match self.set_of(page) {
            Some(i) if self.sets[i].project.is_none() && self.sets[i].pages.len() == 1 => {
                self.sets[i].project = Some(project.to_path_buf());
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(sets: &[(&str, &[&str])]) -> Win {
        let mut win = Win::default();
        for (project, pages) in sets.iter().rev() {
            win.show(Some(Path::new(project)));
            for page in *pages {
                win.insert(page, None);
                win.bring(page);
            }
        }
        win
    }

    fn fronts(win: &Win) -> Vec<(Option<&Path>, Option<&str>)> {
        win.sets.iter().map(|s| (s.project.as_deref(), s.front.as_deref())).collect()
    }

    #[test]
    fn a_tab_comes_after_the_one_in_front() {
        let mut w = win(&[("/a", &["1", "2", "3"])]);
        w.bring("1");
        w.insert("4", None);
        assert_eq!(w.sets[0].pages, ["1", "4", "2", "3"]);
    }

    #[test]
    fn a_project_shown_raises_its_set_or_makes_one_and_a_loose_file_joins_the_one_shown() {
        let mut w = win(&[("/a", &["1"]), ("/b", &["2"])]);
        assert_eq!(w.shown(), Some(Path::new("/a")));
        w.show(Some(Path::new("/b")));
        assert_eq!(w.shown(), Some(Path::new("/b")));
        w.show(None);
        assert_eq!(w.shown(), Some(Path::new("/b")));
        w.show(Some(Path::new("/c")));
        assert_eq!(w.sets.len(), 3);
        assert_eq!(w.shown(), Some(Path::new("/c")));
    }

    #[test]
    fn bringing_a_tab_shows_its_set() {
        let mut w = win(&[("/a", &["1"]), ("/b", &["2", "3"])]);
        w.bring("2");
        assert_eq!(fronts(&w), [(Some(Path::new("/b")), Some("2")), (Some(Path::new("/a")), Some("1"))]);
    }

    #[test]
    fn the_tab_in_front_closed_gives_the_front_to_the_next_else_the_one_before() {
        let mut w = win(&[("/a", &["1", "2", "3"])]);
        w.bring("2");
        assert_eq!(w.remove("2"), Removed { next: Some("3".into()), empty: false });
        assert_eq!(w.remove("3"), Removed { next: Some("1".into()), empty: false });
    }

    #[test]
    fn a_tab_behind_closed_changes_nothing_shown() {
        let mut w = win(&[("/a", &["1", "2"]), ("/b", &["3"])]);
        assert_eq!(w.remove("1"), Removed { next: None, empty: false });
        assert_eq!(w.remove("3"), Removed { next: None, empty: false });
        assert_eq!(w.sets.len(), 1);
    }

    #[test]
    fn the_last_tab_of_the_project_shown_closed_shows_the_project_before_and_the_last_of_all_empties_the_window() {
        let mut w = win(&[("/a", &["1"]), ("/b", &["2"])]);
        assert_eq!(w.remove("1"), Removed { next: Some("2".into()), empty: false });
        assert_eq!(w.shown(), Some(Path::new("/b")));
        assert_eq!(w.remove("2"), Removed { next: None, empty: true });
    }

    #[test]
    fn a_loose_tab_alone_is_adopted_by_the_project_of_its_file() {
        let mut w = Win::default();
        w.show(None);
        w.insert("1", None);
        assert!(w.adopt("1", Path::new("/a")));
        assert_eq!(w.shown(), Some(Path::new("/a")));
        w.insert("2", None);
        assert!(!w.adopt("2", Path::new("/b")));
    }
}
