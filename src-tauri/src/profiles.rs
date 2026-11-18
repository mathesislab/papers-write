//! The build profile of a document (`write_core::profiles`: `.papers-write/build/<name>.toml` in its project) and the
//! commands left running by the `watch` ones.
//!
//! Which profile is active is the user's, kept in the app's state (`state.json`: `profiles`), not in the project:
//! the one chosen for the document, else the one chosen last in its project, else the project's default
//! (`write.toml`: `profile`); none (or "" chosen) is no profile, and Build builds the document in front as it always
//! has. A profile chosen from a document is that document's, and the project's from then on, so the files never chosen
//! for (the chapters of a book) follow it.
//!
//! Two choices per project. *Build with*: the profile Build (⇧⌘B) runs now, a `build` one (or an `external` one, its PDF
//! only shown). *In the background*: a `watch` or `save` profile, or none (the default: nothing runs at launch), chosen
//! in the Build button's menu only, never by Build, and kept for the session, not saved.
//!
//! A `watch` profile's command is started when chosen and runs till another (or none) is chosen or the app quits; it
//! runs in a process group of its own, which is stopped whole (`make watch` starts texmark, which starts LaTeX). What it
//! prints goes to `.papers-write/build/<name>.log`. The same command running already in the project's folder (started
//! in a terminal) is not started again: it is followed, and never stopped. A `save` profile's command is run when chosen,
//! then after the project's files are saved, once they have been quiet `QUIET` (autosave saves as one types), one run at
//! a time (`build.rs`: a save during a run makes one more).

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager, Webview};
use write_core::profiles::{self, Profile, Run};
use write_core::project::{self, Project};

use crate::{documents, projects};

/// What runs in the background of each project (by its folder): the profile's name, how, and the document it was
/// chosen from (its `{file}`).
#[derive(Default)]
pub struct Watches(Mutex<HashMap<PathBuf, (String, Background, PathBuf)>>);

pub enum Background {
    /// the `watch` command, started here
    Started(Child),
    /// the same command, started elsewhere (a terminal): its process, followed
    Elsewhere(u32),
    /// a `save` profile: the saves counted, the last one's run waits `QUIET` for no other
    OnSave(u64),
}

/// How long the project's files are to be quiet after a save before a `save` profile runs.
const QUIET: std::time::Duration = std::time::Duration::from_secs(5);

/// What the Build button says of a project's background.
#[derive(Serialize)]
pub struct Shown {
    name: String,
    /// the document it runs for, when its command names it (`{file}`)
    file: Option<String>,
    /// "watching" (started here), "elsewhere" (followed), "ended" (its command exited), "on-save"
    state: &'static str,
}

/// The profiles of the page's project, the one active, and what is wrong with the files that are not profiles.
#[derive(Serialize)]
pub struct Profiles {
    profiles: Vec<Profile>,
    active: Option<String>,
    /// the profile in the background, and how it is
    background: Option<Shown>,
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
    all.into_iter().find(|p| p.name == name && builds_now(p)).map(|p| (p, root))
}

/// A profile Build runs (or whose PDF it shows): not a background one.
fn builds_now(profile: &Profile) -> bool {
    matches!(profile.run, Run::Build | Run::External)
}

fn profiles_of(app: &AppHandle, source: &Path) -> Profiles {
    let Some(root) = project::find(source) else {
        return Profiles { profiles: vec![], active: None, background: None, wrong: vec![], folder: None };
    };
    let (profiles, wrong) = profiles::list(&root);
    let active = chosen(app, source, &root).filter(|n| profiles.iter().any(|p| &p.name == n && builds_now(p)));
    let per_document = |name: &str| profiles.iter().any(|p| p.name == name && p.per_document());
    let background = app.state::<Watches>().0.lock().unwrap().get_mut(&root).map(|(name, how, file)| Shown {
        file: per_document(name).then(|| file.file_name().unwrap_or_default().to_string_lossy().into_owned()),
        name: name.clone(),
        state: match how {
            Background::Started(child) => match child.try_wait() {
                Ok(None) => "watching",
                _ => "ended",
            },
            Background::Elsewhere(pid) if alive(*pid) => "elsewhere",
            Background::Elsewhere(_) => "ended",
            Background::OnSave(_) => "on-save",
        },
    });
    Profiles { profiles, active, background, wrong, folder: Some(profiles::dir_of(&root).to_string_lossy().into_owned()) }
}

/// The build profiles of the page's document.
#[tauri::command]
pub fn build_profiles(window: Webview) -> Profiles {
    match documents::path_of(window.app_handle(), window.label()) {
        Some(source) => profiles_of(window.app_handle(), &source),
        None => Profiles { profiles: vec![], active: None, background: None, wrong: vec![], folder: None },
    }
}

/// The profile `name` Build runs, chosen for the page's document and its project (none: the document, as always).
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
    menu_lists(app);
    Ok(profiles_of(app, &source))
}

/// The profile `name` chosen to run in the background of the page's project (none: nothing). What ran there is
/// stopped (unless started elsewhere); a `watch` profile's command is started, unless it runs already in the project's
/// folder; a `save` profile's is run now, then after the saves.
#[tauri::command]
pub fn set_background(window: Webview, name: Option<String>) -> Result<Profiles, String> {
    let app = window.app_handle();
    let source = documents::path_of(app, window.label()).ok_or("Save the document first")?;
    let root = project::find(&source).ok_or("Not in a project: File ▸ Open Project… makes a folder one")?;
    stop(app, &root);
    if let Some(name) = name.filter(|n| !n.is_empty()) {
        let (all, _) = profiles::list(&root);
        let profile = all.into_iter().find(|p| p.name == name).ok_or_else(|| format!("{name}: no such profile"))?;
        let profile = profile.for_document(&root, &source)?;
        let how = match profile.run {
            Run::Watch => match running_elsewhere(&root, &profile.command) {
                Some(pid) => Background::Elsewhere(pid),
                None => Background::Started(start(&root, &profile)?),
            },
            Run::Save => {
                crate::build::run_profile_for(app, window.label(), profile.clone(), root.clone());
                Background::OnSave(0)
            }
            _ => return Err(format!("{name}: a profile Build runs, not one for the background")),
        };
        app.state::<Watches>().0.lock().unwrap().insert(root.clone(), (name, how, source.clone()));
    }
    menu_lists(app);
    Ok(profiles_of(app, &source))
}

/// A file of a project saved by the page `label`: the project's `save` profile run once its files have been quiet.
pub fn saved(app: &AppHandle, label: &str, path: &Path) {
    let Some(root) = project::find(path) else { return };
    let count = {
        let state = app.state::<Watches>();
        let mut watches = state.0.lock().unwrap();
        let Some((_, Background::OnSave(count), _)) = watches.get_mut(&root) else { return };
        *count += 1;
        *count
    };
    let (app, label) = (app.clone(), label.to_string());
    std::thread::spawn(move || {
        std::thread::sleep(QUIET);
        let (name, source) = match app.state::<Watches>().0.lock().unwrap().get(&root) {
            Some((name, Background::OnSave(now), source)) if *now == count => (name.clone(), source.clone()),
            _ => return,
        };
        let (all, _) = profiles::list(&root);
        if let Some(Ok(profile)) = all.into_iter().find(|p| p.name == name).map(|p| p.for_document(&root, &source)) {
            crate::build::run_profile_for(&app, &label, profile, root);
        }
    });
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

/// The `watch` profile's command started in `root`.
fn start(root: &Path, profile: &Profile) -> Result<Child, String> {
    let log_path = profiles::dir_of(root).join(format!("{}.log", profile.name));
    let log = File::create(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
    let mut cmd = shell(&profile.command, root);
    cmd.stdout(log.try_clone().map_err(|e| e.to_string())?).stderr(log);
    // a group of its own, stopped whole
    crate::group::own(&mut cmd).spawn().map_err(|e| format!("{}: {e}", profile.command))
}

/// What runs in the background of `root`, stopped (a command started elsewhere is left to run).
pub fn stop(app: &AppHandle, root: &Path) {
    if let Some((_, Background::Started(mut child), _)) = app.state::<Watches>().0.lock().unwrap().remove(root) {
        crate::group::stop(&mut child);
    }
}

/// Every command started here, stopped (the app quits).
pub fn stop_all(app: &AppHandle) {
    for (_, (_, how, _)) in app.state::<Watches>().0.lock().unwrap().drain() {
        if let Background::Started(mut child) = how {
            crate::group::stop(&mut child);
        }
    }
}

/// A process running `command` in the folder `root` (started in a terminal): `ps`, and the process's folder
/// (`/proc` on Linux, `lsof` on macOS). None on Windows.
fn running_elsewhere(root: &Path, command: &str) -> Option<u32> {
    if cfg!(windows) {
        return None;
    }
    let words = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let wanted = words(command);
    let root = std::fs::canonicalize(root).ok()?;
    let out = Command::new("ps").args(["-Ao", "pid=,args="]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().find_map(|line| {
        let (pid, args) = line.trim().split_once(' ')?;
        let pid: u32 = pid.parse().ok()?;
        let args = words(args);
        let same = args == wanted || args.ends_with(&format!(" -c {wanted}"));
        (same && pid != std::process::id() && folder_of(pid).is_some_and(|f| f == root)).then_some(pid)
    })
}

/// The folder a process runs in.
fn folder_of(pid: u32) -> Option<PathBuf> {
    if cfg!(target_os = "linux") {
        return std::fs::read_link(format!("/proc/{pid}/cwd")).ok();
    }
    let out = Command::new("lsof").args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| l.strip_prefix('n').map(PathBuf::from))
}

/// Whether the process runs still.
fn alive(pid: u32) -> bool {
    Command::new("ps").args(["-p", &pid.to_string()]).stdout(Stdio::null()).status().is_ok_and(|s| s.success())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_command_running_in_the_projects_folder_is_found_and_not_one_elsewhere() {
        let root = std::env::temp_dir().join(format!("watch-{}", std::process::id()));
        let other = root.join("other");
        std::fs::create_dir_all(&other).unwrap();
        let mut child = Command::new("sleep").arg("37.5").current_dir(&root).spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(running_elsewhere(&root, "sleep   37.5"), Some(child.id()));
        assert_eq!(running_elsewhere(&other, "sleep 37.5"), None);
        assert_eq!(running_elsewhere(&root, "sleep 38"), None);
        assert!(alive(child.id()));
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(!alive(child.id()));
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// Outside macOS, Build ▸ Build With and In the Background as the Build button's ▾ has them, for the tab in front:
/// made again on the main thread (the menu's), not waited for.
pub fn menu_lists(app: &AppHandle) {
    if cfg!(target_os = "macos") {
        return;
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let app = &handle;
        let shown = documents::front(app).and_then(|page| documents::path_of(app, page.label())).map(|source| profiles_of(app, &source));
        let menu = app.menu();
        for (id, background) in [("build-with", false), ("background", true)] {
            let Some(sub) = menu.as_ref().and_then(|m| m.get(id)).and_then(|k| k.as_submenu().cloned()) else { continue };
            for item in sub.items().unwrap_or_default() {
                let _ = sub.remove(&item);
            }
            let Some(p) = shown.as_ref().filter(|p| p.folder.is_some()) else {
                let _ = sub.append(&tauri::menu::MenuItem::new(app, "Not in a project", false, None::<&str>).unwrap());
                continue;
            };
            let on = if background { p.background.as_ref().map(|b| b.name.clone()) } else { p.active.clone() };
            let first = if background { "Nothing" } else { "This Document" };
            let mut items = vec![(String::new(), first.to_string())];
            for profile in p.profiles.iter().filter(|q| builds_now(q) != background) {
                let how = match profile.run {
                    Run::Watch => " (kept running)",
                    Run::Save => " (after saves)",
                    Run::External => " (built elsewhere)",
                    Run::Build => "",
                };
                items.push((profile.name.clone(), format!("{}{how}", profile.name)));
            }
            for (name, text) in items {
                let checked = on.as_deref().unwrap_or("") == name;
                if let Ok(item) = tauri::menu::CheckMenuItem::with_id(app, format!("{id}:{name}"), text, true, checked, None::<&str>) {
                    let _ = sub.append(&item);
                }
            }
            if background && p.profiles.iter().all(builds_now) {
                let _ = sub.append(&tauri::menu::MenuItem::new(app, "No watch or save profile (Build Profiles…)", false, None::<&str>).unwrap());
            }
        }
    });
}
