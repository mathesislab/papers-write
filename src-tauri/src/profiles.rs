//! The build profile of a document (`write_core::profiles`: `.papers-write/build/<name>.toml` in its project) and the
//! commands left running by the `watch` ones.
//!
//! Which profile is active is the user's, kept in the app's state (`state.json`: `profiles`), not in the project:
//! the one chosen for the document, else the one chosen last in its project, else the project's default
//! (`write.toml`: `profile`); none (or "" chosen) is no profile, and Build builds the document in front as it always
//! has. A profile chosen from a document is that document's, and the project's from then on, so the files never chosen
//! for (the chapters of a book) follow it.
//!
//! Per project, kept in the app's state: *Build With*, the profile Build (⇧⌘B) runs now, a `build` one (or an `external`
//! one, its PDF only shown); *Build After Saves*, the same build once the project's files have been quiet `QUIET` after
//! a save (autosave saves as one types; one run at a time, `build.rs`: a save during a run makes one more); *In the
//! Background*, a `watch` profile or none, chosen in the Build button's menu only, never by Build.
//!
//! A `watch` profile's command is not started at the start: it shows paused till the Build button's ▶, or a PDF shown
//! beside a text. Then it runs till paused, another (or none) is chosen, or the app quits; in a process group of its own,
//! stopped whole (`make watch` starts texmark, which starts LaTeX). What it prints goes to
//! `.papers-write/build/<name>.log`, read to tell whether it is building now. The same command running already in the
//! project's folder (started in a terminal) is not started again: it is followed, and never stopped. While a watch runs,
//! a Build With profile making the same PDF is not run (`kept_pdf`): the watch keeps it.

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
}

/// How long the project's files are to be quiet after a save before Build After Saves builds.
const QUIET: std::time::Duration = std::time::Duration::from_secs(5);
/// The saves of each project (by folder) counted: a build after saves waits `QUIET` for no other.
static SAVES: std::sync::LazyLock<Mutex<HashMap<PathBuf, u64>>> = std::sync::LazyLock::new(Default::default);

/// What the Build button says of a project's background.
#[derive(Serialize)]
pub struct Shown {
    name: String,
    /// the document it runs for, when its command names it (`{file}`)
    file: Option<String>,
    /// "busy" (started here, building now: its log written a moment ago and not yet back to watching), "watching"
    /// (started here), "elsewhere" (followed), "ended" (its command exited), "paused" (chosen,
    /// not started: the app started since, or paused by hand)
    state: &'static str,
}

/// The profiles of the page's project, the one active, and what is wrong with the files that are not profiles.
#[derive(Serialize)]
pub struct Profiles {
    profiles: Vec<Profile>,
    active: Option<String>,
    /// the profile in the background, and how it is
    background: Option<Shown>,
    /// the PDF a running watch keeps up to date (as its profile says it): a Build With profile making the same one is
    /// not run while it runs
    kept: Option<PathBuf>,
    /// Build ▸ Build After Saves, for the project
    on_save: bool,
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
        return Profiles { profiles: vec![], active: None, background: None, kept: None, on_save: false, wrong: vec![], folder: None };
    };
    let (profiles, wrong) = profiles::list(&root);
    let active = chosen(app, source, &root).filter(|n| profiles.iter().any(|p| &p.name == n && builds_now(p)));
    let per_document = |name: &str| profiles.iter().any(|p| p.name == name && p.per_document());
    let background = app.state::<Watches>().0.lock().unwrap().get_mut(&root).map(|(name, how, file)| Shown {
        file: per_document(name).then(|| file.file_name().unwrap_or_default().to_string_lossy().into_owned()),
        name: name.clone(),
        state: match how {
            Background::Started(child) => match child.try_wait() {
                Ok(None) if busy(&profiles::dir_of(&root).join(format!("{name}.log"))) => "busy",
                Ok(None) => "watching",
                _ => "ended",
            },
            Background::Elsewhere(pid) if alive(*pid) => "elsewhere",
            Background::Elsewhere(_) => "ended",
        },
    });
    let kept = kept_pdf(app, &root);
    let on_save = projects::with_saved(app, |saved| saved.build_on_save.contains(&root));
    let background = background.or_else(|| {
        let (name, file) = projects::with_saved(app, |saved| saved.background.get(&root).cloned())?;
        let shown = profiles.iter().find(|p| p.name == name)?;
        Some(Shown {
            file: shown.per_document().then(|| file.file_name().unwrap_or_default().to_string_lossy().into_owned()),
            name,
            state: "paused",
        })
    });
    Profiles { profiles, active, background, kept, on_save, wrong, folder: Some(profiles::dir_of(&root).to_string_lossy().into_owned()) }
}

/// The PDF the watch running in `root` keeps up to date (its profile's `pdf`, for the document it was chosen from),
/// relative to `root`; none when no watch runs.
pub fn kept_pdf(app: &AppHandle, root: &Path) -> Option<PathBuf> {
    let (name, source) = {
        let state = app.state::<Watches>();
        let mut watches = state.0.lock().unwrap();
        let (name, how, source) = watches.get_mut(root)?;
        let live = match how {
            Background::Started(child) => matches!(child.try_wait(), Ok(None)),
            Background::Elsewhere(pid) => alive(*pid),
        };
        live.then(|| (name.clone(), source.clone()))?
    };
    let (all, _) = profiles::list(root);
    all.into_iter().find(|p| p.name == name)?.for_document(root, &source).ok()?.pdf
}

/// Build ▸ Build After Saves for the page's project, on or off.
#[tauri::command]
pub fn set_build_on_save(window: Webview, on: bool) -> Result<Profiles, String> {
    let app = window.app_handle();
    let source = documents::path_of(app, window.label()).ok_or("Save the document first")?;
    let root = project::find(&source).ok_or("Not in a project: File ▸ Open Project… makes a folder one")?;
    projects::with_saved(app, |saved| {
        saved.build_on_save.retain(|r| r != &root);
        if on {
            saved.build_on_save.push(root.clone());
        }
    });
    menu_lists(app);
    Ok(profiles_of(app, &source))
}

/// The build profiles of the page's document.
#[tauri::command]
pub fn build_profiles(window: Webview) -> Profiles {
    match documents::path_of(window.app_handle(), window.label()) {
        Some(source) => profiles_of(window.app_handle(), &source),
        None => Profiles { profiles: vec![], active: None, background: None, kept: None, on_save: false, wrong: vec![], folder: None },
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
/// folder.
#[tauri::command]
pub fn set_background(window: Webview, name: Option<String>) -> Result<Profiles, String> {
    let app = window.app_handle();
    let source = documents::path_of(app, window.label()).ok_or("Save the document first")?;
    let root = project::find(&source).ok_or("Not in a project: File ▸ Open Project… makes a folder one")?;
    stop(app, &root);
    let name = name.filter(|n| !n.is_empty());
    projects::with_saved(app, |saved| match &name {
        Some(n) => saved.background.insert(root.clone(), (n.clone(), source.clone())),
        None => saved.background.remove(&root),
    });
    start_background(app, &root, name, &source)?;
    menu_lists(app);
    Ok(profiles_of(app, &source))
}

/// The Build button's ▶ (and the PDF shown beside a text: `play` true, `only` paused ones): what was chosen for the
/// background of the page's project started; its ❚❚ (`play` false): stopped, still chosen.
#[tauri::command]
pub fn background_play(window: Webview, play: bool) -> Result<Profiles, String> {
    let app = window.app_handle();
    let source = documents::path_of(app, window.label()).ok_or("Save the document first")?;
    let root = project::find(&source).ok_or("Not in a project")?;
    // running: started and not ended (an ended command is started again)
    let running = app.state::<Watches>().0.lock().unwrap().get_mut(&root).is_some_and(|(_, how, _)| match how {
        Background::Started(child) => matches!(child.try_wait(), Ok(None)),
        Background::Elsewhere(pid) => alive(*pid),
    });
    if play && !running {
        stop(app, &root);
        if let Some((name, from)) = projects::with_saved(app, |saved| saved.background.get(&root).cloned()) {
            start_background(app, &root, Some(name), if from.exists() { &from } else { &source })?;
        }
    } else if !play {
        stop(app, &root);
    }
    menu_lists(app);
    Ok(profiles_of(app, &source))
}

/// The profile `name` started in the background of `root`, for the document `source` (none: nothing).
fn start_background(app: &AppHandle, root: &Path, name: Option<String>, source: &Path) -> Result<(), String> {
    let root = root.to_path_buf();
    let source = source.to_path_buf();
    if let Some(name) = name {
        let (all, _) = profiles::list(&root);
        let profile = all.into_iter().find(|p| p.name == name).ok_or_else(|| format!("{name}: no such profile"))?;
        let profile = profile.for_document(&root, &source)?;
        let how = match profile.run {
            Run::Watch => match running_elsewhere(&root, &profile.command) {
                Some(pid) => Background::Elsewhere(pid),
                None => Background::Started(start(&root, &profile)?),
            },
            _ => return Err(format!("{name}: a profile Build runs, not one for the background")),
        };
        app.state::<Watches>().0.lock().unwrap().insert(root.clone(), (name, how, source.clone()));
    }
    Ok(())
}

/// A file of a project saved by the page `label`: with Build After Saves on for the project, Build (as Build With
/// says) once its files have been quiet `QUIET`.
pub fn saved(app: &AppHandle, label: &str, path: &Path) {
    let Some(root) = project::find(path) else { return };
    if !projects::with_saved(app, |saved| saved.build_on_save.contains(&root)) {
        return;
    }
    let count = {
        let mut saves = SAVES.lock().unwrap();
        let count = saves.entry(root.clone()).or_default();
        *count += 1;
        *count
    };
    let (app, label) = (app.clone(), label.to_string());
    std::thread::spawn(move || {
        std::thread::sleep(QUIET);
        if SAVES.lock().unwrap().get(&root) != Some(&count) {
            return;
        }
        if let Some(window) = app.get_webview(&label) {
            let _ = crate::build::build(app.clone(), window, false);
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
    // macOS: shown in Finder, selected; elsewhere the folder itself opened (revealing it shows its parent)
    if cfg!(target_os = "macos") {
        crate::build::reveal(dir.to_string_lossy().into_owned())
    } else {
        tauri_plugin_opener::open_path(&dir, None::<&str>).map_err(|e| format!("{}: {e}", dir.display()))
    }
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

/// Outside macOS, the Build menu and its two lists, and whether each is in it now.
pub struct MenuLists {
    build: tauri::menu::Submenu<tauri::Wry>,
    with: tauri::menu::Submenu<tauri::Wry>,
    behind: tauri::menu::Submenu<tauri::Wry>,
    on_save: tauri::menu::CheckMenuItem<tauri::Wry>,
    shown: (bool, bool),
}

/// The Build menu's lists kept, for `menu_lists` to fill them, and to put them in or take them out.
pub fn keep_menu_lists(
    app: &AppHandle,
    build: tauri::menu::Submenu<tauri::Wry>,
    with: tauri::menu::Submenu<tauri::Wry>,
    behind: tauri::menu::Submenu<tauri::Wry>,
    on_save: tauri::menu::CheckMenuItem<tauri::Wry>,
) {
    let lists = MenuLists { build, with, behind, on_save, shown: (false, false) };
    match app.try_state::<Mutex<Option<MenuLists>>>() {
        Some(state) => *state.lock().unwrap() = Some(lists),
        None => {
            app.manage(Mutex::new(Some(lists)));
        }
    }
    menu_lists(app);
}

/// Outside macOS, Build ▸ Build With and In the Background as the Build button's ▾ has them, for the tab in front;
/// a list with nothing to choose (out of a project; no `watch` or `save` profile) not in the menu at all. Made again
/// on the main thread (the menu's), not waited for.
pub fn menu_lists(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let app = &handle;
        let Some(state) = app.try_state::<Mutex<Option<MenuLists>>>() else { return };
        let mut slot = state.lock().unwrap();
        let Some(lists) = slot.as_mut() else { return };
        let shown = documents::front(app).and_then(|page| documents::path_of(app, page.label())).map(|source| profiles_of(app, &source));
        let p = shown.as_ref().filter(|p| p.folder.is_some());
        let want = (p.is_some(), p.is_some_and(|p| p.profiles.iter().any(|q| !builds_now(q))));
        let _ = lists.on_save.set_enabled(p.is_some());
        let _ = lists.on_save.set_checked(p.is_some_and(|p| p.on_save));
        for (sub, background) in [(&lists.with, false), (&lists.behind, true)] {
            for item in sub.items().unwrap_or_default() {
                let _ = sub.remove(&item);
            }
            let Some(p) = p else { continue };
            let on = if background { p.background.as_ref().map(|b| b.name.clone()) } else { p.active.clone() };
            let mut items = vec![(String::new(), if background { "Nothing" } else { "This Document" }.to_string())];
            for profile in p.profiles.iter().filter(|q| builds_now(q) != background) {
                let kept = !background && p.kept.is_some() && p.kept == profile.pdf;
                let how = match profile.run {
                    Run::Watch => " (kept running)",
                    Run::External => " (built elsewhere)",
                    Run::Build if kept => " (kept by the watch)",
                    Run::Build => "",
                };
                items.push((profile.name.clone(), format!("{}{how}", profile.name)));
            }
            let id = if background { "background" } else { "build-with" };
            for (name, text) in items {
                let checked = on.as_deref().unwrap_or("") == name;
                if let Ok(item) = tauri::menu::CheckMenuItem::with_id(app, format!("{id}:{name}"), text, true, checked, None::<&str>) {
                    let _ = sub.append(&item);
                }
            }
        }
        // in the menu after Marked-up PDF and its separator (position 5), as they have something to list
        if lists.shown.1 && !want.1 {
            let _ = lists.build.remove(&lists.behind);
        }
        if lists.shown.0 && !want.0 {
            let _ = lists.build.remove(&lists.with);
        }
        if want.0 && !lists.shown.0 {
            let _ = lists.build.insert(&lists.with, 5);
        }
        if want.1 && !lists.shown.1 {
            let _ = lists.build.insert(&lists.behind, if want.0 { 6 } else { 5 });
        }
        lists.shown = want;
    });
}

/// Whether a watch is building now, from its log: written in the last seconds, its last line not one of waiting
/// (texmark's "watching for next change", or any tool's "watching"/"waiting").
fn busy(log: &Path) -> bool {
    let recent = std::fs::metadata(log).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|d| d.as_secs_f64() < 4.0);
    if !recent {
        return false;
    }
    let text = std::fs::read(log).unwrap_or_default();
    let tail = String::from_utf8_lossy(&text[text.len().saturating_sub(400)..]).to_lowercase();
    let last = tail.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
    !(last.contains("watching") || last.contains("waiting"))
}
