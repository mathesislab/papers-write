//! git (the `git` command, no library): the state of the repository of a document (branch, ahead and behind of its
//! upstream after the last fetch, the files changed), fetched quietly now and then. The versions of a document to
//! choose from (the tags and the last commits that touched it) and the text of one are here too: the builds and the
//! exchange use them. Nothing here rewrites history, merges or forces.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::build::{find_tool, search_path};
use crate::documents;

/// What is kept of the repositories: when each was last fetched.
#[derive(Default)]
pub struct Git {
    fetched: Mutex<HashMap<PathBuf, Instant>>,
}

/// How long after a fetch the next may come.
const FETCH_EVERY: Duration = Duration::from_secs(5 * 60);

// ---- running git

/// git run in `folder`: its standard output, else what it said (stderr, else stdout, as git wrote it). It never asks
/// for a password or a passphrase, and never opens an editor: a GUI app has no terminal to answer in.
fn git(folder: &Path, args: &[&str]) -> Result<String, String> {
    let program = find_tool("git")?;
    let out = Command::new(&program)
        .args(args)
        .current_dir(folder)
        .env("PATH", search_path(program.parent()))
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("GIT_SSH_COMMAND", std::env::var("GIT_SSH_COMMAND").unwrap_or_else(|_| "ssh -o BatchMode=yes".into()))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let (err, said) = (String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout));
    let text = if err.trim().is_empty() { said } else { err };
    Err(text.trim().to_string())
}

/// What git printed, or none when it failed.
pub(crate) fn git_lines(folder: &Path, args: &[&str]) -> Option<String> {
    git(folder, args).ok()
}

/// The text of `file` as it was at `rev`.
pub(crate) fn git_show(folder: &Path, name: &str, rev: &str) -> Result<Vec<u8>, String> {
    let program = find_tool("git")?;
    let out = Command::new(&program)
        .args(["show", &format!("{rev}:./{name}")])
        .current_dir(folder)
        .env("PATH", search_path(program.parent()))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// The folder of a document.
fn folder_of(doc: &Path) -> &Path {
    doc.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."))
}

/// The top of the repository the folder is in; none when it is not in one (or there is no git).
fn toplevel(folder: &Path) -> Option<PathBuf> {
    let out = git_lines(folder, &["rev-parse", "--show-toplevel"])?;
    let top = PathBuf::from(out.trim());
    Some(top.canonicalize().unwrap_or(top))
}

// ---- the versions of a document

/// A version of a document in git, to choose from.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Revision {
    /// what git is given: a tag, or the hash of a commit
    pub rev: String,
    /// the short name: the tag, the short hash
    pub label: String,
    /// "tag" or "commit"
    pub kind: &'static str,
    /// date, author and subject
    pub detail: String,
}

/// The versions of a file from `git log --format=%H%x1f%h%x1f%as%x1f%an%x1f%s`.
pub(crate) fn parse_commits(output: &str) -> Vec<Revision> {
    output
        .lines()
        .filter_map(|line| {
            let mut f = line.split('\u{1f}');
            let (hash, short, date, author, subject) = (f.next()?, f.next()?, f.next()?, f.next()?, f.next()?);
            Some(Revision {
                rev: hash.to_string(),
                label: short.to_string(),
                kind: "commit",
                detail: format!("{date} {author}: {subject}"),
            })
        })
        .collect()
}

/// The tags from `git tag --format=%(refname:short)%1f%(creatordate:short)%1f%(subject)`.
pub(crate) fn parse_tags(output: &str) -> Vec<Revision> {
    output
        .lines()
        .filter_map(|line| {
            let mut f = line.split('\u{1f}');
            let name = f.next().filter(|n| !n.is_empty())?;
            let (date, subject) = (f.next().unwrap_or(""), f.next().unwrap_or(""));
            Some(Revision {
                rev: name.to_string(),
                label: name.to_string(),
                kind: "tag",
                detail: format!("{date} {subject}").trim().to_string(),
            })
        })
        .collect()
}

/// The versions of the file `doc` in git: the tags, and the last 30 commits that touched it; none when it is not in
/// a repository.
pub(crate) fn revisions_of(doc: &Path) -> Vec<Revision> {
    let folder = folder_of(doc);
    let name = doc.file_name().unwrap_or_default().to_string_lossy().into_owned();
    if git_lines(folder, &["rev-parse", "--is-inside-work-tree"]).is_none() {
        return Vec::new();
    }
    let mut found = git_lines(folder, &["tag", "--sort=-creatordate", "--format=%(refname:short)%1f%(creatordate:short)%1f%(subject)"])
        .map(|o| parse_tags(&o))
        .unwrap_or_default();
    found.truncate(50);
    found.extend(
        git_lines(folder, &["log", "-n", "30", "--format=%H%x1f%h%x1f%as%x1f%an%x1f%s", "--", &name])
            .map(|o| parse_commits(&o))
            .unwrap_or_default(),
    );
    found
}

// ---- the state of the repository

/// A file that differs from the last commit, as `git status` says.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Changed {
    /// relative to the top of the repository
    pub path: String,
    /// what git says of it, the two letters without the dots: "M", "A", "D", "R", "MM", "UU"; "?" for a new file
    pub status: String,
    /// a renamed file: where it was
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

/// The repository of a document.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct State {
    /// the top of the repository
    pub repo: String,
    /// the branch; "(detached)" off any
    pub branch: String,
    /// the branch it follows, as "origin/main"
    pub upstream: Option<String>,
    /// the commits it is ahead of, and behind, the upstream as of the last fetch
    pub ahead: u32,
    pub behind: u32,
    pub changed: Vec<Changed>,
}

/// `git status --porcelain=v2 --branch -z` read.
pub(crate) fn parse_status(output: &str) -> State {
    let mut state = State::default();
    let mut tokens = output.split('\0').filter(|t| !t.is_empty());
    while let Some(token) = tokens.next() {
        if let Some(header) = token.strip_prefix("# ") {
            let (key, value) = header.split_once(' ').unwrap_or((header, ""));
            match key {
                "branch.head" => state.branch = value.to_string(),
                "branch.upstream" => state.upstream = Some(value.to_string()),
                "branch.ab" => {
                    let mut n = value.split(' ').map(|v| v.trim_start_matches(['+', '-']).parse::<u32>().unwrap_or(0));
                    (state.ahead, state.behind) = (n.next().unwrap_or(0), n.next().unwrap_or(0));
                }
                _ => {}
            }
            continue;
        }
        let kind = token.as_bytes()[0];
        let changed = match kind {
            // ordinary: 1 XY sub mH mI mW hH hI path
            b'1' => {
                let f: Vec<&str> = token.splitn(9, ' ').collect();
                (f.len() == 9).then(|| (f[8].to_string(), f[1], None))
            }
            // renamed or copied: 2 XY sub mH mI mW hH hI Xscore path, then the path it came from
            b'2' => {
                let f: Vec<&str> = token.splitn(10, ' ').collect();
                let from = tokens.next().map(str::to_string);
                (f.len() == 10).then(|| (f[9].to_string(), f[1], from))
            }
            // unmerged: u XY sub m1 m2 m3 mW h1 h2 h3 path
            b'u' => {
                let f: Vec<&str> = token.splitn(11, ' ').collect();
                (f.len() == 11).then(|| (f[10].to_string(), f[1], None))
            }
            b'?' => Some((token[2..].to_string(), "?", None)),
            _ => None,
        };
        if let Some((path, xy, from)) = changed {
            state.changed.push(Changed { path, status: xy.replace('.', ""), from });
        }
    }
    state
}

/// The state of the repository `doc` is in; none when it is not in one.
pub(crate) fn state_of(doc: &Path) -> Option<State> {
    let folder = folder_of(doc);
    let top = toplevel(folder)?;
    // not taking the index's lock: a poll must not make a commit of the user's wait or fail
    let out = git(&top, &["--no-optional-locks", "status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all"]).ok()?;
    let mut state = parse_status(&out);
    state.repo = top.to_string_lossy().into_owned();
    Some(state)
}

/// The document of the window, else an error that says to save it.
pub(crate) fn doc_of(window: &WebviewWindow) -> Result<PathBuf, String> {
    documents::path_of(window.app_handle(), window.label()).ok_or_else(|| "Save the document first (⌘S)".to_string())
}

/// Blocking work off the thread of the page.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

/// The repository of the window's document: none when it is untitled or not in a repository.
#[tauri::command]
pub async fn git_state(window: WebviewWindow) -> Result<Option<State>, String> {
    let Ok(doc) = doc_of(&window) else { return Ok(None) };
    blocking(move || state_of(&doc)).await
}

/// Fetches the upstream of the repository, quietly, for the ahead and behind: at most every 5 minutes for a
/// repository, and a failure (offline, no remote, a password asked for) is not told. Whether it fetched.
#[tauri::command]
pub async fn git_fetch(app: AppHandle, window: WebviewWindow) -> Result<bool, String> {
    let Ok(doc) = doc_of(&window) else { return Ok(false) };
    blocking(move || {
        let Some(top) = toplevel(folder_of(&doc)) else { return false };
        let state = app.state::<Git>();
        {
            let mut fetched = state.fetched.lock().unwrap();
            if fetched.get(&top).is_some_and(|t| t.elapsed() < FETCH_EVERY) {
                return false;
            }
            // a failure waits as long as a success: no hammering of a server that is not there
            fetched.insert(top.clone(), Instant::now());
        }
        if git_lines(&top, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]).is_none() {
            return false;
        }
        git(&top, &["fetch", "--quiet"]).is_ok()
    })
    .await
}

// ---- Commit…

/// How the app's own files are told apart in the list of the files changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Candidate {
    #[serde(flatten)]
    pub file: Changed,
    /// "document", "bibliography" (a .bib the document names), "review" (its review folder), or "other"
    pub kind: &'static str,
    /// whether it is ticked as the sheet opens: all but "other"
    pub ticked: bool,
}

/// The files changed, with the document, its bibliography files and its review folder `review` (a path ending in
/// `/`) ticked by default and the rest not. All paths are relative to the top of the repository.
pub(crate) fn candidates(changed: &[Changed], doc: &str, bibs: &[String], review: &str) -> Vec<Candidate> {
    changed
        .iter()
        .map(|c| {
            let kind = if c.path == doc {
                "document"
            } else if bibs.contains(&c.path) {
                "bibliography"
            } else if c.path.starts_with(review) {
                "review"
            } else {
                "other"
            };
            Candidate { file: c.clone(), kind, ticked: kind != "other" }
        })
        .collect()
}

/// `path` relative to the top of the repository, with `/`.
fn relative(top: &Path, path: &Path) -> Option<String> {
    let path = path.canonicalize().ok()?;
    Some(path.strip_prefix(top).ok()?.to_string_lossy().replace('\\', "/"))
}

/// The files of the repository of `doc` that differ from the last commit, as the sheet lists them. `bibs` are the
/// bibliography files of the document.
pub(crate) fn changes_of(doc: &Path, bibs: &[PathBuf]) -> Result<Vec<Candidate>, String> {
    let state = state_of(doc).ok_or("The document is not in a git repository")?;
    let top = PathBuf::from(&state.repo);
    let doc_rel = relative(&top, doc).ok_or("The document is not in the repository")?;
    let bibs: Vec<String> = bibs.iter().filter_map(|b| relative(&top, b)).collect();
    let dir = Path::new(&doc_rel).parent().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    let name = doc.file_name().unwrap_or_default().to_string_lossy();
    let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
    let review = format!("{prefix}.papers-write/review/{name}/");
    Ok(candidates(&state.changed, &doc_rel, &bibs, &review))
}

/// `paths` (relative to the top of the repository) committed with `message` and nothing else: `git add` of those
/// that are there, then `git commit -m` of those paths (never `-a`, never `--amend`; what else is staged stays so).
pub(crate) fn commit(doc: &Path, paths: &[String], message: &str) -> Result<String, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("A commit needs a message".into());
    }
    if paths.is_empty() {
        return Err("Tick at least one file to commit".into());
    }
    let state = state_of(doc).ok_or("The document is not in a git repository")?;
    let top = PathBuf::from(&state.repo);
    // a file renamed goes with the name it had: its deletion is part of the commit
    let mut all: Vec<&str> = paths.iter().map(String::as_str).collect();
    all.extend(state.changed.iter().filter(|c| paths.contains(&c.path)).filter_map(|c| c.from.as_deref()));
    // a file deleted is not added (git has no path to match): the commit of its path records it
    let present: Vec<&str> = paths.iter().map(String::as_str).filter(|p| top.join(p).exists()).collect();
    if !present.is_empty() {
        let mut args = vec!["add", "--"];
        args.extend(&present);
        git(&top, &args)?;
    }
    let mut args = vec!["commit", "-m", message, "--"];
    args.extend(&all);
    git(&top, &args)?;
    let head = git_lines(&top, &["log", "-n", "1", "--format=%h"]).unwrap_or_default();
    Ok(format!("Committed {}", head.trim()))
}

/// What a branch follows, if anything.
fn upstream_of(top: &Path) -> Option<String> {
    git_lines(top, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]).map(|o| o.trim().to_string())
}

fn top_of(doc: &Path) -> Result<(PathBuf, State), String> {
    let state = state_of(doc).ok_or("The document is not in a git repository")?;
    Ok((PathBuf::from(&state.repo), state))
}

/// `git pull --ff-only`: the upstream's new commits, if the branch can go forward to them; else nothing is done
/// and it is said (never a merge commit, never a rebase: that is for a terminal).
pub(crate) fn pull(doc: &Path) -> Result<String, String> {
    let (top, state) = top_of(doc)?;
    if upstream_of(&top).is_none() {
        return Err(format!("{} follows no upstream branch: nothing to pull (in a terminal: git branch -u origin/{})", state.branch, state.branch));
    }
    match git(&top, &["pull", "--ff-only", "--no-rebase"]) {
        Ok(out) => Ok(out.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Pulled").to_string()),
        Err(e) if e.contains("fast-forward") || e.contains("diverging") || e.contains("divergent") => Err(format!(
            "Cannot fast-forward: {} and its upstream have each moved on. Nothing was changed; resolve it in a terminal (git pull --rebase, or git merge).\n{e}",
            state.branch
        )),
        Err(e) => Err(e),
    }
}

/// `git push` of the branch to its upstream (which it must have); never forced.
pub(crate) fn push(doc: &Path) -> Result<String, String> {
    let (top, state) = top_of(doc)?;
    let Some(upstream) = upstream_of(&top) else {
        return Err(format!("{} follows no upstream branch: set one in a terminal (git push -u origin {})", state.branch, state.branch));
    };
    git(&top, &["push"])?;
    Ok(format!("Pushed {} to {upstream}", state.branch))
}

/// The files changed, for the Commit sheet (`text` is the document as typed, which names its bibliography).
#[tauri::command]
pub async fn git_changes(window: WebviewWindow, text: String) -> Result<Vec<Candidate>, String> {
    let doc = doc_of(&window)?;
    let bibs = crate::refs::own_bibs(&window, &text);
    blocking(move || changes_of(&doc, &bibs)).await?
}

/// The paths committed with the message; what git says.
#[tauri::command]
pub async fn git_commit(window: WebviewWindow, paths: Vec<String>, message: String) -> Result<String, String> {
    let doc = doc_of(&window)?;
    blocking(move || commit(&doc, &paths, &message)).await?
}

#[tauri::command]
pub async fn git_pull(window: WebviewWindow) -> Result<String, String> {
    let doc = doc_of(&window)?;
    blocking(move || pull(&doc)).await?
}

#[tauri::command]
pub async fn git_push(window: WebviewWindow) -> Result<String, String> {
    let doc = doc_of(&window)?;
    blocking(move || push(&doc)).await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_versions() {
        let commits = parse_commits("abc123def\u{1f}abc123d\u{1f}2026-10-01\u{1f}Alex\u{1f}Methods: rewritten\nbad line\n");
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].rev, "abc123def");
        assert_eq!(commits[0].label, "abc123d");
        assert_eq!(commits[0].detail, "2026-10-01 Alex: Methods: rewritten");
        let tags = parse_tags("v2\u{1f}2026-09-01\u{1f}Submitted\nv1\u{1f}2026-05-01\u{1f}\n");
        assert_eq!(tags.len(), 2);
        assert_eq!((tags[0].kind, tags[0].label.as_str(), tags[0].detail.as_str()), ("tag", "v2", "2026-09-01 Submitted"));
        assert_eq!(tags[1].detail, "2026-05-01");
    }

    fn status(lines: &[&str]) -> String {
        lines.iter().map(|l| format!("{l}\0")).collect()
    }

    #[test]
    fn status_of_a_branch_with_an_upstream() {
        let s = parse_status(&status(&[
            "# branch.oid 1234567890abcdef",
            "# branch.head main",
            "# branch.upstream origin/main",
            "# branch.ab +3 -1",
        ]));
        assert_eq!((s.branch.as_str(), s.upstream.as_deref(), s.ahead, s.behind), ("main", Some("origin/main"), 3, 1));
        assert!(s.changed.is_empty());
    }

    #[test]
    fn status_without_an_upstream_or_on_a_detached_head() {
        let s = parse_status(&status(&["# branch.oid abc", "# branch.head (detached)"]));
        assert_eq!((s.branch.as_str(), s.upstream.as_deref(), s.ahead, s.behind), ("(detached)", None, 0, 0));
    }

    #[test]
    fn status_of_changed_renamed_and_new_files() {
        let h = "a".repeat(40);
        let s = parse_status(&status(&[
            "# branch.head main",
            &format!("1 .M N... 100644 100644 100644 {h} {h} paper.md"),
            &format!("1 A. N... 000000 100644 100644 {h} {h} notes with space.md"),
            &format!("2 R. N... 100644 100644 100644 {h} {h} R100 new name.md"),
            "old name.md",
            &format!("u UU N... 100644 100644 100644 100644 {h} {h} {h} clash.md"),
            "? .papers-write/review/paper.md/jane.toml",
        ]));
        let got: Vec<(&str, &str, Option<&str>)> = s.changed.iter().map(|c| (c.path.as_str(), c.status.as_str(), c.from.as_deref())).collect();
        assert_eq!(
            got,
            vec![
                ("paper.md", "M", None),
                ("notes with space.md", "A", None),
                ("new name.md", "R", Some("old name.md")),
                ("clash.md", "UU", None),
                (".papers-write/review/paper.md/jane.toml", "?", None),
            ]
        );
    }

    /// A scratch folder of its own.
    pub(super) fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pw-git-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    /// `git` in `dir`, which must succeed.
    pub(super) fn sh(dir: &Path, args: &[&str]) -> String {
        git(dir, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    }

    /// A repository with a committed document and a user.
    pub(super) fn repo(name: &str) -> PathBuf {
        let dir = scratch(name);
        sh(&dir, &["init", "-q", "-b", "main"]);
        sh(&dir, &["config", "user.name", "Test User"]);
        sh(&dir, &["config", "user.email", "test@example.org"]);
        std::fs::write(dir.join("paper.md"), "The line retreats rapidly now.\n").unwrap();
        sh(&dir, &["add", "."]);
        sh(&dir, &["commit", "-q", "-m", "First"]);
        dir
    }

    #[test]
    fn the_state_of_a_real_repository() {
        let dir = repo("state");
        let doc = dir.join("paper.md");
        std::fs::write(&doc, "The line retreats slowly now.\n").unwrap();
        std::fs::write(dir.join("refs.bib"), "").unwrap();
        let s = state_of(&doc).unwrap();
        assert_eq!((s.branch.as_str(), s.upstream.as_deref()), ("main", None));
        assert_eq!(s.repo, dir.to_string_lossy());
        let got: Vec<(&str, &str)> = s.changed.iter().map(|c| (c.path.as_str(), c.status.as_str())).collect();
        assert_eq!(got, vec![("paper.md", "M"), ("refs.bib", "?")]);
        // not a repository
        let plain = scratch("plain");
        assert!(state_of(&plain.join("x.md")).is_none());
    }

    #[test]
    fn the_revisions_of_a_file() {
        let dir = repo("revisions");
        sh(&dir, &["tag", "v1"]);
        let found = revisions_of(&dir.join("paper.md"));
        assert_eq!((found[0].kind, found[0].label.as_str()), ("tag", "v1"));
        assert_eq!(found[1].kind, "commit");
    }

    #[test]
    fn the_document_its_bibliography_and_its_review_are_ticked() {
        let c = |p: &str| Changed { path: p.into(), status: "M".into(), from: None };
        let changed = [
            c("paper/paper.md"),
            c("paper/refs.bib"),
            c("paper/.papers-write/review/paper.md/jane.toml"),
            c("paper/.papers-write/review/other.md/jane.toml"),
            c("paper/figure.png"),
            c("notes.md"),
        ];
        let got = candidates(&changed, "paper/paper.md", &["paper/refs.bib".into()], "paper/.papers-write/review/paper.md/");
        let kinds: Vec<(&str, bool)> = got.iter().map(|c| (c.kind, c.ticked)).collect();
        assert_eq!(
            kinds,
            vec![("document", true), ("bibliography", true), ("review", true), ("other", false), ("other", false), ("other", false)]
        );
    }

    #[test]
    fn changes_of_a_real_repository_are_ticked() {
        let dir = repo("changes");
        std::fs::create_dir_all(dir.join("sub/.papers-write/review/p.md")).unwrap();
        std::fs::write(dir.join("sub/p.md"), "x\n").unwrap();
        std::fs::write(dir.join("sub/refs.bib"), "").unwrap();
        std::fs::write(dir.join("sub/.papers-write/review/p.md/jane.toml"), "").unwrap();
        std::fs::write(dir.join("other.txt"), "").unwrap();
        let got = changes_of(&dir.join("sub/p.md"), &[dir.join("sub/refs.bib")]).unwrap();
        let kinds: Vec<(&str, &str, bool)> = got.iter().map(|c| (c.file.path.as_str(), c.kind, c.ticked)).collect();
        assert_eq!(
            kinds,
            vec![
                ("other.txt", "other", false),
                ("sub/.papers-write/review/p.md/jane.toml", "review", true),
                ("sub/p.md", "document", true),
                ("sub/refs.bib", "bibliography", true),
            ]
        );
    }

    #[test]
    fn the_commit_has_exactly_the_paths_given() {
        let dir = repo("commit");
        let doc = dir.join("paper.md");
        std::fs::write(&doc, "The line retreats slowly now.\n").unwrap();
        std::fs::write(dir.join("refs.bib"), "@book{a}\n").unwrap();
        std::fs::write(dir.join("staged.txt"), "s\n").unwrap();
        std::fs::write(dir.join("loose.txt"), "l\n").unwrap();
        sh(&dir, &["add", "staged.txt"]);
        let said = commit(&doc, &["paper.md".into(), "refs.bib".into()], "  Methods rewritten  ").unwrap();
        assert!(said.starts_with("Committed "));
        assert_eq!(sh(&dir, &["log", "-n", "1", "--format=%s"]).trim(), "Methods rewritten");
        let mut files: Vec<String> = sh(&dir, &["show", "--name-only", "--format=", "HEAD"]).lines().map(String::from).collect();
        files.sort();
        assert_eq!(files, vec!["paper.md", "refs.bib"]);
        // what was staged for something else stays staged; the loose file stays loose
        let left = sh(&dir, &["status", "--porcelain"]);
        assert!(left.contains("A  staged.txt") && left.contains("?? loose.txt"), "{left}");
    }

    #[test]
    fn a_deleted_and_a_renamed_file_are_committed() {
        let dir = repo("commit-rm");
        std::fs::write(dir.join("old.md"), "o\n").unwrap();
        sh(&dir, &["add", "."]);
        sh(&dir, &["commit", "-q", "-m", "Second"]);
        sh(&dir, &["mv", "old.md", "new.md"]);
        std::fs::remove_file(dir.join("paper.md")).unwrap();
        commit(&dir.join("new.md"), &["new.md".into(), "paper.md".into()], "Move and delete").unwrap();
        assert_eq!(sh(&dir, &["status", "--porcelain"]).trim(), "");
    }

    #[test]
    fn a_commit_needs_a_message_and_a_file() {
        let dir = repo("commit-empty");
        let doc = dir.join("paper.md");
        assert!(commit(&doc, &["paper.md".into()], "  ").unwrap_err().contains("message"));
        assert!(commit(&doc, &[], "x").unwrap_err().contains("file"));
        // nothing changed: git says so
        assert!(commit(&doc, &["paper.md".into()], "x").is_err());
    }

    /// A bare origin and two clones of it.
    fn clones(name: &str) -> (PathBuf, PathBuf) {
        let base = scratch(name);
        sh(&base, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
        let origin = base.join("origin.git");
        let clone = |n: &str| {
            sh(&base, &["clone", "-q", origin.to_str().unwrap(), n]);
            let dir = base.join(n);
            sh(&dir, &["config", "user.name", n]);
            sh(&dir, &["config", "user.email", "t@example.org"]);
            dir
        };
        let a = clone("a");
        std::fs::write(a.join("paper.md"), "one\n").unwrap();
        sh(&a, &["add", "."]);
        sh(&a, &["commit", "-q", "-m", "One"]);
        sh(&a, &["push", "-q", "-u", "origin", "main"]);
        let b = clone("b");
        (a, b)
    }

    fn commit_file(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(name), text).unwrap();
        sh(dir, &["add", "."]);
        sh(dir, &["commit", "-q", "-m", name]);
    }

    #[test]
    fn pull_goes_forward_and_push_goes_out() {
        let (a, b) = clones("pull");
        commit_file(&a, "paper.md", "two\n");
        push(&a.join("paper.md")).unwrap();
        let said = pull(&b.join("paper.md")).unwrap();
        assert!(!said.is_empty());
        assert_eq!(std::fs::read_to_string(b.join("paper.md")).unwrap(), "two\n");
        assert_eq!(state_of(&b.join("paper.md")).unwrap().behind, 0);
    }

    #[test]
    fn pull_refuses_what_cannot_go_forward() {
        let (a, b) = clones("diverge");
        commit_file(&a, "paper.md", "from a\n");
        push(&a.join("paper.md")).unwrap();
        commit_file(&b, "paper.md", "from b\n");
        let head = sh(&b, &["rev-parse", "HEAD"]);
        let e = pull(&b.join("paper.md")).unwrap_err();
        assert!(e.contains("Cannot fast-forward"), "{e}");
        // nothing was changed: no merge, no rebase
        assert_eq!(sh(&b, &["rev-parse", "HEAD"]), head);
        assert_eq!(std::fs::read_to_string(b.join("paper.md")).unwrap(), "from b\n");
        // and the push is refused by the remote, never forced
        assert!(push(&b.join("paper.md")).is_err());
    }

    #[test]
    fn push_needs_an_upstream() {
        let dir = repo("push");
        let e = push(&dir.join("paper.md")).unwrap_err();
        assert!(e.contains("no upstream"), "{e}");
        assert!(pull(&dir.join("paper.md")).unwrap_err().contains("no upstream"));
    }
}
