//! Commands run in a process group of their own, so that what they start stops with them: a `watch` profile's command
//! (`make watch` starts texmark, which starts LaTeX: profiles.rs) and the Read Aloud command (a player: readaloud.rs).

use std::process::{Child, Command};

/// `command` to run as the leader of a process group of its own (Unix; elsewhere as it is).
pub fn own(command: &mut Command) -> &mut Command {
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(command, 0);
    command
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
}
