//! git (the `git` command, no library): the state of the repository of a document (branch, ahead and behind of its
//! upstream after the last fetch, the files changed), fetched quietly now and then. The versions of a document to
//! choose from (the tags and the last commits that touched it) and the text of one are here too: the builds and the
//! exchange use them. Nothing here rewrites history, merges or forces.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Webview, WebviewUrl, WebviewWindowBuilder};
use write_core::review::Item;
use write_core::review::compare::{as_items, suggestions};
use write_core::review::history::{self, Added, Attribution};

use crate::build::{find_tool, search_path};
use crate::documents;

/// What is kept of the repositories: when each was last fetched.
#[derive(Default)]
pub struct Git {
    fetched: Mutex<HashMap<PathBuf, Instant>>,
    /// the windows that show a document since a commit, by label: the document and the commit
    views: Mutex<HashMap<String, (PathBuf, String)>>,
    /// the number of the next such window
    next: AtomicUsize,
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
    /// the commit HEAD is at (its id; "(initial)" before the first commit): a commit made elsewhere moves it
    pub head: String,
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
                "branch.oid" => state.head = value.to_string(),
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
pub(crate) fn doc_of(window: &Webview) -> Result<PathBuf, String> {
    documents::path_of(window.app_handle(), window.label()).ok_or_else(|| "Save the document first (⌘S)".to_string())
}

/// Blocking work off the thread of the page.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

/// The repository of the window's document: none when it is untitled or not in a repository.
#[tauri::command]
pub async fn git_state(window: Webview) -> Result<Option<State>, String> {
    let Ok(doc) = doc_of(&window) else { return Ok(None) };
    blocking(move || state_of(&doc)).await
}

/// Fetches the upstream of the repository, quietly, for the ahead and behind: at most every 5 minutes for a
/// repository, and a failure (offline, no remote, a password asked for) is not told. Whether it fetched.
#[tauri::command]
pub async fn git_fetch(app: AppHandle, window: Webview) -> Result<bool, String> {
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
pub async fn git_changes(window: Webview, text: String) -> Result<Vec<Candidate>, String> {
    let doc = doc_of(&window)?;
    let bibs = crate::refs::own_bibs(&window, &text);
    blocking(move || changes_of(&doc, &bibs)).await?
}

/// The paths committed with the message; what git says.
#[tauri::command]
pub async fn git_commit(window: Webview, paths: Vec<String>, message: String) -> Result<String, String> {
    let doc = doc_of(&window)?;
    blocking(move || commit(&doc, &paths, &message)).await?
}

#[tauri::command]
pub async fn git_pull(window: Webview) -> Result<String, String> {
    let doc = doc_of(&window)?;
    blocking(move || pull(&doc)).await?
}

#[tauri::command]
pub async fn git_push(window: Webview) -> Result<String, String> {
    let doc = doc_of(&window)?;
    blocking(move || push(&doc)).await?
}

// ---- a co-author's edits as suggestions

/// The text of the document at `rev`.
fn text_at(doc: &Path, rev: &str) -> Result<String, String> {
    let name = doc.file_name().unwrap_or_default().to_string_lossy().into_owned();
    String::from_utf8(git_show(folder_of(doc), &name, rev)?).map_err(|_| format!("{name} at {rev}: not a UTF-8 text file"))
}

/// The commit `rev` names, in full; an error that says so when there is none.
fn commit_of(folder: &Path, rev: &str) -> Result<String, String> {
    git(folder, &["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")])
        .map(|h| h.trim().to_string())
        .map_err(|_| format!("{rev}: no such branch or commit"))
}

/// The edits made to `doc` from the commit `start` to `end` written as the suggestions of their authors (each hunk
/// under the commit that made it), on the text at `start` as their base. That text, and what was written.
fn as_suggestions(doc: &Path, start: &str, end: &str) -> Result<(String, Added), String> {
    let folder = folder_of(doc);
    let name = doc.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let (from, to) = (text_at(doc, start)?, text_at(doc, end)?);
    if from == to {
        return Ok((from, Added::default()));
    }
    let who = Attribution::read(|args| git(folder, args), &name, start, end)?;
    let newest = who.commits.last().ok_or_else(|| format!("no commit from {start} to {end}"))?;
    let changes = history::suggestions(&from, &to, |h| who.commit_of(&from, &to, h).unwrap_or(newest).clone());
    let added = history::add(doc, &from, &changes).map_err(|e| e.to_string())?;
    Ok((from, added))
}

/// The edits of the branch or commit `rev` since it left the history of HEAD (`git merge-base HEAD <rev>`) as
/// suggestions: only what was done there, never the undoing of what was written here since.
pub(crate) fn review_revision(doc: &Path, rev: &str) -> Result<Added, String> {
    let folder = folder_of(doc);
    let tip = commit_of(folder, rev)?;
    let base = git(folder, &["merge-base", "HEAD", &tip]).map_err(|_| format!("{rev} has no history in common with this branch"))?;
    let base = base.trim();
    if base == tip {
        return Err(format!(
            "{rev} is in the history of this branch already: its edits are in the text. Git ▸ Review Commits… turns committed edits back into suggestions."
        ));
    }
    Ok(as_suggestions(doc, base, &tip)?.1)
}

// ---- Review Branch…

/// A branch with edits of the document that HEAD does not have.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Branch {
    /// as git names it: "jane/edits", "origin/jane/edits"
    pub name: String,
    /// the authors of those commits, the latest first
    pub authors: Vec<String>,
    /// how many commits touched the document
    pub count: usize,
    /// the date of the latest, YYYY-MM-DD
    pub last: String,
}

/// The local and remote branches that have commits touching `doc` that HEAD does not (`git log HEAD..<branch> --
/// <doc>`), the latest first; a remote branch at the same commit as a local one is left out. With `fetch`, the remotes
/// are fetched first, quietly (a failure is not told: what is there is listed).
pub(crate) fn branches_of(doc: &Path, fetch: bool) -> Result<Vec<Branch>, String> {
    let folder = folder_of(doc);
    let top = toplevel(folder).ok_or("The document is not in a git repository")?;
    if fetch {
        let _ = git(&top, &["fetch", "--quiet"]);
    }
    let name = doc.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let refs = git(folder, &["for-each-ref", "--format=%(refname:short)%1f%(objectname)%1f%(symref)", "refs/heads", "refs/remotes"])?;
    let mut seen = std::collections::HashSet::new();
    let mut found = Vec::new();
    for line in refs.lines() {
        let mut f = line.split('\u{1f}');
        let (Some(branch), Some(oid)) = (f.next(), f.next()) else { continue };
        // origin/HEAD names another branch; the same commit is listed once
        if !f.next().unwrap_or("").is_empty() || !seen.insert(oid.to_string()) {
            continue;
        }
        let log = git(folder, &["log", "--format=%an%x1f%as", &format!("HEAD..{oid}"), "--", &name]).unwrap_or_default();
        let commits: Vec<(&str, &str)> = log.lines().filter_map(|l| l.split_once('\u{1f}')).collect();
        let Some(&(_, last)) = commits.first() else { continue };
        let mut authors: Vec<String> = Vec::new();
        for (a, _) in &commits {
            if !authors.iter().any(|x| x == a) {
                authors.push(a.to_string());
            }
        }
        found.push(Branch { name: branch.to_string(), authors, count: commits.len(), last: last.to_string() });
    }
    found.sort_by(|a, b| b.last.cmp(&a.last));
    Ok(found)
}

/// The branches with edits of the window's document that HEAD has not, for Git ▸ Review Branch… (fetched first).
#[tauri::command]
pub async fn review_branches(window: Webview, fetch: bool) -> Result<Vec<Branch>, String> {
    let doc = doc_of(&window)?;
    blocking(move || branches_of(&doc, fetch)).await?
}

// ---- Review Commits…

/// A commit of the current branch that touched the document, to review from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Recent {
    pub rev: String,
    /// the short hash
    pub label: String,
    /// YYYY-MM-DD
    pub date: String,
    pub author: String,
    pub subject: String,
    /// made by the user (their git e-mail, else name)
    pub mine: bool,
}

/// The last commits of the branch that touched the document, the latest first, and the one to review from to begin
/// with: the first after the user's own last one (none when that is the latest, or there is none of theirs).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Recents {
    pub commits: Vec<Recent>,
    pub first: Option<String>,
}

/// The last 30 commits of HEAD that touched `doc`, for Review Commits….
pub(crate) fn recent_of(doc: &Path) -> Result<Recents, String> {
    let folder = folder_of(doc);
    let name = doc.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let config = |key: &str| git_lines(folder, &["config", "--get", key]).map(|v| v.trim().to_lowercase()).filter(|v| !v.is_empty());
    let (email, me) = (config("user.email"), config("user.name"));
    let log = git(folder, &["log", "-n", "30", "--format=%H%x1f%h%x1f%as%x1f%an%x1f%ae%x1f%s", "HEAD", "--", &name])?;
    let commits: Vec<Recent> = log
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.splitn(6, '\u{1f}').collect();
            let [rev, label, date, author, mail, subject] = f.as_slice() else { return None };
            let mine = match &email {
                Some(e) => mail.to_lowercase() == *e,
                None => me.as_deref() == Some(author.to_lowercase().as_str()),
            };
            Some(Recent { rev: rev.to_string(), label: label.to_string(), date: date.to_string(), author: author.to_string(), subject: subject.to_string(), mine })
        })
        .collect();
    let first = commits.iter().position(|c| c.mine).and_then(|i| i.checked_sub(1)).map(|i| commits[i].rev.clone());
    Ok(Recents { commits, first })
}

/// What Review Commits… did: the suggestions written (by whom), and the commit the text went back to (short).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Reviewed {
    #[serde(flatten)]
    pub added: Added,
    pub since: String,
}

/// The edits committed to `doc` from the commit `first` to HEAD turned back into suggestions: each change of them
/// under the author of its commit, on the text before `first` as their base, and the file set back to that text, so
/// that accepting a suggestion brings its change back. The document must have no changes not committed (they would
/// be lost). Nothing is committed: the Commit sheet does it.
pub(crate) fn review_commits_of(doc: &Path, first: &str) -> Result<Reviewed, String> {
    let folder = folder_of(doc);
    let name = doc.file_name().unwrap_or_default().to_string_lossy().into_owned();
    if !git(folder, &["status", "--porcelain", "--", &name])?.trim().is_empty() {
        return Err(format!("{name} has changes not committed. Commit them first (Git ▸ Commit…): the text is set back to before the edits reviewed."));
    }
    let first = commit_of(folder, first)?;
    if git(folder, &["merge-base", "--is-ancestor", &first, "HEAD"]).is_err() {
        return Err(format!("{} is not on this branch (Git ▸ Review Branch… takes another branch's edits)", short(&first)));
    }
    let start = commit_of(folder, &format!("{first}^")).map_err(|_| format!("{} is the first commit: there is nothing before it to go back to", short(&first)))?;
    let head = commit_of(folder, "HEAD")?;
    let (text, added) = as_suggestions(doc, &start, &head)?;
    if read(doc)? != text {
        crate::files::write(doc, &text).map_err(|e| format!("{}: {e}", doc.display()))?;
    }
    Ok(Reviewed { added, since: short(&start).to_string() })
}

/// The last commits of the window's document on this branch, for Git ▸ Review Commits….
#[tauri::command]
pub async fn recent_commits(window: Webview) -> Result<Recents, String> {
    let doc = doc_of(&window)?;
    blocking(move || recent_of(&doc)).await?
}

/// The window's document from the commit `first` on, back as suggestions (the page has written it first).
#[tauri::command]
pub async fn review_commits(app: AppHandle, window: Webview, first: String) -> Result<Reviewed, String> {
    let doc = doc_of(&window)?;
    let made = blocking(move || review_commits_of(&doc, &first)).await??;
    let _ = app.emit_to(tauri::EventTarget::webview(window.label()), "review-changed", ());
    Ok(made)
}

// ---- Compare with Commit…

/// A document as it was at a commit, with what changed since as tracked changes.
#[derive(Serialize)]
pub struct History {
    /// "paper.md since v2"
    pub title: String,
    /// the file at the commit
    pub text: String,
    /// the changes from `text` to the file now: open suggestions of "since v2", ids `history-<n>`
    pub items: Vec<Item>,
    /// whether the document is Markdown (else plain monospace text)
    pub markdown: bool,
}

/// What a commit is called in a title: a hash shortened.
fn short(rev: &str) -> &str {
    if rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()) { &rev[..7] } else { rev }
}

/// The text of `doc` at `rev`, and the changes from it to `current` as items, drawn by the editor's review over the
/// text of `rev`. Applied all, they give `current`.
pub(crate) fn history_of(doc: &Path, rev: &str, current: &str) -> Result<History, String> {
    let name = doc.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let bytes = git_show(folder_of(doc), &name, rev)?;
    let text = String::from_utf8(bytes).map_err(|_| format!("{name} at {rev}: not a UTF-8 text file"))?;
    let label = short(rev);
    let items = as_items(&text, &suggestions(&text, current), &format!("since {label}"));
    let markdown = doc.extension().is_some_and(|e| ["md", "markdown", "txt"].iter().any(|m| e.eq_ignore_ascii_case(m)));
    Ok(History { title: format!("{name} since {label}"), text, items, markdown })
}

fn read(doc: &Path) -> Result<String, String> {
    let bytes = std::fs::read(doc).map_err(|e| format!("{}: {e}", doc.display()))?;
    String::from_utf8(bytes).map_err(|_| format!("{}: not a UTF-8 text file", doc.display()))
}

/// The window's document at `rev` in a tab of its own, read only, with the changes since as tracked changes (the
/// page has written the document: the file is what is compared). Nothing is saved from it.
#[tauri::command]
pub async fn compare_with_commit(app: AppHandle, window: Webview, rev: String) -> Result<(), String> {
    let doc = doc_of(&window)?;
    let (path, at) = (doc.clone(), rev.clone());
    // the commit is read first: a revision that does not have the file is told here, not in an empty tab
    let made = blocking(move || history_of(&path, &at, &read(&path)?)).await??;
    let state = app.state::<Git>();
    let label = format!("history-{}", state.next.fetch_add(1, Ordering::Relaxed) + 1);
    state.views.lock().unwrap().insert(label.clone(), (doc, rev));
    let joined = documents::front(&app);
    let built = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("/history".into()))
        .title(made.title)
        .inner_size(900.0, 700.0)
        .tabbing_identifier(documents::TABBING)
        .build();
    match built {
        Ok(new) => {
            match joined {
                Some(to) => crate::tabs::join(&new.as_ref().window(), &to.window()),
                None => {
                    let _ = new.set_focus();
                }
            }
            Ok(())
        }
        Err(e) => {
            closed(&app, &label);
            Err(e.to_string())
        }
    }
}

/// What a window of history shows: the document at the commit it was made for, and the changes since (the file as it
/// is now, so the tab asked again shows what has changed since it opened).
#[tauri::command]
pub async fn history(app: AppHandle, window: Webview) -> Result<History, String> {
    let (doc, rev) = app
        .state::<Git>()
        .views
        .lock()
        .unwrap()
        .get(window.label())
        .cloned()
        .ok_or("This window has no document to compare")?;
    blocking(move || history_of(&doc, &rev, &read(&doc)?)).await?
}

/// A window gone: what was kept of it goes.
pub fn closed(app: &AppHandle, label: &str) {
    app.state::<Git>().views.lock().unwrap().remove(label);
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
        assert_eq!(s.head, "1234567890abcdef");
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

    #[test]
    fn the_branches_with_edits_of_the_document() {
        let (a, b) = clones("branches");
        // jane's branch, pushed: edits of the document
        sh(&b, &["checkout", "-q", "-b", "jane/edits"]);
        commit_file(&b, "paper.md", "one, two\n");
        sh(&b, &["push", "-q", "-u", "origin", "jane/edits"]);
        // a branch that touches another file only, and one already merged
        sh(&b, &["checkout", "-q", "-b", "notes", "main"]);
        commit_file(&b, "notes.md", "n\n");
        sh(&b, &["push", "-q", "origin", "notes"]);
        sh(&a, &["branch", "old"]);
        let found = branches_of(&a.join("paper.md"), true).unwrap();
        let got: Vec<(&str, usize, &str)> = found.iter().map(|b| (b.name.as_str(), b.count, b.authors[0].as_str())).collect();
        assert_eq!(got, vec![("origin/jane/edits", 1, "b")]);
        // a local branch at the same commit is listed instead of the remote one
        sh(&a, &["branch", "jane/edits", "origin/jane/edits"]);
        let found = branches_of(&a.join("paper.md"), false).unwrap();
        assert_eq!(found.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), vec!["jane/edits"]);
    }

    #[test]
    fn committed_edits_come_back_as_suggestions() {
        let dir = repo("recall");
        let doc = dir.join("paper.md");
        let as_jane = |text: &str, subject: &str| {
            std::fs::write(&doc, text).unwrap();
            sh(&dir, &["-c", "user.name=Jane Doe", "-c", "user.email=jane@example.org", "commit", "-q", "-am", subject]);
        };
        as_jane("The line retreats slowly now.\n", "Slower");
        as_jane("The line retreats slowly now.\nReally.\n", "Really");
        let recent = recent_of(&doc).unwrap();
        let got: Vec<(&str, bool)> = recent.commits.iter().map(|c| (c.subject.as_str(), c.mine)).collect();
        assert_eq!(got, vec![("Really", false), ("Slower", false), ("First", true)]);
        assert_eq!(recent.first.as_deref(), Some(recent.commits[1].rev.as_str()), "the first after the user's own last");
        // not with changes not committed
        std::fs::write(&doc, "typed\n").unwrap();
        assert!(review_commits_of(&doc, &recent.commits[1].rev).unwrap_err().contains("not committed"));
        sh(&dir, &["checkout", "--", "paper.md"]);
        let done = review_commits_of(&doc, &recent.commits[1].rev).unwrap();
        assert_eq!((done.added.added, done.added.people.clone()), (2, vec!["Jane Doe".to_string()]));
        assert_eq!(done.since, sh(&dir, &["rev-parse", "--short=7", "HEAD~2"]).trim());
        // the text is back to before them, and accepting the suggestions brings the edits back
        let now = std::fs::read_to_string(&doc).unwrap();
        assert_eq!(now, "The line retreats rapidly now.\n");
        let items = write_core::review::Review::load(&doc).unwrap().items(&now);
        let rounds: Vec<&str> = items.iter().map(|i| i.record.round.as_deref().unwrap()).collect();
        assert!(rounds[0].ends_with(" Slower") && rounds[1].ends_with(" Really"), "{rounds:?}");
        assert_eq!(write_core::review::apply_all(&now, &items), "The line retreats slowly now.\nReally.\n");
        // the first commit has nothing before it
        let root = sh(&dir, &["rev-list", "--max-parents=0", "HEAD"]);
        sh(&dir, &["checkout", "--", "paper.md"]);
        assert!(review_commits_of(&doc, root.trim()).unwrap_err().contains("first commit"));
    }

    #[test]
    fn the_history_of_a_small_change_is_tracked_changes() {
        let dir = repo("history");
        let doc = dir.join("paper.md");
        sh(&dir, &["tag", "v2"]);
        let now = "The line retreats slowly now.\nA new line.\n";
        std::fs::write(&doc, now).unwrap();
        let h = history_of(&doc, "v2", now).unwrap();
        assert_eq!(h.title, "paper.md since v2");
        assert_eq!(h.text, "The line retreats rapidly now.\n");
        assert!(h.markdown);
        let got: Vec<(&str, &str, &str, Option<&str>)> =
            h.items.iter().map(|i| (i.id.as_str(), i.author.as_str(), i.record.exact.as_deref().unwrap_or(""), i.replacement.as_deref())).collect();
        assert_eq!(got[0].0, "history-1");
        assert_eq!(got[0].1, "since v2");
        assert_eq!(got[0].2, "rapidly");
        assert_eq!(got[0].3, Some("slowly"));
        // all applied to the text at the commit give the text now
        let mut out = h.text.clone();
        for i in h.items.iter().rev() {
            out.replace_range(i.range.clone(), i.replacement.as_deref().unwrap());
        }
        assert_eq!(out, now);
        // a commit is named by a short hash; a revision without the file says so
        let hash = sh(&dir, &["rev-parse", "HEAD"]);
        assert_eq!(history_of(&doc, hash.trim(), now).unwrap().title, format!("paper.md since {}", &hash.trim()[..7]));
        assert!(history_of(&doc, "nonsense", now).is_err());
    }
}
