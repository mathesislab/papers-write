//! Read Aloud: the text given to the command of the settings (`read_aloud`), such as `bard --text {text}`,
//! `spd-say {text}` or `say {text}`. The command is split as a shell would split it (quotes kept together), without a
//! shell; `{text}` in it stands for the text, as one argument, and without `{text}` the text is written to the
//! command's input. One reading at a time: a new one stops the one before, as Stop Reading does. On Unix the command
//! runs in a process group of its own, so that what it starts (a player) stops with it.

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

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
pub fn read_aloud(app: tauri::AppHandle, text: String) -> Result<(), String> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    start(&text).inspect_err(|e| app.dialog().message(e).kind(MessageDialogKind::Error).title("Read Aloud").show(|_| {}))
}

fn start(text: &str) -> Result<(), String> {
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
    let mut child = command.spawn().map_err(|e| format!("{} could not be run ({e}). The command is set in Settings ▸ Build ▸ Read Aloud.", words[0]))?;
    // the text as the command's input, written aside: a command reading slowly does not hold the app
    if let (false, Some(mut input)) = (placed, child.stdin.take()) {
        let text = text.to_string();
        std::thread::spawn(move || {
            let _ = input.write_all(text.as_bytes());
        });
    }
    *reading = Some(child);
    Ok(())
}

/// Stop Reading.
#[tauri::command]
pub fn stop_reading() {
    if let Some(mut child) = READING.lock().unwrap().take() {
        stop(&mut child);
    }
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
