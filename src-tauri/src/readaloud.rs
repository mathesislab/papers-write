//! Read Aloud: the text given to the command of the settings (`read_aloud`), such as `bard --text {text}`,
//! `spd-say {text}` or `say {text}`. The command is split as a shell would split it (quotes kept together), without a
//! shell; `{text}` in it stands for the text, as one argument, and without `{text}` the text is written to the
//! command's input. One reading at a time: a new one stops the one before, as Stop Reading does. On Unix the command
//! runs in a process group of its own, so that what it starts (a player) stops with it. The windows are told as a
//! reading starts and ends ("reading-changed"), for the toolbar's button.

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

static READING: Mutex<Option<Child>> = Mutex::new(None);

/// The command's words, `{text}` replaced by the text; whether the text was placed in them.
fn words_of(template: &str, text: &str) -> Result<(Vec<String>, bool), String> {
    let words = shlex::split(template).ok_or("The Read Aloud command has an unclosed quote (Settings)")?;
    if words.is_empty() {
        return Err("No Read Aloud command: one is set in Settings, such as spd-say {text}".into());
    }
    let placed = words.iter().any(|w| w.contains("{text}"));
    Ok((words.into_iter().map(|w| w.replace("{text}", text)).collect(), placed))
}

fn stop(child: &mut Child) {
    #[cfg(unix)]
    {
        // the group: the command and what it started
        let _ = Command::new("kill").args(["-TERM", &format!("-{}", child.id())]).status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The text read aloud by the command of the settings, the reading before stopped; what went wrong in a dialog.
#[tauri::command]
pub fn read_aloud(app: AppHandle, text: String) -> Result<(), String> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    match start(&text) {
        Ok(id) => {
            let _ = app.emit("reading-changed", true);
            crate::menu::read_aloud_item(&app, None, Some(true));
            watch(app, id);
            Ok(())
        }
        Err(e) => {
            app.dialog().message(&e).kind(MessageDialogKind::Error).title("Read Aloud").show(|_| {});
            Err(e)
        }
    }
}

/// The windows told when the reading `id` ends by itself (not when another takes its place: that one is watched).
fn watch(app: AppHandle, id: u32) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(400));
        let mut reading = READING.lock().unwrap();
        match reading.as_mut() {
            Some(child) if child.id() == id => {
                if let Ok(None) = child.try_wait() {
                    continue;
                }
                *reading = None;
                let _ = app.emit("reading-changed", false);
                crate::menu::read_aloud_item(&app, None, Some(false));
                return;
            }
            // stopped (Stop Reading tells it) or followed by another
            _ => return,
        }
    });
}

/// The reading started; its process.
fn start(text: &str) -> Result<u32, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Nothing to read".into());
    }
    let (words, placed) = words_of(&crate::settings::get().read_aloud, text)?;
    let mut reading = READING.lock().unwrap();
    if let Some(mut child) = reading.take() {
        stop(&mut child);
    }
    let mut command = Command::new(&words[0]);
    command.args(&words[1..]).stdin(if placed { Stdio::null() } else { Stdio::piped() }).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| format!("{} could not be run ({e}). The command is set in Settings ▸ Read Aloud.", words[0]))?;
    // the text as the command's input, written aside: a command reading slowly does not hold the app
    if let (false, Some(mut input)) = (placed, child.stdin.take()) {
        let text = text.to_string();
        std::thread::spawn(move || {
            let _ = input.write_all(text.as_bytes());
        });
    }
    let id = child.id();
    *reading = Some(child);
    Ok(id)
}

/// Stop Reading.
#[tauri::command]
pub fn stop_reading(app: AppHandle) {
    if let Some(mut child) = READING.lock().unwrap().take() {
        stop(&mut child);
    }
    let _ = app.emit("reading-changed", false);
    crate::menu::read_aloud_item(&app, None, Some(false));
}

/// Whether the command is reading (for the menus' Stop Reading).
#[tauri::command]
pub fn reading() -> bool {
    let mut reading = READING.lock().unwrap();
    match reading.as_mut().map(|c| c.try_wait()) {
        Some(Ok(None)) => true,
        _ => {
            *reading = None;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_is_one_argument() {
        let (words, placed) = words_of("bard --voice 'af sky' --text {text}", "Hello, \"world\" it's me").unwrap();
        assert!(placed);
        assert_eq!(words, ["bard", "--voice", "af sky", "--text", "Hello, \"world\" it's me"]);
    }

    #[test]
    fn without_a_place_the_text_goes_to_the_input() {
        let (words, placed) = words_of("espeak-ng --stdin", "Hello").unwrap();
        assert!(!placed);
        assert_eq!(words, ["espeak-ng", "--stdin"]);
    }

    #[test]
    fn an_empty_command_says_so() {
        assert!(words_of("  ", "Hello").is_err());
        assert!(words_of("say 'unclosed", "Hello").is_err());
    }
}
