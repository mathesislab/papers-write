//! The build profile of a document (`write_core::profiles`: `.papers-write/build/<name>.toml` in its project) and the
//! commands left running by the `watch` ones.
//!
//! Which profile is active is the user's, kept in the app's state (`state.json`: `profiles`), not in the project:
//! the one chosen for the document, else the one chosen last in its project, else the project's default
//! (`write.toml`: `profile`); none (or "" chosen) is no profile, and Build builds the document in front as it always
//! has. A profile chosen from a document is that document's, and the project's from then on, so the files never chosen
//! for (the chapters of a book) follow it.
//!
//! A `watch` profile's command is started by Build and runs till the app quits or another profile is chosen in its
//! project; it runs in a process group of its own, which is stopped whole (`make watch` starts texmark, which starts
//! LaTeX). What it prints goes to `.papers-write/build/<name>.log`.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager, Webview};
use write_core::profiles::{self, Profile};
use write_core::project::{self, Project};

use crate::{documents, projects};

/// The commands of `watch` profiles running, by project folder: the profile's name, and the command.
#[derive(Default)]
pub struct Watches(Mutex<HashMap<PathBuf, (String, Child)>>);

/// The profiles of the page's project, the one active, and what is wrong with the files that are not profiles.
#[derive(Serialize)]
pub struct Profiles {
    profiles: Vec<Profile>,
    active: Option<String>,
    /// the profile whose command is running (a `watch` one)
    watching: Option<String>,
    wrong: Vec<String>,
    /// the folder of the profiles, to open
    folder: Option<String>,
}

/// The name of the profile chosen for `source`, else last in its project at `root`, else the project's default.
fn chosen(app: &AppHandle, source: &Path, root: &Path) -> Option<String> {
    let kept = projects::with_saved(app, |saved| saved.profiles.get(source).or_else(|| saved.profiles.get(root)).cloned());
    kept.or_else(|| Project::read(root).ok().and_then(|p| p.manifest.profile)).filter(|n| !n.is_empty())
}

/// The active profile of the document `source` and its project's folder; none: Build builds the document as always.
pub fn active(app: &AppHandle, source: &Path) -> Option<(Profile, PathBuf)> {
    let root = project::find(source)?;
    let name = chosen(app, source, &root)?;
    let (all, _) = profiles::list(&root);
    all.into_iter().find(|p| p.name == name).map(|p| (p, root))
}

fn profiles_of(app: &AppHandle, source: &Path) -> Profiles {
    let Some(root) = project::find(source) else {
        return Profiles { profiles: vec![], active: None, watching: None, wrong: vec![], folder: None };
    };
    let (profiles, wrong) = profiles::list(&root);
    let active = chosen(app, source, &root).filter(|n| profiles.iter().any(|p| &p.name == n));
    let watching = app.state::<Watches>().0.lock().unwrap().get(&root).map(|(name, _)| name.clone());
    Profiles { profiles, active, watching, wrong, folder: Some(profiles::dir_of(&root).to_string_lossy().into_owned()) }
}

/// The build profiles of the page's document.
#[tauri::command]
pub fn build_profiles(window: Webview) -> Profiles {
    match documents::path_of(window.app_handle(), window.label()) {
        Some(source) => profiles_of(window.app_handle(), &source),
        None => Profiles { profiles: vec![], active: None, watching: None, wrong: vec![], folder: None },
    }
}

/// The profile `name` chosen for the page's document and its project (none: no profile). A command left running by
/// another profile of the project is stopped.
#[tauri::command]
pub fn set_build_profile(window: Webview, name: Option<String>) -> Result<Profiles, String> {
    let app = window.app_handle();
    let source = documents::path_of(app, window.label()).ok_or("Save the document first")?;
    let root = project::find(&source).ok_or("Not in a project: File ▸ Open Project… makes a folder one")?;
    let name = name.unwrap_or_default();
    projects::with_saved(app, |saved| {
        saved.profiles.insert(source.clone(), name.clone());
        saved.profiles.insert(root.clone(), name.clone());
    });
    let running = app.state::<Watches>().0.lock().unwrap().get(&root).map(|(n, _)| n.clone());
    if running.is_some_and(|n| n != name) {
        stop(app, &root);
    }
    Ok(profiles_of(app, &source))
}

/// The folder of the profiles of the page's project, made if missing, shown in the file manager.
#[tauri::command]
pub fn reveal_profiles(window: Webview) -> Result<(), String> {
    let source = documents::path_of(window.app_handle(), window.label()).ok_or("Save the document first")?;
    let root = project::find(&source).ok_or("Not in a project")?;
    let dir = profiles::dir_of(&root);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    crate::build::reveal(dir.to_string_lossy().into_owned())
}

/// A shell running `command` in `root`, as a terminal would (`sh -c`; `cmd /C` on Windows), with the PATH the builds
/// have.
pub fn shell(command: &str, root: &Path) -> Command {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.args(["/C", command]);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    };
    cmd.current_dir(root).env("PATH", crate::build::search_path(None)).env("NO_COLOR", "1").stdin(Stdio::null());
    cmd
}

/// The `watch` profile's command started in `root`, unless it runs already; false when it did.
pub fn start(app: &AppHandle, root: &Path, profile: &Profile) -> Result<bool, String> {
    let state = app.state::<Watches>();
    let mut watches = state.0.lock().unwrap();
    if let Some((name, child)) = watches.get_mut(root)
        && name == &profile.name
        && child.try_wait().ok().flatten().is_none()
    {
        return Ok(false);
    }
    let log_path = profiles::dir_of(root).join(format!("{}.log", profile.name));
    let log = File::create(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
    let mut cmd = shell(&profile.command, root);
    cmd.stdout(log.try_clone().map_err(|e| e.to_string())?).stderr(log);
    // a group of its own, stopped whole
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let child = cmd.spawn().map_err(|e| format!("{}: {e}", profile.command))?;
    if let Some((_, old)) = watches.insert(root.to_path_buf(), (profile.name.clone(), child)) {
        kill(old);
    }
    Ok(true)
}

/// The command left running in `root`, stopped.
pub fn stop(app: &AppHandle, root: &Path) {
    if let Some((_, child)) = app.state::<Watches>().0.lock().unwrap().remove(root) {
        kill(child);
    }
}

/// Every command left running, stopped (the app quits).
pub fn stop_all(app: &AppHandle) {
    for (_, (_, child)) in app.state::<Watches>().0.lock().unwrap().drain() {
        kill(child);
    }
}

/// A command and all it started (its process group), stopped.
fn kill(mut child: Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill").args(["-TERM", &format!("-{}", child.id())]).status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The log of a profile's command left running.
pub fn log_of(root: &Path, profile: &Profile) -> PathBuf {
    profiles::dir_of(root).join(format!("{}.log", profile.name))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_stopped_with_what_it_started() {
        let dir = std::env::temp_dir().join(format!("profile-kill-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("still-running");
        // the shell starts a child that would write the marker after a second
        let mut cmd = shell(&format!("(sleep 1; touch {}) & wait", marker.display()), &dir);
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let child = cmd.spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        kill(child);
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(!marker.exists(), "the child of the command was not stopped");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
