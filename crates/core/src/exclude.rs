//! What Go to File and Find in Project leave out of a project's files (the sidebar and Open… show them all): hidden
//! files, the usual output folders (`build*`, `_build`, `dist`, `node_modules`, `target`, `__pycache__`, `venv`, and
//! the project's own build folder), what its `.gitignore` ignores (unless `write.toml` says `gitignore = false`), and
//! the paths of its `hide`; the paths of its `show` brought back, wherever they are. Patterns are gitignore's: a name
//! alone matches at any depth (`*.log`), one with a `/` from the project's folder (`data/raw/`).

use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::project::Project;

/// The output and cache folders left out unless shown.
const DEFAULTS: &[&str] = &["build*", "_build", "dist", "node_modules", "target", "__pycache__", "venv"];

pub struct Exclusions {
    gitignore: Option<Gitignore>,
    hide: GlobSet,
    show: GlobSet,
    /// the folders the `show` patterns go into, for those to be walked though hidden
    show_folders: Vec<String>,
    /// a `show` pattern that can be anywhere (`**/…`, or a name alone): every folder walked
    show_anywhere: bool,
}

/// A pattern as globs on a path from the project's folder: the path itself and what is inside it.
fn globs(pattern: &str) -> Vec<String> {
    let p = pattern.trim().trim_end_matches('/');
    if p.is_empty() {
        return Vec::new();
    }
    let anchored = p.starts_with('/') || p.contains('/');
    let p = p.trim_start_matches('/');
    let base = if anchored { p.to_string() } else { format!("**/{p}") };
    vec![base.clone(), format!("{base}/**")]
}

fn set(patterns: &[String]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for g in patterns.iter().flat_map(|p| globs(p)) {
        if let Ok(g) = Glob::new(&g) {
            b.add(g);
        }
    }
    b.build().unwrap_or_else(|_| GlobSet::empty())
}

impl Exclusions {
    /// What is left out of the project at `root`, as its `write.toml` says (the defaults without one).
    pub fn of(root: &Path) -> Exclusions {
        let manifest = Project::read(root).ok().map(|p| p.manifest).unwrap_or_default();
        let mut hide: Vec<String> = DEFAULTS.iter().map(|s| s.to_string()).collect();
        if let Some(build) = &manifest.build {
            hide.push(format!("/{}", build.to_string_lossy()));
        }
        hide.extend(manifest.hide.iter().cloned());
        let gitignore = manifest.gitignore.unwrap_or(true).then(|| {
            let mut b = GitignoreBuilder::new(root);
            b.add(root.join(".gitignore"));
            b.add(root.join(".git/info/exclude"));
            b.build().ok()
        });
        let show_folders = manifest.show.iter().filter(|p| p.contains('/')).map(|p| p.trim_start_matches('/').to_string()).collect();
        let show_anywhere = manifest.show.iter().any(|p| p.starts_with("**") || !p.trim_end_matches('/').contains('/'));
        Exclusions { gitignore: gitignore.flatten(), hide: set(&hide), show: set(&manifest.show), show_folders, show_anywhere }
    }

    /// Whether `path` (`relative` from the project's folder) is left out; a folder left out may still be walked for
    /// what `show` brings back in it (`walked`).
    pub fn hidden(&self, path: &Path, relative: &Path, is_dir: bool) -> bool {
        let dot = relative.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.'));
        let left = dot
            || self.hide.is_match(relative)
            || self.gitignore.as_ref().is_some_and(|g| g.matched_path_or_any_parents(path, is_dir).is_ignore());
        left && !self.show.is_match(relative)
    }

    /// Whether a folder left out is walked all the same: something `show` brings back may be in it.
    pub fn walked(&self, relative: &Path) -> bool {
        let r = relative.to_string_lossy().replace('\\', "/");
        self.show_anywhere || self.show_folders.iter().any(|s| s.starts_with(&format!("{r}/")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("write-core-exclude-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_defaults_the_gitignore_and_the_project_s_own() {
        let root = folder("all");
        fs::write(root.join(".gitignore"), "*.log\nscratch/\n").unwrap();
        fs::write(root.join("write.toml"), "hide = [\"data/raw/\"]\nshow = [\"build-full/thesis.pdf\"]\n").unwrap();
        let ex = Exclusions::of(&root);
        let hidden = |r: &str, dir: bool| ex.hidden(&root.join(r), Path::new(r), dir);
        assert!(!hidden("paper.md", false));
        assert!(hidden("build", true) && hidden("build-full/x.aux", false) && hidden("node_modules", true));
        assert!(hidden("out.log", false) && hidden("scratch", true));
        assert!(hidden("data/raw", true) && !hidden("data/clean.csv", false));
        assert!(hidden(".git", true));
        // brought back, and its folder walked for it
        assert!(!hidden("build-full/thesis.pdf", false));
        assert!(ex.walked(Path::new("build-full")) && !ex.walked(Path::new("build")));
    }

    #[test]
    fn the_gitignore_not_followed_when_the_project_says_so() {
        let root = folder("nogit");
        fs::write(root.join(".gitignore"), "*.log\n").unwrap();
        fs::write(root.join("write.toml"), "gitignore = false\n").unwrap();
        let ex = Exclusions::of(&root);
        assert!(!ex.hidden(&root.join("out.log"), Path::new("out.log"), false));
    }
}
