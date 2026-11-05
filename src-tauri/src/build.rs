//! Builds: a saved Markdown document made into a PDF by texmark (Build).
//!
//! Where things land, beside the document `name.md`: texmark's `build/name.tex`, `build/name.pdf` and `build/name.log`
//! (its folder holds the journal's class files and the bibliography too).
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

use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, Manager, WebviewWindow};

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
    /// "build"
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

fn find_tool(name: &str) -> Result<PathBuf, String> {
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
            tell(
                &app,
                &label,
                Status {
                    what: "build",
                    state: "building",
                    ..Default::default()
                },
            );
            let status = run_build(&file);
            let show = matches!(job, Job::Build { show: true });
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
}
