//! Every confirmation and message as a sheet on the window it concerns (macOS: NSAlert beginSheetModalForWindow;
//! elsewhere a dialog of its own, as the plugin makes it). tauri-plugin-dialog gives a sheet when the builder has a
//! `.parent(window)` and is shown with `show`/`show_with_result`, which hop to the main thread themselves (rfd's
//! `ModalFuture`), so these helpers can be called from any thread, the main one included, as long as the blocking
//! one is not.
//!
//! Which window: the one named, or else the window in front; with none (an error at launch) an app-modal alert.

use std::sync::mpsc;

use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Info,
    Warning,
    Error,
}

impl Level {
    fn plugin(self) -> MessageDialogKind {
        match self {
            Level::Info => MessageDialogKind::Info,
            Level::Warning => MessageDialogKind::Warning,
            Level::Error => MessageDialogKind::Error,
        }
    }
}

/// The window a sheet is on: the one it concerns, else the one in front, else none.
pub fn parent_label<'a>(own: Option<&'a str>, front: Option<&'a str>) -> Option<&'a str> {
    own.or(front)
}

/// The plugin's buttons for one to three labels (the first is the default one).
fn buttons_of(buttons: &[&str]) -> MessageDialogButtons {
    let b = |i: usize| buttons[i].to_string();
    match buttons.len() {
        0 => MessageDialogButtons::Ok,
        1 => MessageDialogButtons::OkCustom(b(0)),
        2 => MessageDialogButtons::OkCancelCustom(b(0), b(1)),
        _ => MessageDialogButtons::YesNoCancelCustom(b(0), b(1), b(2)),
    }
}

/// The label of the button pressed, if it is one of `buttons`.
fn pressed(result: MessageDialogResult, buttons: &[&str]) -> Option<String> {
    match result {
        MessageDialogResult::Custom(label) => buttons.contains(&label.as_str()).then_some(label),
        MessageDialogResult::Ok | MessageDialogResult::Yes => buttons.first().map(|b| b.to_string()),
        _ => None,
    }
}

/// `message` as a sheet (on `window`, else the window in front), `then` called with the button pressed. Never blocks.
pub fn ask_then(
    app: &AppHandle,
    window: Option<&WebviewWindow>,
    title: &str,
    message: impl Into<String>,
    kind: Level,
    buttons: &[&str],
    then: impl FnOnce(Option<String>) + Send + 'static,
) {
    let front = crate::documents::front(app);
    let own = window.map(|w| w.label());
    let label = parent_label(own, front.as_ref().map(|w| w.label()));
    let parent = label.and_then(|l| match window {
        Some(w) if w.label() == l => Some(w.clone()),
        _ => front.clone().filter(|w| w.label() == l),
    });
    let mut dialog = app.dialog().message(message).title(title).kind(kind.plugin()).buttons(buttons_of(buttons));
    if let Some(p) = &parent {
        dialog = dialog.parent(p);
    }
    let labels: Vec<String> = buttons.iter().map(|b| b.to_string()).collect();
    dialog.show_with_result(move |result| {
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        then(pressed(result, &labels));
    });
}

/// `ask_then` awaited: the button pressed. For async commands.
pub async fn ask(
    app: &AppHandle,
    window: Option<&WebviewWindow>,
    title: &str,
    message: impl Into<String>,
    kind: Level,
    buttons: &[&str],
) -> Option<String> {
    let (tx, rx) = mpsc::channel();
    ask_then(app, window, title, message, kind, buttons, move |b| {
        let _ = tx.send(b);
    });
    tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten()).await.ok().flatten()
}

/// `ask_then` waited for, on a thread of one's own (never the main thread, which the sheet needs).
pub fn ask_blocking(
    app: &AppHandle,
    window: Option<&WebviewWindow>,
    title: &str,
    message: impl Into<String>,
    kind: Level,
    buttons: &[&str],
) -> Option<String> {
    let (tx, rx) = mpsc::channel();
    ask_then(app, window, title, message, kind, buttons, move |b| {
        let _ = tx.send(b);
    });
    rx.recv().ok().flatten()
}

/// A message with an OK button only (an error, news), shown and forgotten.
pub fn tell(app: &AppHandle, window: Option<&WebviewWindow>, title: &str, message: impl Into<String>, kind: Level) {
    ask_then(app, window, title, message, kind, &["OK"], |_| {});
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sheet_is_on_the_window_it_concerns_else_the_one_in_front_else_none() {
        assert_eq!(parent_label(Some("doc-2"), Some("doc-1")), Some("doc-2"));
        assert_eq!(parent_label(None, Some("doc-1")), Some("doc-1"));
        assert_eq!(parent_label(None, None), None);
    }

    #[test]
    fn the_button_pressed_is_one_of_the_labels() {
        let b = ["Discard", "Cancel"];
        assert_eq!(pressed(MessageDialogResult::Custom("Discard".into()), &b).as_deref(), Some("Discard"));
        assert_eq!(pressed(MessageDialogResult::Custom("Cancel".into()), &b).as_deref(), Some("Cancel"));
        assert_eq!(pressed(MessageDialogResult::Custom("Other".into()), &b), None);
        assert_eq!(pressed(MessageDialogResult::Ok, &b).as_deref(), Some("Discard"));
        assert_eq!(pressed(MessageDialogResult::Cancel, &b), None);
    }

    #[test]
    fn one_to_three_labels_make_the_plugins_buttons() {
        assert!(matches!(buttons_of(&["OK"]), MessageDialogButtons::OkCustom(s) if s == "OK"));
        assert!(matches!(buttons_of(&["A", "B"]), MessageDialogButtons::OkCancelCustom(a, b) if a == "A" && b == "B"));
        assert!(matches!(buttons_of(&["A", "B", "C"]), MessageDialogButtons::YesNoCancelCustom(..)));
    }
}
