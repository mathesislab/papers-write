//! The files of a project, as a tree: folders first, then files, by name; hidden files and the app's own manifest
//! (`write.toml`, papers-review's `review.toml`) left out. Shared with papers-review.

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// What a file is, for how it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Folder,
    Pdf,
    Markdown,
    /// LaTeX: a document, a style, a class
    Latex,
    /// text to read as it is: code, data descriptions, configuration
    Text,
    Bib,
    /// a figure
    Image,
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
    /// a linked file that is not there
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub missing: bool,
}

/// The most files and folders a tree lists: a folder of data can hold many.
const MOST: usize = 5000;

pub fn kind(path: &Path) -> Kind {
    if path.is_dir() {
        return Kind::Folder;
    }
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "pdf" => Kind::Pdf,
        "md" | "markdown" => Kind::Markdown,
        "bib" => Kind::Bib,
        "tex" | "sty" | "cls" => Kind::Latex,
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "eps" | "tif" | "tiff" => Kind::Image,
        "txt" | "text" | "csv" | "tsv" | "json" | "yaml" | "yml" | "toml" | "xml" | "nml" | "cfg"
        | "ini" | "log" | "jl" | "py" | "r" | "m" | "f" | "f90" | "f95" | "c" | "h"
        | "cpp" | "rs" | "sh" | "ncl" => Kind::Text,
        "" if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().to_uppercase().starts_with("README")) =>
        {
            Kind::Text
        }
        _ => Kind::Other,
    }
}

/// The tree of a folder; the names in `skip` are left out besides the hidden ones.
pub fn tree(root: &Path, skip: &[&str]) -> Vec<Node> {
    let mut count = 0;
    children(root, skip, &mut count)
}

fn children(folder: &Path, skip: &[&str], count: &mut usize) -> Vec<Node> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut nodes: Vec<Node> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || skip.contains(&name.as_str()) {
                return None;
            }
            let path = entry.path();
            Some(Node {
                kind: kind(&path),
                name,
                path,
                children: Vec::new(),
                missing: false,
            })
        })
        .collect();
    nodes.sort_by(order);
    nodes.truncate(MOST.saturating_sub(*count));
    *count += nodes.len();
    for node in &mut nodes {
        if node.kind == Kind::Folder && *count < MOST {
            node.children = children(&node.path, skip, count);
        }
    }
    nodes
}

/// Folders first, then by name, as the Finder sorts (case aside, numbers by value).
fn order(a: &Node, b: &Node) -> Ordering {
    let folder = |n: &Node| n.kind != Kind::Folder;
    folder(a).cmp(&folder(b)).then_with(|| natural(&a.name, &b.name))
}

/// Names compared as the Finder does: case aside, a run of digits by its value.
pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let number = |s: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = s.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        s.next();
                    }
                    digits
                };
                let (m, n) = (number(&mut a), number(&mut b));
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                match m.len().cmp(&n.len()).then_with(|| m.cmp(n)) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            (Some(x), Some(y)) => {
                match x.to_lowercase().cmp(y.to_lowercase()) {
                    Ordering::Equal => {
                        a.next();
                        b.next();
                    }
                    other => return other,
                }
            }
        }
    }
}

/// A file linked from elsewhere, there or not.
pub fn linked(path: &Path) -> Node {
    Node {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.to_path_buf(),
        kind: kind(path),
        children: Vec::new(),
        missing: !path.exists(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fresh, empty folder.
    fn folder() -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("pw-files-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(nodes: &[Node]) -> Vec<&str> {
        nodes.iter().map(|n| n.name.as_str()).collect()
    }

    #[test]
    fn folders_first_by_name_hidden_files_and_those_skipped_left_out() {
        let dir = folder();
        let root = dir.as_path();
        for folder in ["round-2", "round-10", "Round-1", ".git"] {
            fs::create_dir(root.join(folder)).unwrap();
        }
        for file in ["review.md", "review.toml", "write.toml", ".DS_Store", "a.pdf", "round-2/revised.pdf"] {
            fs::write(root.join(file), "").unwrap();
        }
        let tree = tree(root, &["review.toml", "write.toml"]);
        assert_eq!(names(&tree), ["Round-1", "round-2", "round-10", "a.pdf", "review.md"]);
        assert_eq!(names(&tree[1].children), ["revised.pdf"]);
        assert_eq!(tree[1].children[0].kind, Kind::Pdf);
        assert_eq!(tree[4].kind, Kind::Markdown);
    }

    #[test]
    fn kinds() {
        assert_eq!(kind(&folder()), Kind::Folder);
        assert_eq!(kind(Path::new("x/Paper.PDF")), Kind::Pdf);
        assert_eq!(kind(Path::new("x/refs.bib")), Kind::Bib);
        assert_eq!(kind(Path::new("x/run.jl")), Kind::Text);
        assert_eq!(kind(Path::new("x/README")), Kind::Text);
        assert_eq!(kind(Path::new("x/paper.tex")), Kind::Latex);
        assert_eq!(kind(Path::new("x/agu.STY")), Kind::Latex);
        assert_eq!(kind(Path::new("x/copernicus.cls")), Kind::Latex);
        assert_eq!(kind(Path::new("x/fig.png")), Kind::Image);
        assert_eq!(kind(Path::new("x/fig.JPEG")), Kind::Image);
        assert_eq!(kind(Path::new("x/map.eps")), Kind::Image);
        assert_eq!(kind(Path::new("x/data.nc")), Kind::Other);
        assert_eq!(serde_json::to_string(&[Kind::Latex, Kind::Image]).unwrap(), r#"["latex","image"]"#);
    }

    #[test]
    fn names_in_natural_order_and_a_linked_file_missing() {
        let mut names = vec!["fig10.png", "Fig2.png", "fig1.png", "fig02b.png"];
        names.sort_by(|a, b| natural(a, b));
        assert_eq!(names, ["fig1.png", "Fig2.png", "fig02b.png", "fig10.png"]);
        let gone = linked(Path::new("/nowhere/paper.md"));
        assert_eq!((gone.name.as_str(), gone.kind, gone.missing), ("paper.md", Kind::Markdown, true));
    }

    #[test]
    fn a_tree_lists_so_many_files_at_most() {
        let dir = folder();
        let data = dir.join("data");
        fs::create_dir(&data).unwrap();
        for i in 0..MOST + 10 {
            fs::write(data.join(format!("{i}.nc")), "").unwrap();
        }
        let tree = tree(&dir, &[]);
        assert_eq!(tree[0].children.len(), MOST - 1);
    }
}
