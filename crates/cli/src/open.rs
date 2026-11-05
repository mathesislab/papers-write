//! `pw <file>…`: the files opened in Papers Write (macOS: `open -a`, which reaches the running app or starts it).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Result, fail};

const BUNDLE_ID: &str = "io.github.mathesislab.papers-write";

/// The app: where Spotlight knows it (by its bundle identifier), else the Applications folders.
fn find_app() -> Option<PathBuf> {
    let found = Command::new("mdfind")
        .arg(format!("kMDItemCFBundleIdentifier == '{BUNDLE_ID}'"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .and_then(|s| s.lines().map(PathBuf::from).find(|p| p.is_dir()));
    found.or_else(|| {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        [Some(PathBuf::from("/Applications")), home.map(|h| h.join("Applications"))]
            .into_iter()
            .flatten()
            .map(|d| d.join("Papers Write.app"))
            .find(|p| p.is_dir())
    })
}

pub fn open(files: &[String]) -> Result {
    if !cfg!(target_os = "macos") {
        return fail("opening files needs the app on macOS");
    }
    let Some(app) = find_app() else {
        return fail("Papers Write is not installed (not found in /Applications or by its bundle identifier)");
    };
    // `open` resolves relative paths in its own folder, which is this one; say which file is not there
    for f in files {
        if !Path::new(f).exists() {
            return fail(format!("{f}: no such file"));
        }
    }
    let status = Command::new("open").arg("-a").arg(&app).args(files).status().map_err(|e| crate::Failure { message: format!("open did not start: {e}"), usage: false })?;
    if status.success() { Ok(()) } else { fail(format!("open failed ({status})")) }
}
