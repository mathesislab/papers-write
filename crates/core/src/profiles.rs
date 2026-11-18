//! A project's build profiles: what Build does, as text files of the project (`.papers-write/build/<name>.toml`),
//! committed with it. One is active at a time (the app keeps which); with none, Build builds the document in front as
//! it always has (texmark, Quarto or latexmk, from the document's own front matter). A profile says a command to run
//! in the project's folder (texmark with options of its own, `make synopsis`, …), the PDF it makes, and how it runs:
//!
//! ```toml
//! command = "make synopsis"
//! pdf = "build/synopsis.pdf"
//! run = "build"     # Build runs it; "watch": started and kept running (Build again: stopped); "save": run after the
//!                   # project's files are saved (Build turns that on and off); "external": never run, its PDF followed
//! ```
//!
//! The command and the PDF may name the document they are run for (the one in front; for a profile run in the
//! background, the one it was chosen from): `{file}` (its path from the project's folder), `{stem}` (its name without
//! the extension), `{dir}` (its folder from the project's), and in the command `{pdf}` (the `pdf` said, filled in
//! likewise). In the command they are quoted for the shell as needed:
//!
//! ```toml
//! command = "texmark {file} --pdf -o {pdf}"
//! pdf = "build/{stem}.pdf"
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The folder of a project's profiles.
pub fn dir_of(root: &Path) -> PathBuf {
    root.join(".papers-write").join("build")
}

/// How a profile runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Run {
    /// Build runs the command, and waits for it
    #[default]
    Build,
    /// Build starts the command (`make watch`, `texmark --watch`) and leaves it running till Build again or the app
    /// quits; the same command running already in the project's folder (started in a terminal) is followed instead
    Watch,
    /// Build turns on (and again off) running the command after the project's files are saved, once they have been
    /// quiet a few seconds (autosave), for tools without a watch of their own
    Save,
    /// the command is run elsewhere (a terminal): Build only shows the PDF
    External,
}

/// A profile as its file says it, and its name (the file's).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub name: String,
    /// a shell command, run in the project's folder; empty for one that is only followed (`external`)
    #[serde(default)]
    pub command: String,
    /// the PDF it makes, relative to the project's folder
    pub pdf: Option<PathBuf>,
    #[serde(default)]
    pub run: Run,
}

/// The profile of the file `path`, named after it; an error says what is wrong with it.
pub fn read(path: &Path) -> Result<Profile, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut profile: Profile = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    profile.name = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    if profile.command.trim().is_empty() && profile.run != Run::External {
        return Err(format!("{}: no command to run", path.display()));
    }
    Ok(profile)
}

impl Profile {
    /// Whether it names the document it is run for (`{file}`, `{stem}`, `{dir}`).
    pub fn per_document(&self) -> bool {
        let pdf = self.pdf.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        ["{file}", "{stem}", "{dir}"].iter().any(|t| self.command.contains(t) || pdf.contains(t))
    }

    /// The profile for the document `file` of the project at `root`: its templates filled in.
    pub fn for_document(&self, root: &Path, file: &Path) -> Result<Profile, String> {
        let rel = file.strip_prefix(root).unwrap_or(file);
        let slash = |p: &Path| p.to_string_lossy().replace('\\', "/");
        let file_s = slash(rel);
        let stem = rel.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let dir = rel.parent().map(slash).filter(|d| !d.is_empty()).unwrap_or_else(|| ".".into());
        let fill = |text: &str, quote: bool| {
            let q = |s: &str| if quote { shell_quote(s) } else { s.to_string() };
            text.replace("{file}", &q(&file_s)).replace("{stem}", &q(&stem)).replace("{dir}", &q(&dir))
        };
        let pdf = self.pdf.as_ref().map(|p| PathBuf::from(fill(&p.to_string_lossy(), false)));
        let mut command = fill(&self.command, true);
        if command.contains("{pdf}") {
            let Some(pdf) = &pdf else {
                return Err(format!("{}: the command says {{pdf}}, but no pdf is set", self.name));
            };
            command = command.replace("{pdf}", &shell_quote(&slash(pdf)));
        }
        Ok(Profile { name: self.name.clone(), command, pdf, run: self.run })
    }
}

/// `s` as one word of a shell command: as it is when it is plain, quoted otherwise (`'…'`; `"…"` on Windows).
fn shell_quote(s: &str) -> String {
    let plain = !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "_-./:+,=@%".contains(c));
    if plain {
        s.to_string()
    } else if cfg!(windows) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// The profiles of the project at `root`, by name; the files that are not profiles, with what is wrong with them.
pub fn list(root: &Path) -> (Vec<Profile>, Vec<String>) {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir_of(root))
        .map(|entries| entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "toml")).collect())
        .unwrap_or_default();
    paths.sort();
    let (mut profiles, mut wrong) = (Vec::new(), Vec::new());
    for path in paths {
        match read(&path) {
            Ok(p) => profiles.push(p),
            Err(e) => wrong.push(e),
        }
    }
    (profiles, wrong)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("profiles-{}-{}", std::process::id(), files.len()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(dir_of(&root)).unwrap();
        for (name, text) in files {
            fs::write(dir_of(&root).join(name), text).unwrap();
        }
        root
    }

    #[test]
    fn the_profiles_of_a_project_by_name_and_what_is_wrong_with_the_others() {
        let root = project(&[
            ("synopsis.toml", "command = \"make synopsis\"\npdf = \"build/synopsis.pdf\"\n"),
            ("full.toml", "command = \"make full\"\npdf = \"build-full/thesis.pdf\"\nrun = \"watch\"\n"),
            ("terminal.toml", "pdf = \"build/synopsis.pdf\"\nrun = \"external\"\n"),
            ("quick.toml", "command = \"make synopsis-fast\"\nrun = \"save\"\n"),
            ("broken.toml", "pdf = \"x.pdf\"\n"),
            ("notes.txt", "not a profile"),
        ]);
        let (profiles, wrong) = list(&root);
        let names: Vec<(&str, Run)> = profiles.iter().map(|p| (p.name.as_str(), p.run)).collect();
        assert_eq!(names, [("full", Run::Watch), ("quick", Run::Save), ("synopsis", Run::Build), ("terminal", Run::External)]);
        assert_eq!(profiles[2].pdf.as_deref(), Some(Path::new("build/synopsis.pdf")));
        assert_eq!(wrong.len(), 1);
        assert!(wrong[0].contains("broken.toml") && wrong[0].contains("no command"), "{wrong:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_profile_for_a_document_has_its_templates_filled_in_and_quoted() {
        let p = Profile {
            name: "chapter".into(),
            command: "texmark {file} --pdf -o {pdf} # {stem} in {dir}".into(),
            pdf: Some(PathBuf::from("build/{stem}.pdf")),
            run: Run::Build,
        };
        assert!(p.per_document());
        let root = Path::new("/home/me/thesis");
        let made = p.for_document(root, &root.join("chapters/methods.md")).unwrap();
        assert_eq!(made.command, "texmark chapters/methods.md --pdf -o build/methods.pdf # methods in chapters");
        assert_eq!(made.pdf.as_deref(), Some(Path::new("build/methods.pdf")));
        let spaced = p.for_document(root, &root.join("my notes.md")).unwrap();
        assert_eq!(spaced.command, "texmark 'my notes.md' --pdf -o 'build/my notes.pdf' # 'my notes' in .");
        let no_pdf = Profile { pdf: None, ..p.clone() };
        assert!(no_pdf.for_document(root, &root.join("a.md")).unwrap_err().contains("no pdf"));
        let plain = Profile { command: "make full".into(), pdf: Some("build/thesis.pdf".into()), ..p };
        assert!(!plain.per_document());
        assert_eq!(plain.for_document(root, &root.join("a.md")).unwrap().command, "make full");
    }

    #[test]
    fn a_project_without_profiles_has_none() {
        let root = std::env::temp_dir().join(format!("profiles-none-{}", std::process::id()));
        assert_eq!(list(&root), (vec![], vec![]));
    }
}
