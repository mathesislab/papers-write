//! Commands run in a process group of their own, so that what they start stops with them: a `watch` profile's command
//! (`make watch` starts texmark, which starts LaTeX: profiles.rs) and the Read Aloud command (a player: readaloud.rs).
//! On Unix the group is also tied to the app: it is stopped when the app ends, however it ends (quit, killed, force
//! quit, crashed), where the app's own stop (`stop`, at quit) is not run.
//!
//! The tie: a pipe the app holds the writing end of, all its life, and writes nothing to. Each command gets a guard,
//! forked in its group just before the command starts: the guard waits on the pipe's reading end, and when the app
//! ends the system closes the writing end and the guard stops the group. A guard whose command ended exits (it looks
//! once a second). The command itself does not get the pipe (closed on exec).

use std::process::{Child, Command};

/// `command` to run as the leader of a process group of its own, stopped when the app ends (Unix; elsewhere as it is).
pub fn own(command: &mut Command) -> &mut Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        if let Some(&(read, _)) = tie() {
            // SAFETY: between fork and exec only async-signal-safe calls (setpgid, fork, dup2, close, poll, getppid, kill,
            // _exit)
            unsafe {
                command.pre_exec(move || {
                    guard(read);
                    Ok(())
                })
            };
        }
    }
    command
}

/// The app's pipe (reading end, writing end), both closed on exec; none if it could not be made.
#[cfg(unix)]
fn tie() -> Option<&'static (i32, i32)> {
    static TIE: std::sync::OnceLock<Option<(i32, i32)>> = std::sync::OnceLock::new();
    TIE.get_or_init(pipe).as_ref()
}

/// A pipe whose ends close on exec (only the guard, which never execs, keeps the reading end). `pipe` then `fcntl`:
/// macOS has no `pipe2`; this is done once, at the first command, before any of them is forked.
#[cfg(unix)]
fn pipe() -> Option<(i32, i32)> {
    let mut fds = [0; 2];
    // SAFETY: a pipe into a local array, then flags set on the two descriptors it made
    unsafe {
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            return None;
        }
        for fd in fds {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
        }
    }
    Some((fds[0], fds[1]))
}

/// In the command's process, before its exec: the guard forked (the command goes on to its exec). The guard waits
/// until the pipe closes (the app ended), then stops the group, or until its parent is no longer the command (it
/// ended), then exits.
#[cfg(unix)]
fn guard(read: i32) {
    // SAFETY: async-signal-safe calls only, in the forked guard; it never returns into Rust code of the app
    unsafe {
        // the group made here, before the fork (whatever the order of std's own setpgid): the guard's `kill(0)` must
        // never reach the app's group
        libc::setpgid(0, 0);
        // the command's pid, taken before the fork: the guard may only run once the command has ended
        let command = libc::getpid();
        if libc::fork() != 0 {
            return;
        }
        // nothing of the app kept but the reading end (as 0): no writing end of the pipe (or it would never close),
        // nor any other file or pipe of the app (a reading's input never ending, a log held)
        libc::dup2(read, 0);
        #[cfg(target_os = "linux")]
        libc::syscall(libc::SYS_close_range, 1u32, u32::MAX, 0u32);
        #[cfg(not(target_os = "linux"))]
        for fd in 1..libc::getdtablesize() {
            libc::close(fd);
        }
        let read = 0;
        loop {
            let mut wait = libc::pollfd { fd: read, events: libc::POLLIN, revents: 0 };
            // nothing is ever written: ready means closed (the app ended)
            if libc::poll(&mut wait, 1, 1000) > 0 {
                libc::kill(0, libc::SIGTERM);
                libc::_exit(0);
            }
            if libc::getppid() != command {
                libc::_exit(0);
            }
        }
    }
}

/// A command run by `own`, stopped with all it started: its group sent SIGTERM, then the command killed and waited for.
pub fn stop(child: &mut Child) {
    // `--` before the group: without it procps's kill (Ubuntu's) reads `-<pid>` as options, and a first digit as the
    // pid -1: every process of the user signalled
    #[cfg(unix)]
    let _ = Command::new("kill").args(["-TERM", "--", &format!("-{}", child.id())]).status();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_stopped_with_what_it_started_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("group-stop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("still-running");
        // a process outside the group, which must live on
        let mut other = Command::new("sleep").arg("5").spawn().unwrap();
        // the shell starts a child that would write the marker after a second
        let mut command = Command::new("sh");
        command.args(["-c", &format!("(sleep 1; touch {}) & wait", marker.display())]);
        let mut child = own(&mut command).spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        stop(&mut child);
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(!marker.exists(), "the child of the command was not stopped");
        assert!(other.try_wait().unwrap().is_none(), "a process outside the group was stopped");
        let _ = other.kill();
        let _ = other.wait();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_command_is_stopped_when_the_app_ends_and_its_guard_goes_when_it_ends() {
        // the app ending: the writing end of a pipe of the test's own closed
        let (read, write) = pipe().unwrap();
        let tied = |line: &str| {
            let mut command = Command::new("sh");
            command.args(["-c", line]);
            use std::os::unix::process::CommandExt;
            command.process_group(0);
            unsafe {
                command.pre_exec(move || {
                    guard(read);
                    Ok(())
                })
            };
            command.spawn().unwrap()
        };
        let group_alive = |pid: u32| Command::new("kill").args(["-0", "--", &format!("-{pid}")]).status().unwrap().success();
        // a command that ends by itself: its guard gone soon after (the group empty)
        let mut short = tied("exit 0");
        short.wait().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(!group_alive(short.id()), "the guard of an ended command is still there");
        // a command that runs on, with what it started: all stopped once the app is gone
        let mut long = tied("sleep 30 & wait");
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(group_alive(long.id()));
        unsafe { libc::close(write) };
        let _ = long.wait();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(!group_alive(long.id()), "the command's group lives on after the app");
        unsafe { libc::close(read) };
    }
}
