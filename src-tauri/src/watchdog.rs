//! The main thread watched (Linux): the windows, the menus and the pages' events all go through it, so when it is
//! held, the app is frozen. A thread asks it every second to note the time; when it has not for `HELD`, the log
//! says so, with the stack of every thread of the app (gdb, when installed), once per freeze; and again once it
//! answers. A freeze is then found in the log (`~/.cache/papers-write.log` from the desktop) where it was held.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tauri::AppHandle;

/// How long the main thread may not answer before it is said to be held.
const HELD: Duration = Duration::from_secs(5);

pub fn start(app: &AppHandle) {
    if !cfg!(target_os = "linux") {
        return;
    }
    let app = app.clone();
    let start = Instant::now();
    // when the main thread last answered (ms since `start`)
    let answered = Arc::new(AtomicU64::new(0));
    std::thread::spawn(move || {
        let ms = move || start.elapsed().as_millis() as u64;
        // the time the freeze was said (none: not frozen)
        let mut held_since: Option<u64> = None;
        // one question at a time: a held thread is not sent one more each second
        let asked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        loop {
            std::thread::sleep(Duration::from_secs(1));
            if !asked.swap(true, Ordering::SeqCst) {
                let (answered, asked) = (answered.clone(), asked.clone());
                let _ = app.run_on_main_thread(move || {
                    answered.store(start.elapsed().as_millis() as u64, Ordering::SeqCst);
                    asked.store(false, Ordering::SeqCst);
                });
            }
            let now = ms();
            let last = answered.load(Ordering::SeqCst);
            let late = now.saturating_sub(last);
            match held_since {
                None if late > HELD.as_millis() as u64 => {
                    held_since = Some(last);
                    eprintln!("[papers-write {}] the main thread has not answered for {} s: the app is frozen", stamp(), late / 1000);
                    stacks();
                }
                Some(since) if last > since => {
                    eprintln!("[papers-write {}] the main thread answers again, after {:.1} s", stamp(), (last - since) as f64 / 1000.0);
                    held_since = None;
                }
                _ => {}
            }
        }
    });
}

/// The stack of every thread of the app, into the log, by gdb (allowed to look at the app for that while: a
/// process may look only at its own children otherwise, as Ubuntu has it).
fn stacks() {
    #[cfg(target_os = "linux")]
    {
        let Ok(gdb) = which("gdb") else {
            eprintln!("[papers-write] (gdb not installed: no stacks)");
            return;
        };
        let me = std::process::id();
        unsafe { libc::prctl(libc::PR_SET_PTRACER, libc::PR_SET_PTRACER_ANY, 0, 0, 0) };
        let out = std::process::Command::new(gdb)
            .args(["-p", &me.to_string(), "-batch", "-nx", "-ex", "set pagination off", "-ex", "thread apply all bt 40"])
            .stdin(std::process::Stdio::null())
            .output();
        unsafe { libc::prctl(libc::PR_SET_PTRACER, 0, 0, 0, 0) };
        match out {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout);
                // the threads' stacks, not gdb's chatter about symbols
                let stacks: Vec<&str> = text.lines().filter(|l| l.starts_with("Thread ") || l.starts_with('#') || l.is_empty()).collect();
                eprintln!("[papers-write] stacks of the app's threads:\n{}\n[papers-write] (end of stacks)", stacks.join("\n"));
            }
            Err(e) => eprintln!("[papers-write] gdb did not run: {e}"),
        }
    }
}

#[cfg(target_os = "linux")]
fn which(name: &str) -> Result<std::path::PathBuf, ()> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file()).ok_or(())
}

/// The time of day (UTC) to the millisecond, for the log.
fn stamp() -> String {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let s = ms / 1000;
    format!("{:02}:{:02}:{:02}.{:03}", s / 3600 % 24, s / 60 % 60, s % 60, ms % 1000)
}
