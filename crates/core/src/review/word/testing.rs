//! pandoc for the tests: a .docx made of Markdown, and read three ways. The tests skip when pandoc is missing.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn pandoc() -> Option<PathBuf> {
    let path = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
    path.into_iter().chain(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from)).map(|d| d.join("pandoc")).find(|p| p.is_file())
}

fn run(args: &[&str], input: &str) -> String {
    run_in(args, input, Path::new("."))
}

fn run_in(args: &[&str], input: &str, dir: &Path) -> String {
    let mut child = Command::new(pandoc().expect("pandoc")).args(args).current_dir(dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "pandoc {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

/// A .docx of `markdown` in `dir`.
pub fn docx(markdown: &str, dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("review.docx");
    run(&["-f", "markdown", "-t", "docx", "-o", path.to_str().unwrap()], markdown);
    path
}

/// The .docx as the import takes it: the JSON of all, the text with the changes rejected, with them accepted.
pub fn read(docx: &Path) -> (String, String, String) {
    let d = docx.to_str().unwrap();
    let json = run(&[d, "--track-changes=all", "-t", "json"], "");
    let reject = run(&[d, "--track-changes=reject", "-t", "plain", "--wrap=none"], "");
    let accept = run(&[d, "--track-changes=accept", "-t", "plain", "--wrap=none"], "");
    (json, reject, accept)
}

/// The Markdown as plain text.
pub fn plain(markdown: &str) -> String {
    run(&["-f", "markdown", "-t", "plain", "--wrap=none"], markdown)
}

/// With citations resolved (the bibliography is in `dir`): a .docx of `markdown`, and the Markdown as plain text.
pub fn docx_cited(markdown: &str, dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("review.docx");
    run_in(&["-f", "markdown", "-t", "docx", "--citeproc", "-o", path.to_str().unwrap()], markdown, dir);
    path
}

pub fn plain_cited(markdown: &str, dir: &Path) -> String {
    run_in(&["-f", "markdown", "-t", "plain", "--wrap=none", "--citeproc"], markdown, dir)
}
