//! A folder of notes (a project of kind `notes`, or a folder another notes app keeps: Obsidian, Logseq, Foam,
//! Zettlr, Dendron): its notes are the Markdown files of the folder and of its folders, found by their title (the
//! front matter's `title`, else the first heading, else the file's name), their file name and their text.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

/// The marks other notes apps leave in a folder they keep, and the app's name.
const MARKERS: [(&str, &str); 5] = [(".obsidian", "Obsidian"), ("logseq", "Logseq"), (".foam", "Foam"), (".ztr-directory", "Zettlr"), ("dendron.yml", "Dendron")];

/// The notes app that keeps the folder `root`, if one does (its mark is in it).
pub fn kept_by(root: &Path) -> Option<&'static str> {
    MARKERS.iter().find(|(mark, _)| root.join(mark).exists()).map(|(_, app)| *app)
}

/// A note as the search lists it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Note {
    pub path: PathBuf,
    /// its path in the folder
    pub name: String,
    pub title: String,
    /// when it was last changed (seconds since 1970)
    pub modified: u64,
    /// how well it matches the query (none: 0)
    pub score: u32,
    /// the line of its text the query is found in
    pub line: Option<String>,
}

/// The title of a note: its front matter's `title`, else its first heading, else its file's name.
pub fn title_of(text: &str, path: &Path) -> String {
    if let Some(title) = crate::frontmatter::value(text, "title").map(|t| t.trim().trim_matches(['"', '\'']).to_string()).filter(|t| !t.is_empty()) {
        return title;
    }
    let heading = text.lines().find_map(|l| l.strip_prefix('#').map(|h| h.trim_start_matches('#').trim().to_string())).filter(|h| !h.is_empty());
    heading.unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into_owned())
}

/// The Markdown files of `root` and its folders: not hidden ones, not build folders.
fn files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "node_modules" || name.starts_with("build") {
                continue;
            }
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() && depth < 6 => walk(&path, depth + 1, out),
                Ok(t) if t.is_file() && crate::files::is_markdown(&path) => out.push(path),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(root, 0, &mut out);
    out
}

/// The notes of `root` matching `query` (its words, each found in the title, the file name or the text), the best
/// first: a word in the title counts most, then in the file name, then each time in the text (five at most). Without
/// a query, all of them, the most recently changed first.
pub fn search(root: &Path, query: &str) -> Vec<Note> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut notes: Vec<Note> = files(root)
        .into_iter()
        .filter_map(|path| {
            let text = fs::read_to_string(&path).ok()?;
            let modified = fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            let title = title_of(&text, &path);
            let name = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned();
            let (lower_title, lower_name, lower_text) = (title.to_lowercase(), name.to_lowercase(), text.to_lowercase());
            let mut score = 0;
            for w in &words {
                let in_text = lower_text.matches(w.as_str()).count().min(5) as u32;
                let here = if lower_title.contains(w.as_str()) { 10 } else { 0 } + if lower_name.contains(w.as_str()) { 5 } else { 0 } + in_text;
                if here == 0 {
                    return None;
                }
                score += here;
            }
            let line = words.first().and_then(|w| {
                text.lines()
                    .skip_while(|l| l.trim() == "---")
                    .find(|l| l.to_lowercase().contains(w.as_str()) && !l.trim_start().starts_with('#'))
                    .map(|l| l.trim().chars().take(160).collect::<String>())
            });
            Some(Note { path, name, title, modified, score, line })
        })
        .collect();
    notes.sort_by(|a, b| b.score.cmp(&a.score).then(b.modified.cmp(&a.modified)));
    notes
}

/// A new note in `root` titled `title`: `<title>.md` (another number after it when that is taken), with the title as
/// its first heading. Its path.
pub fn create(root: &Path, title: &str) -> std::io::Result<PathBuf> {
    let title = title.trim();
    let stem: String = title.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c }).collect();
    let stem = if stem.trim().is_empty() { "Untitled".to_string() } else { stem.trim().to_string() };
    let mut path = root.join(format!("{stem}.md"));
    let mut n = 2;
    while path.exists() {
        path = root.join(format!("{stem} {n}.md"));
        n += 1;
    }
    let heading = if title.is_empty() { "Untitled" } else { title };
    fs::write(&path, format!("# {heading}\n\n"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str, notes: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("notes-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        for (path, text) in notes {
            fs::write(root.join(path), text).unwrap();
        }
        root
    }

    #[test]
    fn titles_from_the_front_matter_else_the_heading_else_the_name() {
        assert_eq!(title_of("---\ntitle: \"Sea level\"\n---\n# Other\n", Path::new("a.md")), "Sea level");
        assert_eq!(title_of("text\n## Ice sheets\n", Path::new("a.md")), "Ice sheets");
        assert_eq!(title_of("just text", Path::new("dir/my note.md")), "my note");
    }

    #[test]
    fn notes_found_by_title_name_and_text_the_best_first() {
        let root = folder(
            "search",
            &[
                ("ice.md", "# Ice sheets\n\nAntarctic discharge and calving.\n"),
                ("sub/calving-notes.md", "# Glaciers\n\nOn calving, calving and calving.\n"),
                ("sea.md", "---\ntitle: Sea level\n---\nNothing about ice.\n"),
                (".obsidian/x.md", "# hidden calving"),
            ],
        );
        assert_eq!(kept_by(&root), Some("Obsidian"));
        let found: Vec<String> = search(&root, "calving").iter().map(|n| n.name.clone()).collect();
        assert_eq!(found, ["sub/calving-notes.md", "ice.md"]);
        let ice = search(&root, "ice");
        assert_eq!(ice[0].title, "Ice sheets");
        assert_eq!(search(&root, "ice antarctic").len(), 1);
        assert_eq!(search(&root, "").len(), 3);
        assert_eq!(search(&root, "calving")[1].line.as_deref(), Some("Antarctic discharge and calving."));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_new_note_takes_its_title_as_its_name_and_heading_never_over_another() {
        let root = folder("create", &[]);
        let a = create(&root, "Meeting: Tuesday").unwrap();
        assert_eq!(a.file_name().unwrap(), "Meeting- Tuesday.md");
        assert_eq!(fs::read_to_string(&a).unwrap(), "# Meeting: Tuesday\n\n");
        let b = create(&root, "Meeting: Tuesday").unwrap();
        assert_eq!(b.file_name().unwrap(), "Meeting- Tuesday 2.md");
        let _ = fs::remove_dir_all(&root);
    }
}
