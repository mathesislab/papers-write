//! The review of each document window (`write_core::review`): who the user is, the suggestions and comments of the
//! document's review files carried onto the text as typed, and what the user adds to them (always an append to their
//! own file). The page sends its text with each call: positions are UTF-16 code units of it, as CodeMirror counts.
//!
//! The review folder beside the document is watched while the window is open (from the first time the page asks for
//! the items); a change in it, the app's own appends included, comes to the page as `review-changed`, a moment after
//! the last (a pull writes many files). The page reloads the items, which is cheap and the same whoever wrote.
//!
//! The mode of the window (suggesting or editing) is kept here too, for the check mark of Review ▸ Suggest Changes,
//! which follows the window in front; so is its view (All Markup, Accepted, Original), for the check marks of the
//! three items of Review ▸ view.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, Manager, WebviewWindow};
use crate::sheet::Level;
use crate::settings::Settings;
use write_core::review::text::slug;
use write_core::review::{COLOURS, Cleanup, Decision, Item, NewRecord, Person, Review, cleanup, cleanup_plan, store, utf16_of};

const UNSAVED: &str = "Save the document first";

/// The review state of each window, by its label.
#[derive(Default)]
pub struct Reviews {
    slots: Mutex<HashMap<String, Slot>>,
    /// the windows that are suggesting
    suggesting: Mutex<HashMap<String, bool>>,
    /// the view of each window, when not All Markup
    views: Mutex<HashMap<String, String>>,
    /// what the window's log is to say of the reviews sent back that were joined to its document, until its page
    /// takes it (a window made for the document is not listening yet)
    received: Mutex<HashMap<String, Vec<String>>>,
}

/// A review sent back, joined to the document of `label`: for its log of what the window did.
pub fn received(app: &AppHandle, label: &str, entry: String) {
    app.state::<Reviews>().received.lock().unwrap().entry(label.to_string()).or_default().push(entry);
    let _ = app.emit_to(EventTarget::webview(label), "review-received", ());
}

/// What the log of the window has to say of the reviews received since it last asked.
#[tauri::command]
pub fn review_received(window: WebviewWindow) -> Vec<String> {
    window.app_handle().state::<Reviews>().received.lock().unwrap().remove(window.label()).unwrap_or_default()
}

/// What a window's review was set up for: its document, who the user is for it, and the watch on its review folder.
struct Slot {
    path: PathBuf,
    me: Person,
    _watch: Option<Watch>,
}

/// Who the user is.
#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct Me {
    pub name: String,
    pub email: String,
    pub slug: String,
}

impl Me {
    fn of(p: &Person) -> Me {
        Me { name: p.name.clone(), email: p.email.clone(), slug: slug(&p.name) }
    }
}

/// The items of the review and what could not be read.
#[derive(Serialize)]
pub struct Items {
    items: Vec<Item>,
    /// the files that could not be read, each as "file: why"
    errors: Vec<String>,
    /// the people of the review files, by slug (the id of a record starts with its person's slug)
    people: HashMap<String, Who>,
}

/// A person of the review: their name and the colour of their file's header (1 to 6), if they chose one.
#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct Who {
    name: String,
    colour: Option<u8>,
}

/// Who the user is for `folder`: the name and e-mail of the settings where set, else those of the git configuration
/// of the folder, else the macOS full name of the user (or `$USER`) and no e-mail.
pub(crate) fn person_in(folder: &Path) -> Person {
    identity(&crate::settings::get(), || Person::from_git(folder), system_person)
}

/// The user as the system says (the full name, else the user name).
fn system_person() -> Person {
    let full = std::process::Command::new("id")
        .arg("-F")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok());
    Person::from_names(full, std::env::var("USER").ok())
}

/// The name and the e-mail each from the settings if set, else from `git` (asked only when needed), else `system`.
fn identity(settings: &Settings, git: impl FnOnce() -> Option<Person>, system: impl FnOnce() -> Person) -> Person {
    let (name, email) = (settings.review_name.trim(), settings.review_email.trim());
    if !name.is_empty() && !email.is_empty() {
        return Person::new(name, email).with_colour(settings.review_colour);
    }
    let base = git().unwrap_or_else(system);
    Person::new(if name.is_empty() { base.name } else { name.to_string() }, if email.is_empty() { base.email } else { email.to_string() })
        .with_colour(settings.review_colour)
}

/// Who the user is changed (the settings): each window's review of a file has the new person from now on.
pub fn identity_changed(app: &AppHandle) {
    let state = app.state::<Reviews>();
    for slot in state.slots.lock().unwrap().values_mut() {
        slot.me = person_in(folder_of(&slot.path));
        // the colour is the person's: their file's header says it to the co-authors (the header only)
        if let Err(e) = store::set_colour(&store::person_path(&slot.path, &slug(&slot.me.name)), slot.me.colour) {
            eprintln!("{}: colour not written ({e})", slot.path.display());
        }
    }
}

fn folder_of(doc: &Path) -> &Path {
    doc.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."))
}

/// The window's document, else an error that says to save it.
fn doc_of(window: &WebviewWindow) -> Result<PathBuf, String> {
    crate::documents::path_of(window.app_handle(), window.label()).ok_or_else(|| UNSAVED.to_string())
}

/// Who the user is for the window's document.
pub(crate) fn me(window: &WebviewWindow) -> Result<Person, String> {
    Ok(setup(window)?.1)
}

/// The user for the window's document, and the document: set up (with the watch) when first asked or when the
/// window shows another file.
fn setup(window: &WebviewWindow) -> Result<(PathBuf, Person), String> {
    let doc = doc_of(window)?;
    let app = window.app_handle();
    let state = app.state::<Reviews>();
    let mut slots = state.slots.lock().unwrap();
    if slots.get(window.label()).is_none_or(|s| s.path != doc) {
        let me = person_in(folder_of(&doc));
        let watch = Watch::start(app, window.label(), &doc)
            .map_err(|e| eprintln!("{}: review not watched ({e})", doc.display()))
            .ok();
        slots.insert(window.label().to_string(), Slot { path: doc.clone(), me, _watch: watch });
    }
    Ok((doc, slots[window.label()].me.clone()))
}

fn load(doc: &Path, text: &str) -> Result<Items, String> {
    let review = Review::load(doc).map_err(|e| e.to_string())?;
    let people = review
        .files
        .iter()
        .map(|f| {
            let who = match &f.person {
                Some(p) => Who { name: p.name.clone(), colour: p.colour.filter(|c| (1..=COLOURS).contains(c)) },
                None => Who { name: f.slug.clone(), colour: None },
            };
            (f.slug.clone(), who)
        })
        .collect();
    Ok(Items {
        items: review.items(text),
        errors: review.errors.iter().map(|e| format!("{}: {}", e.file.file_name().unwrap_or_default().to_string_lossy(), e.message)).collect(),
        people,
    })
}

/// A new suggestion of `me` on `text` and its id; with `id`, the edit of their suggestion `id`, which has a new id.
fn suggest(doc: &Path, me: &Person, text: &str, id: Option<&str>, from: usize, to: usize, insert: &str) -> Result<String, String> {
    let new = NewRecord::suggestion(from, to, insert);
    let r = match id {
        Some(old) => Review::supersede(doc, me, text, old, new),
        None => Review::add(doc, me, text, new),
    };
    r.map(|r| r.id).map_err(|e| e.to_string())
}

/// What a counter-suggestion says (see `counter`).
struct Counter {
    on: String,
    from: usize,
    to: usize,
    insert: String,
    note: Option<String>,
}

/// A counter-suggestion of `me` to the suggestion `on` of the review of `text`. `from` and `to` are UTF-16 offsets
/// into what `on` puts in the text (its replacement), not into the text: the counter-suggestion is made on the text
/// as `on` would leave it, in which the replacement starts where `on` is. `0..len` replaces all of it.
fn counter(doc: &Path, me: &Person, text: &str, c: &Counter) -> Result<String, String> {
    let Counter { on, from, to, insert, note } = c;
    let (from, to) = (*from, *to);
    let items = Review::load(doc).map_err(|e| e.to_string())?.items(text);
    let item = items.iter().find(|i| i.id == *on).ok_or_else(|| format!("no suggestion {on}"))?;
    let len = item.replacement.as_deref().map_or(0, |r| r.encode_utf16().count());
    if from > to || to > len {
        return Err(format!("the range {from}..{to} is not in the replacement ({len} units)"));
    }
    let mut new = NewRecord::suggestion(item.at.from + from, item.at.from + to, insert.as_str());
    if let Some(note) = note.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        new = new.note(note);
    }
    Review::counter(doc, me, text, on, new).map(|r| r.id).map_err(|e| e.to_string())
}

fn comment(doc: &Path, me: &Person, text: &str, from: usize, to: usize, body: &str) -> Result<String, String> {
    if body.trim().is_empty() {
        return Err("A comment needs some text".into());
    }
    if from > utf16_of(text, text.len()) {
        return Err("The selection is not in the text".into());
    }
    Review::add(doc, me, text, NewRecord::comment(from, to, body.trim())).map(|r| r.id).map_err(|e| e.to_string())
}

/// Who the user is: the git `user.name` and `user.email` of the document's folder, else the full name of the user in
/// the system, else `$USER`.
#[tauri::command]
pub async fn review_me(window: WebviewWindow) -> Result<Me, String> {
    Ok(Me::of(&setup(&window)?.1))
}

/// The items of the document's review carried onto `text` (the text as typed), in the order of their place in it.
#[tauri::command]
pub async fn review_items(window: WebviewWindow, text: String) -> Result<Items, String> {
    let (doc, _) = setup(&window)?;
    load(&doc, &text)
}

/// A suggestion of the user on `text`, from `from` to `to` replaced by `insert`; with `id`, their suggestion `id`
/// changed. Returns the id of the record written (an edit has a new id).
#[tauri::command]
pub async fn review_suggest(window: WebviewWindow, text: String, id: Option<String>, from: usize, to: usize, insert: String) -> Result<String, String> {
    let (doc, me) = setup(&window)?;
    suggest(&doc, &me, &text, id.as_deref(), from, to, &insert)
}

/// The user's suggestion `id` withdrawn (a suggestion typed away to nothing; any suggestion or comment of theirs).
#[tauri::command]
pub async fn review_withdraw(window: WebviewWindow, id: String) -> Result<(), String> {
    let (doc, me) = setup(&window)?;
    Review::decide(&doc, &me, &id, Decision::Withdrawn).map(|_| ()).map_err(|e| e.to_string())
}

/// A comment of the user on `from..to` of `text`. Returns its id.
#[tauri::command]
pub async fn review_comment(window: WebviewWindow, text: String, from: usize, to: usize, body: String) -> Result<String, String> {
    let (doc, me) = setup(&window)?;
    comment(&doc, &me, &text, from, to, &body)
}

/// A reply of the user to the suggestion, comment or reply `on`.
#[tauri::command]
pub async fn review_reply(window: WebviewWindow, on: String, body: String) -> Result<(), String> {
    if body.trim().is_empty() {
        return Err("A reply needs some text".into());
    }
    let (doc, me) = setup(&window)?;
    Review::reply(&doc, &me, &on, body.trim()).map(|_| ()).map_err(|e| e.to_string())
}

/// The user's decision on `on`: accepted, rejected, resolved, reopened or withdrawn. (Accepting also needs the new
/// text, which the editor makes.)
#[tauri::command]
pub async fn review_decide(window: WebviewWindow, on: String, decision: Decision) -> Result<(), String> {
    let (doc, me) = setup(&window)?;
    Review::decide(&doc, &me, &on, decision).map(|_| ()).map_err(|e| e.to_string())
}

/// A counter-suggestion of the user to the suggestion `on` (see `counter`: positions in what `on` puts there).
#[tauri::command]
pub async fn review_counter(window: WebviewWindow, text: String, on: String, from: usize, to: usize, insert: String, note: Option<String>) -> Result<String, String> {
    let (doc, me) = setup(&window)?;
    counter(&doc, &me, &text, &Counter { on, from, to, insert, note })
}

/// What Clean Up Review would remove for the user: their resolved records, the bases nothing needs; nothing is written.
#[tauri::command]
pub async fn review_cleanup_plan(window: WebviewWindow) -> Result<Cleanup, String> {
    let (doc, me) = setup(&window)?;
    cleanup_plan(&doc, &me).map_err(|e| e.to_string())
}

/// Clean Up Review: the user's own file without its resolved records, the bases nothing needs deleted.
#[tauri::command]
pub async fn review_cleanup(window: WebviewWindow) -> Result<Cleanup, String> {
    let (doc, me) = setup(&window)?;
    cleanup(&doc, &me).map_err(|e| e.to_string())
}

/// The page says whether its window is suggesting: kept for the check mark of the menu.
#[tauri::command]
pub fn review_mode(window: WebviewWindow, suggesting: bool) {
    let app = window.app_handle();
    app.state::<Reviews>().suggesting.lock().unwrap().insert(window.label().to_string(), suggesting);
    if crate::documents::front(app).is_some_and(|w| w.label() == window.label()) {
        sync_menu(app, suggesting);
    }
}

/// The page says which view its window shows (`markup`, `accepted` or `original`): kept for the check marks.
#[tauri::command]
pub fn review_view(window: WebviewWindow, view: String) {
    let app = window.app_handle();
    app.state::<Reviews>().views.lock().unwrap().insert(window.label().to_string(), view.clone());
    if crate::documents::front(app).is_some_and(|w| w.label() == window.label()) {
        sync_views(app, &view);
    }
}

/// Asks before a step that cannot be taken back (Accept All, Reject All): true if the user went on.
#[tauri::command]
pub async fn review_confirm(window: WebviewWindow, message: String, ok: String) -> bool {
    let app = window.app_handle().clone();
    crate::sheet::ask(&app, Some(&window), "Review", message, Level::Warning, &[&ok, "Cancel"]).await.as_deref() == Some(ok.as_str())
}

fn sync_menu(app: &AppHandle, suggesting: bool) {
    if let Some(item) = app.menu().and_then(|m| m.get("suggest")).and_then(|k| k.as_check_menuitem().cloned()) {
        let _ = item.set_checked(suggesting);
    }
}

/// The three views of the menu, the one shown checked.
fn sync_views(app: &AppHandle, view: &str) {
    for id in ["markup", "accepted", "original"] {
        if let Some(item) = app.menu().and_then(|m| m.get(&format!("view-{id}"))).and_then(|k| k.as_check_menuitem().cloned()) {
            let _ = item.set_checked(id == view);
        }
    }
}

/// A window got the keys: the check marks of Suggest Changes and the views show its.
pub fn focused(app: &AppHandle, label: &str) {
    let state = app.state::<Reviews>();
    let on = state.suggesting.lock().unwrap().get(label).copied().unwrap_or(false);
    let view = state.views.lock().unwrap().get(label).cloned().unwrap_or_else(|| "markup".into());
    sync_menu(app, on);
    sync_views(app, &view);
}

/// A window gone: its review (and the watch) goes.
pub fn unload(app: &AppHandle, label: &str) {
    let state = app.state::<Reviews>();
    state.slots.lock().unwrap().remove(label);
    state.suggesting.lock().unwrap().remove(label);
    state.views.lock().unwrap().remove(label);
}

/// The watch on a review folder, on a thread of its own that stops when this is dropped. The folder may not be there
/// yet (nobody has reviewed): then its nearest folder that is, one level at a time, until it is.
struct Watch(Arc<AtomicBool>);

impl Drop for Watch {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// What to watch for the review folder `dir`: itself with what is in it, else its nearest ancestor that is there.
fn target(dir: &Path) -> (PathBuf, RecursiveMode) {
    if dir.is_dir() {
        return (dir.to_path_buf(), RecursiveMode::Recursive);
    }
    let near = dir.ancestors().skip(1).find(|p| p.is_dir()).unwrap_or(Path::new("/"));
    (near.to_path_buf(), RecursiveMode::NonRecursive)
}

/// Whether an event at `path` is about the review folder `dir`: in it, or the making of a folder on the way to it.
fn about(dir: &Path, path: &Path) -> bool {
    path.starts_with(dir) || dir.starts_with(path)
}

impl Watch {
    fn start(app: &AppHandle, label: &str, doc: &Path) -> Result<Watch, String> {
        // events name files by their resolved folder
        let folder = folder_of(doc).canonicalize().map_err(|e| e.to_string())?;
        let dir = store::review_dir(&folder.join(doc.file_name().unwrap_or_default()));
        let (tx, rx) = channel::<notify::Event>();
        let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |e: notify::Result<notify::Event>| {
            if let Ok(e) = e {
                let _ = tx.send(e);
            }
        })
        .map_err(|e| e.to_string())?;
        let mut armed = target(&dir);
        watcher.watch(&armed.0, armed.1).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let (flag, handle, label) = (stop.clone(), app.clone(), label.to_string());
        std::thread::spawn(move || {
            let mut pending: Option<Instant> = None;
            while !flag.load(Ordering::SeqCst) {
                match rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(e) => {
                        let changes = matches!(e.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_));
                        if changes && e.paths.iter().any(|p| about(&dir, p)) {
                            pending = Some(Instant::now());
                            // the folder made since: watch it (or the next one down)
                            let next = target(&dir);
                            if next != armed {
                                let _ = watcher.unwatch(&armed.0);
                                if watcher.watch(&next.0, next.1).is_ok() {
                                    armed = next;
                                }
                            }
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                if pending.is_some_and(|t| t.elapsed() >= Duration::from_millis(200)) {
                    pending = None;
                    let _ = handle.emit_to(EventTarget::webview(label.clone()), "review-changed", ());
                }
            }
        });
        Ok(Watch(stop))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    const TEXT: &str = "The grounding line retreats rapidly when the bed slopes inland.\nWe show this in Fig. 3.";

    fn doc() -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("pw-app-review-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("paper.md")
    }

    fn range(text: &str, needle: &str) -> (usize, usize) {
        let b = text.find(needle).unwrap();
        (utf16_of(text, b), utf16_of(text, b + needle.len()))
    }

    fn cs(on: &str, from: usize, to: usize, insert: &str, note: Option<&str>) -> Counter {
        Counter { on: on.into(), from, to, insert: insert.into(), note: note.map(Into::into) }
    }

    fn jane() -> Person {
        Person::new("Jane Doe", "jane@example.org")
    }

    fn settings(name: &str, email: &str) -> Settings {
        Settings { review_name: name.into(), review_email: email.into(), ..Settings::default() }
    }

    fn system() -> Person {
        Person::new("Sys Tem", "")
    }

    #[test]
    fn who_the_user_is_settings_first_then_git_then_the_system() {
        let git = || Some(Person::new("Git Name", "git@example.org"));
        // nothing set: git, else the system
        assert_eq!(identity(&settings("", ""), git, system), Person::new("Git Name", "git@example.org"));
        assert_eq!(identity(&settings("", ""), || None, system), system());
        // set: they win, each on its own
        assert_eq!(identity(&settings("Set Name", "set@example.org"), git, system), Person::new("Set Name", "set@example.org"));
        assert_eq!(identity(&settings("Set Name", ""), git, system), Person::new("Set Name", "git@example.org"));
        assert_eq!(identity(&settings("", "set@example.org"), git, system), Person::new("Git Name", "set@example.org"));
        assert_eq!(identity(&settings("Set Name", ""), || None, system), Person::new("Set Name", ""));
        // blank is empty
        assert_eq!(identity(&settings("  ", " "), || None, system), system());
    }

    #[test]
    fn the_colour_of_the_settings_is_the_users_whoever_they_are() {
        let coloured = |c| Settings { review_colour: c, ..settings("", "") };
        assert_eq!(identity(&coloured(Some(3)), || None, system).colour, Some(3));
        assert_eq!(identity(&coloured(Some(9)), || None, system).colour, None);
        assert_eq!(identity(&Settings { review_colour: Some(2), ..settings("A", "a@x.org") }, || None, system).colour, Some(2));
        assert_eq!(identity(&coloured(None), || None, system).colour, None);
    }

    #[test]
    fn each_person_of_the_review_has_their_name_and_chosen_colour() {
        let d = doc();
        let (f, t) = range(TEXT, "rapidly");
        suggest(&d, &jane().with_colour(Some(5)), TEXT, None, f, t, "fast").unwrap();
        suggest(&d, &Person::new("Alex Robinson", ""), TEXT, None, f, t, "slowly").unwrap();
        let got = load(&d, TEXT).unwrap();
        assert_eq!(got.people["jane-doe"], Who { name: "Jane Doe".into(), colour: Some(5) });
        assert_eq!(got.people["alex-robinson"], Who { name: "Alex Robinson".into(), colour: None });
        // the owner changes it: the co-author sees it from the header
        store::set_colour(&store::person_path(&d, "jane-doe"), Some(2)).unwrap();
        assert_eq!(load(&d, TEXT).unwrap().people["jane-doe"].colour, Some(2));
    }

    #[test]
    fn git_and_the_system_are_not_asked_when_the_settings_say_it_all() {
        let asked = || -> Option<Person> { panic!("asked") };
        assert_eq!(identity(&settings("A", "a@x.org"), asked, || panic!("asked")), Person::new("A", "a@x.org"));
    }

    #[test]
    fn who_falls_back_to_the_full_name_then_the_user() {
        assert_eq!(Person::from_names(Some(" Alex Robinson\n".into()), Some("alrobi001".into())).name, "Alex Robinson");
        assert_eq!(Person::from_names(Some("  ".into()), Some("alrobi001".into())).name, "alrobi001");
        assert_eq!(Person::from_names(None, None).name, "Unknown");
        assert_eq!(Me::of(&Person::new("José Núñez", "j@x.org")).slug, "jose-nunez");
    }

    #[test]
    fn suggest_edit_comment_reply_decide() {
        let d = doc();
        let (f, t) = range(TEXT, "rapidly");
        let id = suggest(&d, &jane(), TEXT, None, f, t, "irreversibly").unwrap();
        assert_eq!(id, "jane-doe-1");
        // an edit has a new id, and the old one is no longer an item
        let id2 = suggest(&d, &jane(), TEXT, Some(&id), f, t, "fast").unwrap();
        assert_eq!(id2, "jane-doe-2");
        let (f, t) = range(TEXT, "Fig. 3");
        assert_eq!(comment(&d, &jane(), TEXT, f, t, " Which one? ").unwrap(), "jane-doe-3");
        assert!(comment(&d, &jane(), TEXT, f, t, "  ").is_err());
        Review::reply(&d, &jane(), "jane-doe-3", "Fig. 3a").unwrap();
        Review::decide(&d, &jane(), "jane-doe-3", Decision::Resolved).unwrap();
        let got = load(&d, TEXT).unwrap();
        assert_eq!(got.items.len(), 2);
        assert_eq!(got.items[0].id, "jane-doe-2");
        assert_eq!(got.items[0].replacement.as_deref(), Some("fast"));
        assert_eq!(got.items[1].record.text.as_deref(), Some("Which one?"));
        assert_eq!(got.items[1].thread.len(), 1);
        assert_eq!(got.people["jane-doe"].name, "Jane Doe");
        assert!(got.errors.is_empty());
        // an edit of somebody else's suggestion is refused
        assert!(suggest(&d, &Person::new("Alex Robinson", ""), TEXT, Some("jane-doe-2"), 0, 1, "x").is_err());
    }

    #[test]
    fn a_broken_file_is_listed_the_others_load() {
        let d = doc();
        let (f, t) = range(TEXT, "rapidly");
        suggest(&d, &jane(), TEXT, None, f, t, "fast").unwrap();
        std::fs::write(store::review_dir(&d).join("bad.toml"), "[[record\n").unwrap();
        let got = load(&d, TEXT).unwrap();
        assert_eq!(got.items.len(), 1);
        assert_eq!(got.errors.len(), 1);
        assert!(got.errors[0].starts_with("bad.toml: "));
    }

    #[test]
    fn counter_positions_are_in_the_replacement() {
        let d = doc();
        let text = "Café 😀 retreats rapidly now";
        let (f, t) = range(text, "rapidly");
        let first = suggest(&d, &jane(), text, None, f, t, "very fast").unwrap();
        let alex = Person::new("Alex Robinson", "");
        // "fast" of "very fast": 5..9 of the replacement
        let id = counter(&d, &alex, text, &cs(&first, 5, 9, "slowly", Some("hmm"))).unwrap();
        assert_eq!(id, "alex-robinson-1");
        let items = Review::load(&d).unwrap().items(text);
        let c = items.iter().find(|i| i.id == id).unwrap();
        assert_eq!(c.on.as_deref(), Some(first.as_str()));
        assert_eq!(c.record.exact.as_deref(), Some("fast"));
        assert_eq!(c.record.insert.as_deref(), Some("slowly"));
        assert_eq!(c.record.text.as_deref(), Some("hmm"));
        // the whole replacement, and outside it
        assert!(counter(&d, &alex, text, &cs(&first, 0, 9, "x", None)).is_ok());
        assert!(counter(&d, &alex, text, &cs(&first, 0, 10, "x", None)).is_err());
        assert!(counter(&d, &alex, text, &cs("nobody-1", 0, 1, "x", None)).is_err());
    }

    #[test]
    fn the_watch_follows_the_review_folder_down() {
        let d = doc();
        let folder = d.parent().unwrap().to_path_buf();
        let dir = store::review_dir(&d);
        assert_eq!(target(&dir), (folder.clone(), RecursiveMode::NonRecursive));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(target(&dir), (dir.clone(), RecursiveMode::Recursive));
        assert!(about(&dir, &dir.join("jane-doe.toml")));
        assert!(about(&dir, &folder.join(".papers-write")));
        assert!(!about(&dir, &folder.join("paper.md")));
    }
}
