//! One watcher of the file system for the whole app. A watcher is an inotify instance on Linux, and a user has 128 of
//! them in all (`fs.inotify.max_user_instances`), shared with every other program: a watcher for each tab's file and
//! another for its review folder ran out of them with a few dozen tabs open, other programs then failing to watch
//! anything. Here every folder is watched once however many ask for it, and what happens in it is handed to each
//! that asked (`subscribe`), till it lets go (`unsubscribe`).
//!
//! The handlers are kept apart from the watcher's lock: the watcher's thread hands an event over while holding only
//! the handlers' lock, never the one under which folders are watched, so that watching a folder (which waits for that
//! thread) cannot wait for a handler.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

type Handler = Arc<dyn Fn(&notify::Event) + Send + Sync>;

/// The handlers, by the number of their subscription.
static HANDLERS: LazyLock<Mutex<HashMap<u64, Handler>>> = LazyLock::new(Default::default);
static NEXT: AtomicU64 = AtomicU64::new(1);

/// The watcher (made at the first subscription), and each folder watched: how many asked for it with its folders and
/// without, and the subscriptions' folders.
#[derive(Default)]
struct Watching {
    watcher: Option<RecommendedWatcher>,
    folders: HashMap<PathBuf, (usize, usize)>,
    subscriptions: HashMap<u64, (PathBuf, bool)>,
}

static WATCHING: LazyLock<Mutex<Watching>> = LazyLock::new(Default::default);

/// A subscription: what happens in its folder handed to its handler till it is dropped (`unsubscribe`).
pub struct Subscription(u64);

impl Drop for Subscription {
    fn drop(&mut self) {
        unsubscribe(self.0);
    }
}

fn mode(recursive: bool) -> RecursiveMode {
    if recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive }
}

/// What happens in `folder` (and in its folders, with `recursive`) handed to `handler`, on the watcher's thread: it
/// should be quick (an event emitted, a message sent).
pub fn subscribe(folder: &Path, recursive: bool, handler: impl Fn(&notify::Event) + Send + Sync + 'static) -> Result<Subscription, String> {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut watching = WATCHING.lock().unwrap();
    if watching.watcher.is_none() {
        let watcher = notify::recommended_watcher(|event: notify::Result<notify::Event>| {
            let Ok(event) = event else { return };
            let handlers: Vec<Handler> = HANDLERS.lock().unwrap().values().cloned().collect();
            for handler in handlers {
                handler(&event);
            }
        })
        .map_err(|e| format!("the file system is not watched ({e})"))?;
        watching.watcher = Some(watcher);
    }
    let (with, without) = watching.folders.get(folder).copied().unwrap_or((0, 0));
    // watched anew when it was not, or not with its folders and now must be
    let rewatch = (with == 0 && without == 0) || (recursive && with == 0);
    if rewatch {
        let watcher = watching.watcher.as_mut().unwrap();
        if with + without > 0 {
            let _ = watcher.unwatch(folder);
        }
        watcher.watch(folder, mode(recursive)).map_err(|e| format!("{}: not watched ({e})", folder.display()))?;
    }
    let counts = if recursive { (with + 1, without) } else { (with, without + 1) };
    watching.folders.insert(folder.to_path_buf(), counts);
    watching.subscriptions.insert(id, (folder.to_path_buf(), recursive));
    drop(watching);
    HANDLERS.lock().unwrap().insert(id, Arc::new(handler));
    Ok(Subscription(id))
}

fn unsubscribe(id: u64) {
    HANDLERS.lock().unwrap().remove(&id);
    let mut watching = WATCHING.lock().unwrap();
    let Some((folder, recursive)) = watching.subscriptions.remove(&id) else { return };
    let Some((with, without)) = watching.folders.get(&folder).copied() else { return };
    let (with, without) = if recursive { (with - 1, without) } else { (with, without - 1) };
    let Some(watcher) = watching.watcher.as_mut() else { return };
    if with + without == 0 {
        let _ = watcher.unwatch(&folder);
        watching.folders.remove(&folder);
        return;
    }
    // the last that wanted its folders gone: the folder alone
    if recursive && with == 0 {
        let _ = watcher.unwatch(&folder);
        let _ = watcher.watch(&folder, RecursiveMode::NonRecursive);
    }
    watching.folders.insert(folder, (with, without));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn a_folder_watched_once_for_all_who_ask_each_told_till_it_lets_go() {
        let dir = std::env::temp_dir().join(format!("watch-hub-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let (tx1, rx1) = channel();
        let (tx2, rx2) = channel();
        let file = dir.join("a.txt");
        let f1 = file.clone();
        let one = subscribe(&dir, false, move |e| {
            if e.paths.contains(&f1) {
                let _ = tx1.send(());
            }
        })
        .unwrap();
        let f2 = file.clone();
        let two = subscribe(&dir, false, move |e| {
            if e.paths.contains(&f2) {
                let _ = tx2.send(());
            }
        })
        .unwrap();
        assert_eq!(WATCHING.lock().unwrap().folders.get(&dir), Some(&(0, 2)));
        std::fs::write(&file, "x").unwrap();
        assert!(rx1.recv_timeout(Duration::from_secs(2)).is_ok());
        assert!(rx2.recv_timeout(Duration::from_secs(2)).is_ok());
        drop(one);
        while rx2.try_recv().is_ok() {}
        std::fs::write(&file, "y").unwrap();
        assert!(rx2.recv_timeout(Duration::from_secs(2)).is_ok());
        drop(two);
        assert_eq!(WATCHING.lock().unwrap().folders.get(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
