//! A project: a folder with `write.toml`, which says what the files do not (the name, short name and colour, the
//! main document, the build folder, the library searched). A document belongs to the project of the nearest folder
//! above it that has one. The file is read as written; a change writes only its own keys, so the comments and the
//! keys the app does not know stay.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};
use toml_edit::{Array, DocumentMut, Item, value};

use crate::colour::Colour;

/// The file of a project, in its folder.
pub const MANIFEST: &str = "write.toml";

/// The build folder, when `write.toml` names none.
pub const BUILD: &str = "build";

/// The main document a new project can start with: the paper-draft skeleton.
pub const PAPER: &str = "paper.md";

#[derive(Debug)]
pub enum Error {
    NotAFolder(PathBuf),
    Read(PathBuf, io::Error),
    Manifest(PathBuf, String),
    Write(PathBuf, io::Error),
    /// a folder that has a `write.toml` already, made again
    Defined(PathBuf),
    /// a main document that is not a Markdown or LaTeX file of the folder
    Main(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotAFolder(p) => write!(f, "{}: not a folder", p.display()),
            Error::Read(p, e) | Error::Write(p, e) => write!(f, "{}: {e}", p.display()),
            Error::Manifest(p, m) => write!(f, "{}: {m}", p.display()),
            Error::Defined(p) => write!(f, "{}: already a project", p.display()),
            Error::Main(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// What `write.toml` says; all of it optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// the project's name; else the folder's
    pub name: Option<String>,
    /// a few letters for the tile of the project bar, where the name is too long
    pub short: Option<String>,
    pub colour: Option<Colour>,
    /// the main document(s), relative to the folder: one is written as a string, more as an array
    #[serde(default, deserialize_with = "one_or_many")]
    pub main: Vec<PathBuf>,
    /// the build folder, relative to the folder (`build` without)
    pub build: Option<PathBuf>,
    /// a `.bib` file or a papers library searched for references, beside the papers library of the user
    pub library: Option<PathBuf>,
}

/// `main = "paper.md"` or `main = ["paper.md", "supplement.md"]`.
fn one_or_many<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Vec<PathBuf>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Main {
        One(PathBuf),
        Many(Vec<PathBuf>),
    }
    Ok(match Main::deserialize(d)? {
        Main::One(path) => vec![path],
        Main::Many(paths) => paths,
    })
}

#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub manifest: Manifest,
    /// `write.toml` as written, changed key by key
    document: DocumentMut,
}

/// The build folder of a document: that of its project (`build` of `write.toml`, relative to the project's folder,
/// else `build`), else `build` beside the document. A project whose `write.toml` cannot be read counts as none.
pub fn build_dir_of(document: &Path) -> PathBuf {
    match Project::of(document) {
        Ok(Some(project)) => project.build_dir(),
        _ => document.parent().unwrap_or(Path::new(".")).join(BUILD),
    }
}

/// The folder of the project `path` belongs to: the nearest folder that has a `write.toml`, from the folder of `path`
/// (or `path` itself, a folder) upwards.
pub fn find(path: &Path) -> Option<PathBuf> {
    let start = if path.is_dir() { path } else { path.parent()? };
    start.ancestors().find(|dir| dir.join(MANIFEST).is_file()).map(Path::to_path_buf)
}

impl Project {
    /// The project of a folder that has a `write.toml`.
    pub fn read(root: &Path) -> Result<Project> {
        if !root.is_dir() {
            return Err(Error::NotAFolder(root.to_path_buf()));
        }
        let path = root.join(MANIFEST);
        let text = fs::read_to_string(&path).map_err(|e| Error::Read(path.clone(), e))?;
        let document: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| Error::Manifest(path.clone(), e.to_string()))?;
        let manifest = toml_edit::de::from_str(&text).map_err(|e| Error::Manifest(path, e.to_string()))?;
        Ok(Project { root: root.to_path_buf(), manifest, document })
    }

    /// The project a document (or a folder) belongs to, if any.
    pub fn of(path: &Path) -> Result<Option<Project>> {
        find(path).map(|root| Project::read(&root)).transpose()
    }

    /// A folder made a project: `write.toml` written with what `manifest` says. A folder that has one already is
    /// left as it is.
    pub fn create(root: &Path, manifest: &Manifest) -> Result<Project> {
        if !root.is_dir() {
            return Err(Error::NotAFolder(root.to_path_buf()));
        }
        let path = root.join(MANIFEST);
        if path.symlink_metadata().is_ok() {
            return Err(Error::Defined(root.to_path_buf()));
        }
        let mut project = Project { root: root.to_path_buf(), manifest: Manifest::default(), document: DocumentMut::new() };
        project.document.decor_mut().set_prefix("# The project of papers-write: what the files do not say.\n");
        project.apply(manifest);
        write_new(&path, &project.document.to_string())?;
        Ok(project)
    }

    /// What `manifest` says, written: the keys that changed, the rest of `write.toml` as it was.
    pub fn set(&mut self, manifest: &Manifest) -> Result<()> {
        if manifest.clean() == self.manifest {
            return Ok(());
        }
        let before = (self.manifest.clone(), self.document.clone());
        self.apply(manifest);
        if let Err(e) = write(&self.root.join(MANIFEST), &self.document.to_string()) {
            (self.manifest, self.document) = before;
            return Err(e);
        }
        Ok(())
    }

    /// The keys of `manifest` that differ set in the document (an empty text or none takes the key out).
    fn apply(&mut self, manifest: &Manifest) {
        let manifest = manifest.clean();
        let old = std::mem::replace(&mut self.manifest, manifest.clone());
        let doc = &mut self.document;
        let text = |doc: &mut DocumentMut, key: &str, new: Option<&str>| match new {
            Some(s) => doc[key] = value(s),
            None => {
                doc.remove(key);
            }
        };
        let path = |p: &Option<PathBuf>| p.as_ref().map(|p| p.to_string_lossy().into_owned());
        if manifest.name != old.name {
            text(doc, "name", manifest.name.as_deref());
        }
        if manifest.short != old.short {
            text(doc, "short", manifest.short.as_deref());
        }
        if manifest.colour != old.colour {
            text(doc, "colour", manifest.colour.map(Colour::name));
        }
        if manifest.main != old.main {
            let names: Vec<String> = manifest.main.iter().map(|p| p.to_string_lossy().into_owned()).collect();
            match names.as_slice() {
                [] => {
                    doc.remove("main");
                }
                [one] => doc["main"] = value(one.as_str()),
                many => doc["main"] = Item::Value(many.iter().map(String::as_str).collect::<Array>().into()),
            }
        }
        if manifest.build != old.build {
            text(doc, "build", path(&manifest.build).as_deref());
        }
        if manifest.library != old.library {
            text(doc, "library", path(&manifest.library).as_deref());
        }
    }

    /// The name, else the folder's.
    pub fn name(&self) -> String {
        match self.manifest.name.as_deref().map(str::trim) {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => self
                .root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.root.to_string_lossy().into_owned()),
        }
    }

    /// The main documents, by their paths.
    pub fn mains(&self) -> Vec<PathBuf> {
        self.manifest.main.iter().map(|m| self.root.join(m)).collect()
    }

    /// The main document (the first), by its path.
    pub fn main(&self) -> Option<PathBuf> {
        self.mains().into_iter().next()
    }

    /// The build folder.
    pub fn build_dir(&self) -> PathBuf {
        self.root.join(self.manifest.build.as_deref().unwrap_or(Path::new(BUILD)))
    }

    /// The library searched, by its path (`~/` is the home folder).
    pub fn library(&self) -> Option<PathBuf> {
        let library = self.manifest.library.as_ref()?;
        Some(match (library.strip_prefix("~"), std::env::var_os("HOME")) {
            (Ok(rest), Some(home)) => PathBuf::from(home).join(rest),
            _ => self.root.join(library),
        })
    }
}

impl Manifest {
    /// Texts of nothing but spaces unset, the rest trimmed; the main documents without empty ones.
    fn clean(&self) -> Manifest {
        let text = |s: &Option<String>| s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        let path = |p: &Option<PathBuf>| p.clone().filter(|p| !p.as_os_str().is_empty());
        Manifest {
            name: text(&self.name),
            short: text(&self.short),
            colour: self.colour,
            main: self.main.iter().filter(|p| !p.as_os_str().is_empty()).cloned().collect(),
            build: path(&self.build),
            library: path(&self.library),
        }
    }
}

/// Whether a file can be a project's main document: Markdown or LaTeX.
pub fn is_document(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ["md", "markdown", "tex"].iter().any(|d| e.eq_ignore_ascii_case(d)))
}

/// The Markdown and LaTeX files of a folder and of its folders, relative to it, sorted; not those of hidden
/// folders or of the build folder.
pub fn documents(root: &Path) -> Vec<PathBuf> {
    fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if depth < 3 && name != BUILD && name != "node_modules" {
                    walk(root, &path, depth + 1, out);
                }
            } else if is_document(&path)
                && let Ok(rel) = path.strip_prefix(root)
            {
                out.push(rel.to_path_buf());
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, 0, &mut out);
    out.sort();
    out
}

/// The text of a new paper, as the paper-draft skill starts one: the front matter texmark reads (title, authors,
/// date, bibliography, the journal still to choose) and the headings of the sections.
pub fn skeleton(title: &str, author: Option<&str>, date: &str) -> String {
    let quoted = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let author = author.map(str::trim).filter(|a| !a.is_empty());
    let (first, last) = match author.and_then(|a| a.rsplit_once(' ')) {
        Some((first, last)) => (first.trim(), last),
        None => ("", author.unwrap_or("")),
    };
    let title = if title.trim().is_empty() { "TITLE" } else { title.trim() };
    let mut text = String::from("---\n");
    text += &format!("title: {}\n", quoted(title));
    text += "authors:\n";
    text += &format!("  - firstname: {}\n", quoted(if author.is_some() { first } else { "FIRSTNAME" }));
    text += &format!("    lastname: {}\n", quoted(if author.is_some() { last } else { "LASTNAME" }));
    text += "    affiliation: 1\n";
    text += "affiliations:\n  - \"AFFILIATION\"\n";
    text += &format!("date: {}\n", quoted(date));
    text += "bibliography: references.bib\n";
    text += "# journal: TBD - set journal.template once the target journal is decided\n";
    text += "---\n";
    for heading in [
        "Abstract",
        "Introduction",
        "Methods",
        "Results",
        "Discussion",
        "Conclusions",
        "Data availability",
        "Author contributions",
        "Competing interests",
        "Acknowledgements",
    ] {
        text += &format!("\n# {heading}\n");
        if heading == "Competing interests" {
            text += "\nThe authors declare that they have no competing interests.\n";
        }
    }
    text
}

/// A new paper (`skeleton`) written in the folder as `name`; a file there already is left as it is.
pub fn new_paper(root: &Path, name: &str, title: &str, author: Option<&str>, date: &str) -> Result<PathBuf> {
    let path = root.join(name);
    if !is_document(&path) || Path::new(name).components().count() != 1 {
        return Err(Error::Main(format!("{name}: not a name for a Markdown or LaTeX file")));
    }
    write_new(&path, &skeleton(title, author, date))?;
    Ok(path)
}

/// The words of a text, as the editor counts them: what lies between spaces and has a letter or digit.
pub fn words(text: &str) -> usize {
    text.split_whitespace().filter(|w| w.chars().any(|c| c.is_alphanumeric() || c == '_')).count()
}

/// A count in a few letters: 950, 6.2k, 12k.
pub fn short_count(n: usize) -> String {
    match n {
        0..1000 => n.to_string(),
        1000..9950 => format!("{:.1}k", n as f64 / 1000.0),
        _ => format!("{}k", (n as f64 / 1000.0).round() as usize),
    }
}

/// `text` written to a file that is not there yet.
fn write_new(path: &Path, text: &str) -> Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| Error::Write(path.to_path_buf(), e))?;
    file.write_all(text.as_bytes()).map_err(|e| Error::Write(path.to_path_buf(), e))
}

/// `text` written to the file atomically: a file beside it, renamed.
fn write(path: &Path, text: &str) -> Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = path.with_file_name(format!(".{name}.tmp"));
    let written = fs::write(&temporary, text).and_then(|()| fs::rename(&temporary, path));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written.map_err(|e| Error::Write(path.to_path_buf(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fresh, empty folder.
    fn folder() -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("pw-project-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_with_one_main_or_many_and_the_defaults() {
        let root = folder();
        fs::write(root.join(MANIFEST), "name = \"Ice sheet paper\"\nshort = \"ISP\"\ncolour = \"teal\"\nmain = \"paper.md\"\n").unwrap();
        let project = Project::read(&root).unwrap();
        assert_eq!(project.name(), "Ice sheet paper");
        assert_eq!(project.manifest.short.as_deref(), Some("ISP"));
        assert_eq!(project.manifest.colour, Some(Colour::Teal));
        assert_eq!(project.main(), Some(root.join("paper.md")));
        assert_eq!(project.build_dir(), root.join("build"));
        assert_eq!(project.library(), None);

        fs::write(root.join(MANIFEST), "main = [\"a.md\", \"b.tex\"]\nbuild = \"out\"\nlibrary = \"refs/all.bib\"\n").unwrap();
        let project = Project::read(&root).unwrap();
        assert_eq!(project.mains(), [root.join("a.md"), root.join("b.tex")]);
        assert_eq!(project.build_dir(), root.join("out"));
        assert_eq!(project.library(), Some(root.join("refs/all.bib")));
        // no name: the folder's
        assert_eq!(project.name(), root.file_name().unwrap().to_string_lossy());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_document_builds_into_its_projects_folder_else_beside_itself() {
        let root = folder();
        fs::create_dir_all(root.join("sections")).unwrap();
        let doc = root.join("sections/intro.md");
        // no project: beside the document
        assert_eq!(build_dir_of(&doc), root.join("sections/build"));
        // a project without `build`: its `build`, at its root
        fs::write(root.join(MANIFEST), "").unwrap();
        assert_eq!(build_dir_of(&doc), root.join("build"));
        // with one: that, relative to the root, not to the document
        fs::write(root.join(MANIFEST), "build = \"out/pdf\"\n").unwrap();
        assert_eq!(build_dir_of(&doc), root.join("out/pdf"));
        assert_eq!(build_dir_of(&root.join("paper.tex")), root.join("out/pdf"));
        // the nearest project
        fs::write(root.join("sections/write.toml"), "").unwrap();
        assert_eq!(build_dir_of(&doc), root.join("sections/build"));
        // a broken write.toml: beside the document
        fs::write(root.join("sections/write.toml"), "colour = \"mauve\"").unwrap();
        assert_eq!(build_dir_of(&doc), root.join("sections/build"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_broken_file_says_where() {
        let root = folder();
        fs::write(root.join(MANIFEST), "colour = \"mauve\"\n").unwrap();
        let e = Project::read(&root).unwrap_err().to_string();
        assert!(e.contains("write.toml"), "{e}");
        assert!(Project::read(&root.join("nowhere")).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn found_from_a_document_upwards() {
        let root = folder();
        fs::create_dir_all(root.join("sections/deep")).unwrap();
        fs::write(root.join("sections/deep/intro.md"), "").unwrap();
        assert_eq!(find(&root.join("sections/deep/intro.md")), None);
        fs::write(root.join(MANIFEST), "").unwrap();
        assert_eq!(find(&root.join("sections/deep/intro.md")), Some(root.clone()));
        assert_eq!(find(&root.join("sections")), Some(root.clone()));
        // the nearest wins
        fs::write(root.join("sections/write.toml"), "name = \"inner\"\n").unwrap();
        let inner = Project::of(&root.join("sections/deep/intro.md")).unwrap().unwrap();
        assert_eq!(inner.root, root.join("sections"));
        assert_eq!(inner.name(), "inner");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn created_once_and_written_key_by_key_keeping_the_rest() {
        let root = folder();
        let manifest = Manifest {
            name: Some("Ice sheet paper".into()),
            short: Some("  ".into()),
            colour: Some(Colour::Blue),
            main: vec!["paper.md".into()],
            ..Manifest::default()
        };
        let mut project = Project::create(&root, &manifest).unwrap();
        assert!(matches!(Project::create(&root, &manifest), Err(Error::Defined(_))));
        let read = Project::read(&root).unwrap();
        assert_eq!(read.manifest.name.as_deref(), Some("Ice sheet paper"));
        // blank: not set
        assert_eq!(read.manifest.short, None);
        assert_eq!(read.manifest, project.manifest);

        // a comment and a key of someone else's, kept
        let text = fs::read_to_string(root.join(MANIFEST)).unwrap();
        fs::write(root.join(MANIFEST), format!("{text}# the journal's template\njournal = \"tc\"\n")).unwrap();
        let mut project_again = Project::read(&root).unwrap();
        project_again
            .set(&Manifest {
                short: Some("ISP".into()),
                main: vec!["paper.md".into(), "si.md".into()],
                build: Some("out".into()),
                ..project_again.manifest.clone()
            })
            .unwrap();
        let text = fs::read_to_string(root.join(MANIFEST)).unwrap();
        assert!(text.contains("# the journal's template\njournal = \"tc\""), "{text}");
        assert!(text.contains("short = \"ISP\""), "{text}");
        assert!(text.contains("main = [\"paper.md\", \"si.md\"]"), "{text}");
        // a key unset is taken out
        project_again.set(&Manifest { build: None, colour: None, ..project_again.manifest.clone() }).unwrap();
        let text = fs::read_to_string(root.join(MANIFEST)).unwrap();
        assert!(!text.contains("build") && !text.contains("colour"), "{text}");
        assert!(text.starts_with("# The project of papers-write"), "{text}");
        project.set(&project.manifest.clone()).unwrap();
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_documents_of_a_folder_not_hidden_nor_built() {
        let root = folder();
        for file in ["paper.md", "notes.txt", "si/supplement.tex", ".git/x.md", "build/paper.tex", "README.MD"] {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }
        assert_eq!(documents(&root), [PathBuf::from("README.MD"), "paper.md".into(), "si/supplement.tex".into()]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_new_paper_from_the_skeleton_never_over_a_file() {
        let root = folder();
        let path = new_paper(&root, PAPER, "Ice \"sheets\"", Some("Alexander Robinson"), "2026-11-05").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("---\ntitle: \"Ice \\\"sheets\\\"\"\nauthors:\n  - firstname: \"Alexander\"\n    lastname: \"Robinson\"\n"), "{text}");
        assert!(text.contains("date: \"2026-11-05\"\nbibliography: references.bib\n"), "{text}");
        assert!(text.contains("\n# Introduction\n") && text.contains("\n# Acknowledgements\n"), "{text}");
        assert_eq!(crate::frontmatter::bibliography(&text), ["references.bib"]);
        assert!(new_paper(&root, PAPER, "Other", None, "2026-11-05").is_err());
        assert!(new_paper(&root, "notes.txt", "x", None, "").is_err());
        assert!(new_paper(&root, "sub/paper.md", "x", None, "").is_err());
        assert!(skeleton("", None, "d").contains("title: \"TITLE\"\nauthors:\n  - firstname: \"FIRSTNAME\""));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn words_and_their_count_in_a_few_letters() {
        assert_eq!(words("# Intro\n\nIce sheets -- melt, $x$ fast."), 6);
        assert_eq!(words("  "), 0);
        assert_eq!(short_count(950), "950");
        assert_eq!(short_count(6240), "6.2k");
        assert_eq!(short_count(9949), "9.9k");
        assert_eq!(short_count(9950), "10k");
        assert_eq!(short_count(12_400), "12k");
    }
}
