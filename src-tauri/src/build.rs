//! Builds: a saved Markdown document made into a PDF by texmark (Build), and a marked-up PDF of it against an earlier
//! version (Marked-up PDF…): the earlier text and the current one each converted to LaTeX by texmark, compared by
//! latexdiff, compiled by latexmk.
//!
//! A LaTeX document (`name.tex`) is built by latexmk itself, in its folder, with `-outdir` the build folder (as texmark's, so
//! `build/name.pdf` and `build/name.log`) and `-file-line-error`, so that the log's errors are `file:line:` and
//! the ones in the document are lines of the editor's text. Its marked-up PDF is latexdiff of the earlier `.tex` and
//! the current one (no texmark), compiled by latexmk in the folder with its output in `build/diff/`.
//!
//! Where things land: in the build folder of the document's project (`build` of `write.toml`, relative to the
//! project's folder), else in `build/` beside the document. Called `build/` below. For the document `name.md`: texmark's `build/name.tex`, `build/name.pdf` and `build/name.log`
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
use write_core::project;

use crate::documents;
// the git helpers live in `git`; `exchange` reaches them through here
pub(crate) use crate::git::{Revision, git_lines, git_show};
use crate::git;

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

/// What a document is built from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Markdown (`.md`, `.markdown`): by texmark
    Markdown,
    /// LaTeX (`.tex`): by latexmk
    Latex,
}

/// The kind of document of `path` by its extension, none for one that cannot be built.
pub fn kind_of(path: &Path) -> Option<Kind> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "md" | "markdown" => Some(Kind::Markdown),
        "tex" => Some(Kind::Latex),
        _ => None,
    }
}

/// An error of a LaTeX log: a line that starts with `! ` or, with `-file-line-error`, `file:line: `, and the line of
/// the `.tex` it was found at.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LatexError {
    pub message: String,
    pub line: Option<u32>,
    /// the file the log names (with `-file-line-error`)
    pub file: Option<String>,
    /// the line is one of the document the window shows (a LaTeX document's own, not an input file's, nor a
    /// Markdown document's converted one)
    pub here: bool,
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
pub(crate) fn search_path(extra: Option<&Path>) -> OsString {
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
    found_tool(name)
}

/// The tool as it is found without the settings: on PATH, else in the folders searched.
pub(crate) fn found_tool(name: &str) -> Result<PathBuf, String> {
    find_in(name, &search_dirs()).ok_or_else(|| match name {
        "texmark" => "texmark is not installed (pip install texmark); it was not found on PATH, in /opt/homebrew/Caskroom/miniforge/base/bin or ~/.local/bin".to_string(),
        "latexmk" | "latexdiff" | "pdflatex" => format!("{name} was not found: install MacTeX (https://tug.org/mactex/), which has it in /Library/TeX/texbin"),
        "pandoc" => "pandoc is not installed: brew install pandoc".to_string(),
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

/// The arguments of latexmk for the LaTeX document `source` (relative to where it runs): its PDF and log in `outdir`,
/// the errors as `file:line:`. With `force`, it goes on after an error (a PDF with what could be set is better than
/// none); `job` names the output, else it is the source's name.
fn latexmk_document_args(source: &str, outdir: &str, job: Option<&str>, force: bool) -> Vec<String> {
    let mut args: Vec<String> = ["-pdf", "-interaction=nonstopmode", "-file-line-error"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    if force {
        args.push("-f".into());
    }
    args.push(format!("-outdir={outdir}"));
    if let Some(job) = job {
        args.push(format!("-jobname={job}"));
    }
    args.push(source.to_string());
    args
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

/// A line of a log of `-file-line-error`: `./paper.tex:42: Undefined control sequence.` as file, line and message.
fn file_line_error(line: &str) -> Option<(&str, u32, &str)> {
    let (file, rest) = line.split_once(':')?;
    if file.is_empty() || file.contains(char::is_whitespace) || !file.contains('.') {
        return None;
    }
    let (digits, message) = rest.split_once(": ")?;
    Some((file, digits.parse().ok()?, message.trim()))
}

/// The errors of a LaTeX log: each line that starts with `! ` or is `file:line: message` (`-file-line-error`), with
/// the line number and text of the `l.<n> ` line that follows it (within a few lines, before another error). The same
/// error twice is one.
fn parse_log_errors(log: &str) -> Vec<LatexError> {
    let lines: Vec<&str> = log.lines().collect();
    let mut errors: Vec<LatexError> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let located = file_line_error(line);
        let Some(message) = line.strip_prefix("! ").or(located.map(|(_, _, m)| m)) else {
            continue;
        };
        let mut error = LatexError {
            message: message.trim().to_string(),
            line: located.map(|(_, n, _)| n),
            file: located.map(|(f, _, _)| f.trim_start_matches("./").to_string()),
            here: false,
            context: None,
        };
        for next in lines.iter().skip(i + 1).take(8) {
            if next.starts_with("! ") || file_line_error(next).is_some() {
                break;
            }
            if let Some(rest) = next.strip_prefix("l.") {
                let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = digits.parse() {
                    error.line.get_or_insert(n);
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

/// `file` (a LaTeX document) into `build/` beside it by latexmk; the errors of the document's own lines are marked
/// `here`.
fn run_latex_build(file: &Path) -> Status {
    let (Some(folder), Some(name), Some(stem)) = (file.parent(), file.file_name(), file.file_stem()) else {
        return failed("build", format!("{}: not a file", file.display()));
    };
    let latexmk = match find_tool("latexmk") {
        Ok(t) => t,
        Err(e) => return failed("build", e),
    };
    let (start, clock) = (SystemTime::now() - Duration::from_secs(1), Instant::now());
    let (name, stem) = (name.to_string_lossy(), stem.to_string_lossy());
    let build = project::build_dir_of(file);
    let out = run(&latexmk, &latexmk_document_args(&name, &s(&build), None, false), folder);
    let took = clock.elapsed();
    let (log, pdf) = (build.join(format!("{stem}.log")), build.join(format!("{stem}.pdf")));
    match out {
        Ok(out) => {
            let mut status = finish(
                "build",
                out.status.success(),
                &log,
                &pdf,
                start,
                took,
                Some(format!("latexmk failed:\n{}", tail(&out.stderr, &out.stdout))),
            );
            mark_here(&mut status, &name);
            status
        }
        Err(e) => failed("build", e),
    }
}

/// The errors at lines of the file `name` marked `here`.
fn mark_here(status: &mut Status, name: &str) {
    for e in &mut status.errors {
        e.here = e.file.as_deref() == Some(name) && e.line.is_some();
    }
}

/// `file` (a Markdown document) into `build/` beside it by texmark, to LaTeX and a PDF.
pub fn run_build(file: &Path) -> Status {
    if kind_of(file) == Some(Kind::Latex) {
        return run_latex_build(file);
    }
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
    let build = project::build_dir_of(file);
    let out = run(
        &texmark,
        &texmark_args(&name.to_string_lossy(), &s(&build), None, true),
        folder,
    );
    let took = clock.elapsed();
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

/// A file made for a while: removed when it goes out of scope.
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// The name of the PDF of a marked-up document: `<stem>-diff-<label of the earlier version>`.
fn diff_job(stem: &str, earlier: &Earlier) -> String {
    let label = match earlier {
        Earlier::Git { label, .. } => safe_label(label),
        Earlier::File { path } => safe_label(&Path::new(path).file_stem().unwrap_or_default().to_string_lossy()),
    };
    format!("{stem}-diff-{label}")
}

/// The marked-up PDF of the LaTeX document `file` against `earlier`: latexdiff of the two `.tex` files, run in the
/// document's folder (what it reads is found as the document's own is), the result `build/diff/diff.tex` compiled by
/// latexmk there into `build/diff/<job>.pdf`.
fn run_latex_diff(file: &Path, earlier: &Earlier) -> Status {
    let (clock, start) = (Instant::now(), SystemTime::now() - Duration::from_secs(1));
    let (Some(folder), Some(name), Some(stem)) = (file.parent(), file.file_name(), file.file_stem()) else {
        return failed("diff", format!("{}: not a file", file.display()));
    };
    let (name, stem) = (name.to_string_lossy().into_owned(), stem.to_string_lossy().into_owned());
    let job = diff_job(&stem, earlier);
    let diff = project::build_dir_of(file).join("diff");
    let log = diff.join(format!("{job}.log"));
    let pdf = diff.join(format!("{job}.pdf"));
    let result = (|| -> Result<(), String> {
        let (latexdiff, latexmk) = (find_tool("latexdiff")?, find_tool("latexmk")?);
        fs::create_dir_all(&diff).map_err(|e| format!("{}: {e}", diff.display()))?;
        let old = match earlier {
            Earlier::Git { rev, .. } => {
                let old = diff.join("old.tex");
                fs::write(&old, git_show(folder, &name, rev)?).map_err(|e| format!("{}: {e}", old.display()))?;
                old
            }
            Earlier::File { path } => PathBuf::from(path),
        };
        let flatten = needs_flatten(&fs::read_to_string(file).unwrap_or_default());
        let marked = run_ok(&latexdiff, &latexdiff_args(&s(&old), &name, flatten), folder)?;
        fs::write(diff.join("diff.tex"), marked.stdout).map_err(|e| format!("diff.tex: {e}"))?;
        // `-f`: latexmk goes on after errors, and the PDF is its result
        run(&latexmk, &latexmk_document_args(&s(&diff.join("diff.tex")), &s(&diff), Some(&job), true), folder)?;
        if !modified_since(&pdf, start) {
            return Err("latexmk made no PDF".into());
        }
        Ok(())
    })();
    match result {
        Ok(()) => finish("diff", true, &log, &pdf, start, clock.elapsed(), None),
        Err(e) => {
            let mut status = finish("diff", false, &log, &pdf, start, clock.elapsed(), Some(e));
            status.pdf = None;
            status
        }
    }
}

/// The marked-up PDF of `file` against `earlier`; see the module.
pub fn run_diff(file: &Path, earlier: &Earlier) -> Status {
    if kind_of(file) == Some(Kind::Latex) {
        return run_latex_diff(file, earlier);
    }
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
    let job = diff_job(&stem, earlier);
    let diff = project::build_dir_of(file).join("diff");
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
                &s(&old_dir),
                Some(&s(&old_tex)),
                false,
            ),
            folder,
        )
        .map_err(|e| format!("The earlier version:\n{e}"))?;
        run_ok(
            &texmark,
            &texmark_args(
                &name,
                &s(&new_dir),
                Some(&s(&new_tex)),
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

/// The document of the window, to build: Markdown (texmark) or LaTeX (latexmk).
fn document_of(window: &WebviewWindow) -> Result<PathBuf, String> {
    let path = documents::path_of(window.app_handle(), window.label())
        .ok_or("Save the document first (⌘S): a file is built")?;
    if kind_of(&path).is_some() {
        Ok(path)
    } else {
        Err("Build is for Markdown (.md, by texmark) and LaTeX (.tex, by latexmk) documents".into())
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

/// The versions of the window's document in git: the tags, and the last 30 commits that touched it (`git`); none
/// when it is not in a repository.
#[tauri::command]
pub async fn revisions(window: WebviewWindow) -> Result<Vec<Revision>, String> {
    let file = document_of(&window)?;
    tauri::async_runtime::spawn_blocking(move || git::revisions_of(&file))
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
    fn latexmk_of_a_tex_document() {
        assert_eq!(
            latexmk_document_args("paper.tex", "build", None, false),
            ["-pdf", "-interaction=nonstopmode", "-file-line-error", "-outdir=build", "paper.tex"]
        );
        assert_eq!(
            latexmk_document_args("build/diff/diff.tex", "build/diff", Some("p-diff-v1"), true),
            ["-pdf", "-interaction=nonstopmode", "-file-line-error", "-f", "-outdir=build/diff", "-jobname=p-diff-v1", "build/diff/diff.tex"]
        );
    }

    #[test]
    fn kinds_of_documents() {
        assert_eq!(kind_of(Path::new("a/p.md")), Some(Kind::Markdown));
        assert_eq!(kind_of(Path::new("p.TEX")), Some(Kind::Latex));
        assert_eq!(kind_of(Path::new("p.txt")), None);
    }

    #[test]
    fn errors_with_file_and_line() {
        let log = "(./paper.tex\n./paper.tex:12: Undefined control sequence.\nl.12 Some \\foo\n\n./paper.tex:30: LaTeX Error: Environment bar undefined.\n\nSee the LaTeX manual.\nl.30 \\begin{bar}\n\n(./chap/one.tex\n./chap/one.tex:3: Missing $ inserted.\n\n! Emergency stop.\n<*> paper.tex\n\nOutput written on build/paper.pdf (1 page, 2:3: x).\n";
        let mut status = Status { errors: parse_log_errors(log), ..Default::default() };
        mark_here(&mut status, "paper.tex");
        let e = &status.errors;
        assert_eq!(e.len(), 4);
        assert_eq!((e[0].message.as_str(), e[0].line, e[0].file.as_deref(), e[0].here), ("Undefined control sequence.", Some(12), Some("paper.tex"), true));
        assert_eq!(e[0].context.as_deref(), Some("Some \\foo"));
        assert_eq!((e[1].line, e[1].here), (Some(30), true));
        assert_eq!(e[1].context.as_deref(), Some("\\begin{bar}"));
        // in another file: not a line of the document
        assert_eq!((e[2].file.as_deref(), e[2].line, e[2].here), (Some("chap/one.tex"), Some(3), false));
        assert_eq!((e[3].message.as_str(), e[3].line, e[3].here), ("Emergency stop.", None, false));
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
            git::parse_tags(&tags)
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

    /// A real build and marked-up PDF of a small .tex document (needs latexmk, latexdiff, pdflatex, bibtex):
    /// `cargo test -p papers-write real_latex -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_latex_build_and_marked_up() {
        let dir = std::env::temp_dir().join(format!("pw-real-tex-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let paper = dir.join("paper.tex");
        let text = "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\nIce sheets matter \\cite{lisiecki2005}.\n\\begin{equation}\\label{eq:a} a = b + c \\end{equation}\nSee \\eqref{eq:a}.\n\\bibliographystyle{plain}\n\\bibliography{references}\n\\end{document}\n";
        fs::write(&paper, text).unwrap();
        fs::write(dir.join("references.bib"), "@article{lisiecki2005, author={Lisiecki, L. and Raymo, M.}, title={A stack}, journal={Paleoceanography}, year={2005}}\n").unwrap();
        let git = |args: &[&str]| {
            let ok = Command::new("git").args(["-c", "user.name=T", "-c", "user.email=t@t"]).args(args).current_dir(&dir).status().unwrap();
            assert!(ok.success());
        };
        git(&["init", "-q"]);
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "First"]);
        git(&["tag", "v1"]);
        fs::write(&paper, text.replace("Ice sheets matter", "Ice sheets matter a lot")).unwrap();
        let built = run_build(&paper);
        println!("build: {} in {:.1} s, pdf {:?}, errors {:?}, {:?}", built.state, built.seconds.unwrap_or(0.0), built.pdf, built.errors, built.message);
        assert_eq!(built.state, "done");
        assert!(dir.join("build/paper.pdf").is_file());
        let again = run_build(&paper);
        println!("build again: {:.1} s", again.seconds.unwrap_or(0.0));
        let diffed = run_diff(&paper, &Earlier::Git { rev: "v1".into(), label: "v1".into() });
        println!("diff: {} in {:.1} s, pdf {:?}, errors {:?}, {:?}", diffed.state, diffed.seconds.unwrap_or(0.0), diffed.pdf, diffed.errors, diffed.message);
        assert_eq!(diffed.state, "done");
        assert!(dir.join("build/diff/paper-diff-v1.pdf").is_file());
        // an error at its line
        fs::write(&paper, text.replace("Ice sheets", "Ice \\oops sheets")).unwrap();
        let broken = run_build(&paper);
        println!("broken: {} {:?}", broken.state, broken.errors);
        assert_eq!(broken.state, "failed");
        assert!(broken.errors.iter().any(|e| e.here && e.line == Some(4)));
        println!("kept in {}", dir.display());
    }
}
