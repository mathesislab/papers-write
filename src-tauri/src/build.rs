//! Builds: a saved Markdown document made into a PDF by texmark (Build), and a marked-up PDF of it against an earlier
//! version (Marked-up PDF…): the earlier text and the current one each converted to LaTeX by texmark, compared by
//! latexdiff, compiled by latexmk.
//!
//! Where things land, beside the document `name.md`: texmark's `build/name.tex`, `build/name.pdf` and `build/name.log`
//! (its folder holds the journal's class files and the bibliography too); for the marked-up PDF `build/diff/old/`
//! (the earlier text as LaTeX), `build/diff/new/` (the current text, built, and `diff.tex` with what latexmk makes of
//! it) and the PDF `build/diff/name-diff-<earlier>.pdf`.
//!
//! One build per document at a time: a request that comes while one runs waits for it, then runs once more (several
//! waiting are one). The page is told only the status (`build-status`): building, done (the time), failed with the
//! first errors of the LaTeX log. The page writes the document before it asks.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, EventTarget, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::documents;

/// The most errors of a LaTeX log that are kept.
const MAX_ERRORS: usize = 20;

/// The builds going on, by document.
#[derive(Default)]
pub struct Builds(Mutex<HashMap<PathBuf, Queue>>);

#[derive(Default)]
struct Queue {
    /// a build of the document is going on
    running: bool,
    /// what is to run once it ends
    pending: Option<Job>,
}

#[derive(Clone, Debug, PartialEq)]
enum Job {
    Build { show: bool },
    Diff(Earlier),
}

/// The earlier version of a marked-up PDF.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Earlier {
    /// a tag or commit of the document's git repository (`label` names it in the PDF's name)
    Git { rev: String, label: String },
    /// another file
    File { path: String },
}

/// An error of a LaTeX log: a line that starts with `! `, and the line of the `.tex` it was found at.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LatexError {
    pub message: String,
    pub line: Option<u32>,
    /// the text of the line at the error, as the log shows it
    pub context: Option<String>,
}

/// What a build came to, told to the page as `build-status`.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Status {
    /// "build" or "diff"
    pub what: &'static str,
    /// "building", "done" or "failed"
    pub state: &'static str,
    pub seconds: Option<f64>,
    /// the PDF, when there is one
    pub pdf: Option<String>,
    /// the LaTeX log
    pub log: Option<String>,
    pub errors: Vec<LatexError>,
    /// what went wrong where the log does not say (no texmark, a command that failed)
    pub message: Option<String>,
}

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

// ---- the tools

/// The folders searched for the tools, after PATH: a GUI app has hardly any PATH, and texmark is a Python script of
/// conda or pip, latexmk and pdflatex are in MacTeX's.
fn extra_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = [
        "/Library/TeX/texbin",
        "/opt/homebrew/bin",
        "/opt/homebrew/Caskroom/miniforge/base/bin",
        "/usr/local/bin",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join("miniforge3/bin"));
    }
    dirs
}

fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for dir in extra_dirs() {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// PATH for the commands run: the folders searched, so texmark finds pandoc and its filters and the engine.
fn search_path(extra: Option<&Path>) -> OsString {
    let mut dirs = search_dirs();
    if let Some(dir) = extra {
        dirs.insert(0, dir.to_path_buf());
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

/// The first of `dirs` that holds the file `name`.
fn find_in(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter().map(|d| d.join(name)).find(|p| p.is_file())
}

pub(crate) fn find_tool(name: &str) -> Result<PathBuf, String> {
    find_in(name, &search_dirs()).ok_or_else(|| match name {
        "texmark" => "texmark is not installed (pip install texmark); it was not found on PATH, in /opt/homebrew/Caskroom/miniforge/base/bin or ~/.local/bin".to_string(),
        "latexmk" | "latexdiff" | "pdflatex" => format!("{name} was not found: install MacTeX (https://tug.org/mactex/), which has it in /Library/TeX/texbin"),
        _ => format!("{name} was not found"),
    })
}

// ---- the commands, and what is read of their output

/// The arguments of texmark: `input` (relative to where it runs) to LaTeX and, with `pdf`, to a PDF; `tex` the
/// name of the LaTeX file, if not `<build>/<input's name>.tex`.
fn texmark_args(input: &str, build: &str, tex: Option<&str>, pdf: bool) -> Vec<String> {
    let mut args = vec![input.to_string(), "-d".to_string(), build.to_string()];
    if let Some(tex) = tex {
        args.extend(["--tex".to_string(), tex.to_string()]);
    }
    if pdf {
        args.push("--pdf".to_string());
    }
    args
}

/// The arguments of latexmk for `diff.tex`, the job named `job` (so that the PDF is `<job>.pdf`). `-f` goes on after
/// an error: a PDF with what could be set is better than none.
fn latexmk_args(job: &str) -> Vec<String> {
    ["-pdf", "-f", "-interaction=nonstopmode"]
        .iter()
        .map(|s| s.to_string())
        .chain([format!("-jobname={job}"), "diff.tex".to_string()])
        .collect()
}

/// The arguments of latexdiff; `--flatten` when the LaTeX reads other files.
fn latexdiff_args(old: &str, new: &str, flatten: bool) -> Vec<String> {
    let mut args = Vec::new();
    if flatten {
        args.push("--flatten".to_string());
    }
    args.extend([old.to_string(), new.to_string()]);
    args
}

fn needs_flatten(tex: &str) -> bool {
    tex.lines().any(|l| {
        let l = l.trim_start();
        !l.starts_with('%') && (l.contains("\\input{") || l.contains("\\include{"))
    })
}

/// A name for a file made of what names an earlier version: letters, digits, `-` and `_` kept, the rest `_`.
fn safe_label(label: &str) -> String {
    let s: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "earlier".into() } else { s }
}

/// The errors of a LaTeX log: each line that starts with `! `, with the line number and text of the `l.<n> ` line
/// that follows it (within a few lines, before another error). The same error twice is one.
fn parse_log_errors(log: &str) -> Vec<LatexError> {
    let lines: Vec<&str> = log.lines().collect();
    let mut errors: Vec<LatexError> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(message) = line.strip_prefix("! ") else {
            continue;
        };
        let mut error = LatexError {
            message: message.trim().to_string(),
            line: None,
            context: None,
        };
        for next in lines.iter().skip(i + 1).take(8) {
            if next.starts_with("! ") {
                break;
            }
            if let Some(rest) = next.strip_prefix("l.") {
                let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = digits.parse() {
                    error.line = Some(n);
                    let text = rest[digits.len()..].trim();
                    if !text.is_empty() {
                        error.context = Some(text.to_string());
                    }
                    break;
                }
            }
        }
        if !errors.contains(&error) {
            errors.push(error);
        }
        if errors.len() == MAX_ERRORS {
            break;
        }
    }
    errors
}

/// The versions of a file from `git log --format=%H%x1f%h%x1f%as%x1f%an%x1f%s`.
fn parse_commits(output: &str) -> Vec<Revision> {
    output
        .lines()
        .filter_map(|line| {
            let mut f = line.split('\u{1f}');
            let (hash, short, date, author, subject) =
                (f.next()?, f.next()?, f.next()?, f.next()?, f.next()?);
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
fn parse_tags(output: &str) -> Vec<Revision> {
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

/// The last lines of what a command printed to stderr (or else stdout), for a message.
fn tail(bytes: &[u8], stdout: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let text = if text.trim().is_empty() {
        String::from_utf8_lossy(stdout)
    } else {
        text
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(6)..].join("\n")
}

// ---- running

fn run(program: &Path, args: &[String], cwd: &Path) -> Result<std::process::Output, String> {
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        .env("PATH", search_path(program.parent()))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", program.display()))
}

/// A command that must succeed: its output, else what it said.
fn run_ok(program: &Path, args: &[String], cwd: &Path) -> Result<std::process::Output, String> {
    let out = run(program, args, cwd)?;
    if out.status.success() {
        Ok(out)
    } else {
        let name = program.file_name().unwrap_or_default().to_string_lossy();
        Err(format!(
            "{name} failed:\n{}",
            tail(&out.stderr, &out.stdout)
        ))
    }
}

fn s(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn modified_since(path: &Path, start: SystemTime) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|t| t >= start)
}

/// The status of an ended build: the log's errors (as long as the log is of this build), and the PDF if it is.
fn finish(
    what: &'static str,
    ok: bool,
    log: &Path,
    pdf: &Path,
    start: SystemTime,
    took: Duration,
    message: Option<String>,
) -> Status {
    let log_text = if modified_since(log, start) {
        fs::read_to_string(log).ok()
    } else {
        None
    };
    let errors = log_text
        .as_deref()
        .map(parse_log_errors)
        .unwrap_or_default();
    let fresh = modified_since(pdf, start);
    Status {
        what,
        state: if ok { "done" } else { "failed" },
        seconds: Some(took.as_secs_f64()),
        pdf: fresh.then(|| s(pdf)),
        log: log_text.is_some().then(|| s(log)),
        message: if ok || !errors.is_empty() {
            None
        } else {
            message
        },
        errors,
    }
}

fn failed(what: &'static str, message: String) -> Status {
    Status {
        what,
        state: "failed",
        message: Some(message),
        ..Default::default()
    }
}

/// `file` (a Markdown document) into `build/` beside it by texmark, to LaTeX and a PDF.
pub fn run_build(file: &Path) -> Status {
    let (Some(folder), Some(name), Some(stem)) =
        (file.parent(), file.file_name(), file.file_stem())
    else {
        return failed("build", format!("{}: not a file", file.display()));
    };
    let texmark = match find_tool("texmark") {
        Ok(t) => t,
        Err(e) => return failed("build", e),
    };
    let (start, clock) = (SystemTime::now() - Duration::from_secs(1), Instant::now());
    let stem = stem.to_string_lossy();
    let out = run(
        &texmark,
        &texmark_args(&name.to_string_lossy(), "build", None, true),
        folder,
    );
    let took = clock.elapsed();
    let build = folder.join("build");
    let (log, pdf) = (
        build.join(format!("{stem}.log")),
        build.join(format!("{stem}.pdf")),
    );
    match out {
        Ok(out) => finish(
            "build",
            out.status.success(),
            &log,
            &pdf,
            start,
            took,
            Some(format!(
                "texmark failed:\n{}",
                tail(&out.stderr, &out.stdout)
            )),
        ),
        Err(e) => failed("build", e),
    }
}

/// The text of `file` as it was at `rev`.
pub(crate) fn git_show(folder: &Path, name: &str, rev: &str) -> Result<Vec<u8>, String> {
    let git = find_tool("git")?;
    let out = run_ok(&git, &["show".into(), format!("{rev}:./{name}")], folder)?;
    Ok(out.stdout)
}

/// A file made for a while: removed when it goes out of scope.
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// The marked-up PDF of `file` against `earlier`; see the module.
pub fn run_diff(file: &Path, earlier: &Earlier) -> Status {
    let (clock, start) = (Instant::now(), SystemTime::now() - Duration::from_secs(1));
    let (Some(folder), Some(name), Some(stem)) =
        (file.parent(), file.file_name(), file.file_stem())
    else {
        return failed("diff", format!("{}: not a file", file.display()));
    };
    let (name, stem) = (
        name.to_string_lossy().into_owned(),
        stem.to_string_lossy().into_owned(),
    );
    let label = match earlier {
        Earlier::Git { label, .. } => safe_label(label),
        Earlier::File { path } => safe_label(
            &Path::new(path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy(),
        ),
    };
    let job = format!("{stem}-diff-{label}");
    let diff = folder.join("build/diff");
    let (old_dir, new_dir) = (diff.join("old"), diff.join("new"));
    let log = new_dir.join(format!("{job}.log"));
    let result = (|| -> Result<(), String> {
        let (texmark, latexdiff, latexmk) = (
            find_tool("texmark")?,
            find_tool("latexdiff")?,
            find_tool("latexmk")?,
        );
        fs::create_dir_all(&old_dir).map_err(|e| format!("{}: {e}", old_dir.display()))?;
        // the earlier text, as a file: beside the document (its figures are found as the document's are) when it comes
        // from git, else where it is
        let _kept;
        let earlier_input = match earlier {
            Earlier::Git { rev, .. } => {
                let text = git_show(folder, &name, rev)?;
                let temporary = folder.join(format!(".{stem}.earlier.md"));
                fs::write(&temporary, text).map_err(|e| format!("{}: {e}", temporary.display()))?;
                _kept = Temporary(temporary.clone());
                s(&temporary)
            }
            Earlier::File { path } => path.clone(),
        };
        // the earlier text as LaTeX only; the current one built, which brings the journal's class files and the
        // bibliography into its folder, where latexmk runs
        let old_tex = old_dir.join("old.tex");
        let new_tex = new_dir.join("new.tex");
        run_ok(
            &texmark,
            &texmark_args(
                &earlier_input,
                "build/diff/old",
                Some("build/diff/old/old.tex"),
                false,
            ),
            folder,
        )
        .map_err(|e| format!("The earlier version:\n{e}"))?;
        run_ok(
            &texmark,
            &texmark_args(
                &name,
                "build/diff/new",
                Some("build/diff/new/new.tex"),
                true,
            ),
            folder,
        )
        .map_err(|e| format!("The current version:\n{e}"))?;
        let flatten = needs_flatten(&fs::read_to_string(&new_tex).unwrap_or_default());
        let marked = run_ok(
            &latexdiff,
            &latexdiff_args(&s(&old_tex), &s(&new_tex), flatten),
            folder,
        )?;
        fs::write(new_dir.join("diff.tex"), marked.stdout).map_err(|e| format!("diff.tex: {e}"))?;
        // latexmk's own result is the PDF's: with `-f` it goes on after errors
        run(&latexmk, &latexmk_args(&job), &new_dir)?;
        let made = new_dir.join(format!("{job}.pdf"));
        if !modified_since(&made, start) {
            return Err("latexmk made no PDF".into());
        }
        // copied (not moved): a PDF window showing it keeps its place
        fs::copy(&made, diff.join(format!("{job}.pdf")))
            .map_err(|e| format!("{}: {e}", made.display()))?;
        Ok(())
    })();
    let pdf = diff.join(format!("{job}.pdf"));
    match result {
        Ok(()) => finish("diff", true, &log, &pdf, start, clock.elapsed(), None),
        Err(e) => {
            let mut status = finish("diff", false, &log, &pdf, start, clock.elapsed(), Some(e));
            // a PDF of an earlier try is not this one's
            status.pdf = None;
            status
        }
    }
}

// ---- the page's side

fn tell(app: &AppHandle, label: &str, status: Status) {
    let _ = app.emit_to(EventTarget::webview(label), "build-status", status);
}

/// A request: run it, or keep it for when the build of the document going on ends.
fn request(app: &AppHandle, window: &WebviewWindow, file: PathBuf, job: Job) {
    let state = app.state::<Builds>();
    {
        let mut builds = state.0.lock().unwrap();
        let queue = builds.entry(file.clone()).or_default();
        if queue.running {
            queue.pending = Some(match (queue.pending.take(), job) {
                (Some(Job::Build { show: a }), Job::Build { show: b }) => {
                    Job::Build { show: a || b }
                }
                (_, job) => job,
            });
            return;
        }
        queue.running = true;
    }
    let (app, label) = (app.clone(), window.label().to_string());
    tauri::async_runtime::spawn_blocking(move || {
        let mut job = job;
        loop {
            let what = if matches!(job, Job::Build { .. }) {
                "build"
            } else {
                "diff"
            };
            tell(
                &app,
                &label,
                Status {
                    what,
                    state: "building",
                    ..Default::default()
                },
            );
            let status = match &job {
                Job::Build { .. } => run_build(&file),
                Job::Diff(earlier) => run_diff(&file, earlier),
            };
            let show = matches!(job, Job::Build { show: true } | Job::Diff(_));
            if show
                && status.state != "failed"
                && let Some(pdf) = &status.pdf
                && let Err(e) = documents::open_file(&app, Path::new(pdf))
            {
                eprintln!("{e}");
            }
            tell(&app, &label, status);
            let queues = app.state::<Builds>();
            let mut builds = queues.0.lock().unwrap();
            match builds.get_mut(&file).and_then(|q| q.pending.take()) {
                Some(next) => job = next,
                None => {
                    builds.remove(&file);
                    break;
                }
            }
        }
    });
}

/// The Markdown document of the window, to build.
fn document_of(window: &WebviewWindow) -> Result<PathBuf, String> {
    let path = documents::path_of(window.app_handle(), window.label())
        .ok_or("Save the document first (⌘S): texmark builds a file")?;
    let markdown = path
        .extension()
        .is_some_and(|e| ["md", "markdown"].iter().any(|m| e.eq_ignore_ascii_case(m)));
    if markdown {
        Ok(path)
    } else {
        Err("texmark builds Markdown documents (.md)".into())
    }
}

/// Build (⌘B), or with `show` Build and Show PDF (⇧⌘B). The page has written the document.
#[tauri::command]
pub fn build(app: AppHandle, window: WebviewWindow, show: bool) -> Result<(), String> {
    let file = document_of(&window)?;
    request(&app, &window, file, Job::Build { show });
    Ok(())
}

/// The marked-up PDF of the document against `earlier`.
#[tauri::command]
pub fn marked_up(app: AppHandle, window: WebviewWindow, earlier: Earlier) -> Result<(), String> {
    let file = document_of(&window)?;
    request(&app, &window, file, Job::Diff(earlier));
    Ok(())
}

pub(crate) fn git_lines(folder: &Path, args: &[&str]) -> Option<String> {
    let git = find_tool("git").ok()?;
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let out = run(&git, &args, folder).ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The versions of the window's document in git: the tags, and the last 30 commits that touched it; none when it is
/// not in a repository.
#[tauri::command]
pub async fn revisions(window: WebviewWindow) -> Result<Vec<Revision>, String> {
    let file = document_of(&window)?;
    let folder = file.parent().ok_or("no folder")?.to_path_buf();
    let name = file
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    tauri::async_runtime::spawn_blocking(move || {
        if git_lines(&folder, &["rev-parse", "--is-inside-work-tree"]).is_none() {
            return Vec::new();
        }
        let mut found = git_lines(
            &folder,
            &[
                "tag",
                "--sort=-creatordate",
                "--format=%(refname:short)%1f%(creatordate:short)%1f%(subject)",
            ],
        )
        .map(|o| parse_tags(&o))
        .unwrap_or_default();
        found.truncate(50);
        found.extend(
            git_lines(
                &folder,
                &[
                    "log",
                    "-n",
                    "30",
                    "--format=%H%x1f%h%x1f%as%x1f%an%x1f%s",
                    "--",
                    &name,
                ],
            )
            .map(|o| parse_commits(&o))
            .unwrap_or_default(),
        );
        found
    })
    .await
    .map_err(|e| e.to_string())
}

/// The file to compare with, chosen in a dialog; none when it was cancelled.
#[tauri::command]
pub async fn earlier_file(app: AppHandle, window: WebviewWindow) -> Result<Option<String>, String> {
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Earlier version")
        .add_filter("Markdown", &["md", "markdown"]);
    if let Ok(file) = document_of(&window)
        && let Some(folder) = file.parent()
    {
        dialog = dialog.set_directory(folder);
    }
    Ok(dialog
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok())
        .map(|p| s(&p)))
}

/// The file shown in Finder.
#[tauri::command]
pub fn reveal(path: String) -> Result<(), String> {
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|e| format!("{path}: {e}"))
}

/// A file opened in a tab (the LaTeX log as text).
#[tauri::command]
pub fn open_document(app: AppHandle, path: String) -> Result<(), String> {
    documents::open_file(&app, Path::new(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texmark_in_the_documents_folder() {
        assert_eq!(
            texmark_args("paper.md", "build", None, true),
            ["paper.md", "-d", "build", "--pdf"]
        );
        assert_eq!(
            texmark_args(
                "paper.md",
                "build/diff/old",
                Some("build/diff/old/old.tex"),
                false
            ),
            [
                "paper.md",
                "-d",
                "build/diff/old",
                "--tex",
                "build/diff/old/old.tex"
            ]
        );
    }

    #[test]
    fn latexmk_and_latexdiff() {
        assert_eq!(
            latexmk_args("p-diff-v1"),
            [
                "-pdf",
                "-f",
                "-interaction=nonstopmode",
                "-jobname=p-diff-v1",
                "diff.tex"
            ]
        );
        assert_eq!(latexdiff_args("a.tex", "b.tex", false), ["a.tex", "b.tex"]);
        assert_eq!(
            latexdiff_args("a.tex", "b.tex", true),
            ["--flatten", "a.tex", "b.tex"]
        );
        assert!(needs_flatten("\\begin{document}\n\\input{body}\n"));
        assert!(!needs_flatten("% \\input{body}\ntext"));
    }

    #[test]
    fn names_of_versions() {
        assert_eq!(safe_label("v1.0/rc 2"), "v1_0_rc_2");
        assert_eq!(safe_label("a1b2c3d"), "a1b2c3d");
        assert_eq!(safe_label(""), "earlier");
    }

    #[test]
    fn errors_of_a_log() {
        let log = "This is pdfTeX\n(./paper.tex\n! Undefined control sequence.\nl.42 Some \\foo\n                 text here\n\n! LaTeX Error: File `x.sty' not found.\n\nType X to quit\nl.7 \\usepackage{x}\n\n! Undefined control sequence.\nl.42 Some \\foo\n\n! Emergency stop.\n<*> paper.tex\n\nNo pages of output.\n";
        let errors = parse_log_errors(log);
        assert_eq!(errors.len(), 3);
        assert_eq!(errors[0].message, "Undefined control sequence.");
        assert_eq!(errors[0].line, Some(42));
        assert_eq!(errors[0].context.as_deref(), Some("Some \\foo"));
        assert_eq!(errors[1].message, "LaTeX Error: File `x.sty' not found.");
        assert_eq!(errors[1].line, Some(7));
        // the same error at the same line is one; an error with no line has none
        assert_eq!(errors[2].message, "Emergency stop.");
        assert_eq!(errors[2].line, None);
        assert!(parse_log_errors("all fine\n").is_empty());
    }

    #[test]
    fn errors_stop_at_the_next() {
        // the l. line after another error is not this one's
        let errors = parse_log_errors("! First.\n! Second.\nl.3 x\n");
        assert_eq!(errors[0].line, None);
        assert_eq!(errors[1].line, Some(3));
    }

    #[test]
    fn git_versions() {
        let commits = parse_commits(
            "abc123def\u{1f}abc123d\u{1f}2026-10-01\u{1f}Alex\u{1f}Methods: rewritten\nbad line\n",
        );
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].rev, "abc123def");
        assert_eq!(commits[0].label, "abc123d");
        assert_eq!(commits[0].detail, "2026-10-01 Alex: Methods: rewritten");
        let tags = parse_tags("v2\u{1f}2026-09-01\u{1f}Submitted\nv1\u{1f}2026-05-01\u{1f}\n");
        assert_eq!(tags.len(), 2);
        assert_eq!(
            (
                tags[0].kind,
                tags[0].label.as_str(),
                tags[0].detail.as_str()
            ),
            ("tag", "v2", "2026-09-01 Submitted")
        );
        assert_eq!(tags[1].detail, "2026-05-01");
    }

    #[test]
    fn tools_found_in_folders() {
        let dir = std::env::temp_dir().join(format!("pw-tools-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("texmark"), "").unwrap();
        let dirs = [PathBuf::from("/nonexistent"), dir.clone()];
        assert_eq!(find_in("texmark", &dirs), Some(dir.join("texmark")));
        assert_eq!(find_in("latexmk", &dirs), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn tail_of_output() {
        assert_eq!(tail(b"a\n\nb\n", b""), "a\nb");
        assert_eq!(tail(b"", b"from stdout"), "from stdout");
        assert_eq!(tail(b"1\n2\n3\n4\n5\n6\n7\n8", b"").lines().count(), 6);
    }

    /// A real build and a real marked-up PDF of a small paper in a git repository of the scratch area (needs texmark,
    /// latexdiff, latexmk and pdflatex): `cargo test -p papers-write real_ -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_build_and_marked_up() {
        let dir = std::env::temp_dir().join(format!("pw-real-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let paper = dir.join("paper.md");
        let text = "---\ntitle: \"A small test\"\nauthors:\n  - firstname: A.\n    lastname: Author\n    affiliation: 1\n    email: a@b.org\naffiliations:\n  - \"Institute, Place\"\ndate: \"2026\"\nbibliography: references.bib\njournal:\n  template: copernicus\n  short: tc\n---\n\n# Abstract\n\nShort abstract.\n\n# Introduction\n\nIce sheets matter [@lisiecki2005].\n\n$$a = b + c$$\n\n# Methods\n\nWe used a model.\n";
        fs::write(&paper, text).unwrap();
        fs::write(dir.join("references.bib"), "@article{lisiecki2005, author={Lisiecki, L. and Raymo, M.}, title={A stack}, journal={Paleoceanography}, year={2005}}\n").unwrap();
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .args(["-c", "user.name=T", "-c", "user.email=t@t"])
                .args(args)
                .current_dir(&dir)
                .status()
                .unwrap();
            assert!(ok.success());
        };
        git(&["init", "-q"]);
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "First"]);
        git(&["tag", "v1"]);
        fs::write(
            &paper,
            text.replace("We used a model.", "We used a new model, and ran it twice."),
        )
        .unwrap();

        let built = run_build(&paper);
        println!(
            "build: {} in {:.1} s, pdf {:?}, errors {:?}, {:?}",
            built.state,
            built.seconds.unwrap_or(0.0),
            built.pdf,
            built.errors,
            built.message
        );
        assert_eq!(built.state, "done");
        assert!(dir.join("build/paper.pdf").is_file());

        let revisions = {
            let tags = git_lines(
                &dir,
                &[
                    "tag",
                    "--format=%(refname:short)%1f%(creatordate:short)%1f%(subject)",
                ],
            )
            .unwrap();
            parse_tags(&tags)
        };
        assert_eq!(revisions[0].label, "v1");
        let diffed = run_diff(
            &paper,
            &Earlier::Git {
                rev: "v1".into(),
                label: "v1".into(),
            },
        );
        println!(
            "diff: {} in {:.1} s, pdf {:?}, errors {:?}, {:?}",
            diffed.state,
            diffed.seconds.unwrap_or(0.0),
            diffed.pdf,
            diffed.errors,
            diffed.message
        );
        assert_eq!(diffed.state, "done");
        assert!(dir.join("build/diff/paper-diff-v1.pdf").is_file());
        // another file, in another folder
        let other = std::env::temp_dir().join(format!("pw-real-other-{}", std::process::id()));
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("draft1.md"), text).unwrap();
        let from_file = run_diff(
            &paper,
            &Earlier::File {
                path: s(&other.join("draft1.md")),
            },
        );
        println!(
            "diff against a file: {} in {:.1} s, pdf {:?}, errors {:?}, {:?}",
            from_file.state,
            from_file.seconds.unwrap_or(0.0),
            from_file.pdf,
            from_file.errors,
            from_file.message
        );
        assert_eq!(from_file.state, "done");
        assert!(dir.join("build/diff/paper-diff-draft1.pdf").is_file());
        println!("kept in {}", dir.display());
    }
}
