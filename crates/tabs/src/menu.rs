//! The Window menu's items for the tabs, the same in each app, and what they do: Show Previous Tab and Show Next Tab,
//! Move Tab Left and Move Tab Right, Switch to Recent Tab (⌃Tab) and Backwards, Back to Previous Tab and Forward to
//! Next Tab (the tabs visited, as a browser's Back and Forward), and Tab ▸ Tab 1 to Tab 8 and Last Tab. The app puts
//! `items` in its Window menu and gives the id of an item chosen to `handle`.

use tauri::menu::{MenuItem, MenuItemKind, Submenu};
use tauri::{AppHandle, Wry};

/// The keys of macOS, else those of Linux and Windows.
const MAC: bool = cfg!(target_os = "macos");

/// The items before Tab ▸: id, title, key. ⇧⌘[ and ⇧⌘] as macOS's apps have them, Ctrl+PageUp and Ctrl+PageDown
/// elsewhere, as browsers and editors do; the recent tab as Sublime and VS Code have it (⌃ held, ⌃Tab again: the one
/// before that).
const ITEMS: [(&str, &str, Option<&str>); 8] = [
    ("previous-tab", "Show Previous Tab", Some(if MAC { "Shift+CmdOrCtrl+[" } else { "Ctrl+PageUp" })),
    ("next-tab", "Show Next Tab", Some(if MAC { "Shift+CmdOrCtrl+]" } else { "Ctrl+PageDown" })),
    ("move-tab-left", "Move Tab Left", Some("Shift+CmdOrCtrl+PageUp")),
    ("move-tab-right", "Move Tab Right", Some("Shift+CmdOrCtrl+PageDown")),
    ("recent-tab", "Switch to Recent Tab", Some("Ctrl+Tab")),
    ("recent-tab-back", "Switch to Recent Tab Backwards", Some("Shift+Ctrl+Tab")),
    ("tab-back", "Back to Previous Tab", if MAC { None } else { Some("Alt+Left") }),
    ("tab-forward", "Forward to Next Tab", if MAC { None } else { Some("Alt+Right") }),
];

/// The items, Tab ▸ last: Tab 1 to Tab 8 and Last Tab, ⌘1 to ⌘9 (Alt+1 to Alt+9 outside macOS), as browsers have them.
pub fn items(app: &AppHandle) -> tauri::Result<Vec<MenuItemKind<Wry>>> {
    let mut items = ITEMS
        .iter()
        .map(|&(id, title, key)| MenuItem::with_id(app, id, title, true, key).map(MenuItemKind::MenuItem))
        .collect::<tauri::Result<Vec<_>>>()?;
    let tab = Submenu::new(app, "Tab", true)?;
    for n in 1..=9 {
        let title = if n == 9 { "Last Tab".to_string() } else { format!("Tab {n}") };
        let key = format!("{}+{n}", if MAC { "Cmd" } else { "Alt" });
        tab.append(&MenuItem::with_id(app, format!("tab-{n}"), title, true, Some(key))?)?;
    }
    items.push(MenuItemKind::Submenu(tab));
    Ok(items)
}

/// The item `id` done, if it is one of the tabs' (true).
pub fn handle(app: &AppHandle, id: &str) -> bool {
    let Some(action) = action(id) else {
        return false;
    };
    match action {
        Action::Step(by) => crate::step(app, by),
        Action::Shift(by) => crate::shift(app, by),
        Action::Recent(by) => crate::recent(app, by),
        Action::History(back) => crate::history(app, back),
        Action::Nth(n) => crate::nth(app, n),
    }
    true
}

/// What an item does.
#[derive(Debug, PartialEq)]
enum Action {
    Step(isize),
    Shift(isize),
    Recent(isize),
    History(bool),
    Nth(usize),
}

fn action(id: &str) -> Option<Action> {
    Some(match id {
        "previous-tab" => Action::Step(-1),
        "next-tab" => Action::Step(1),
        "move-tab-left" => Action::Shift(-1),
        "move-tab-right" => Action::Shift(1),
        "recent-tab" => Action::Recent(1),
        "recent-tab-back" => Action::Recent(-1),
        "tab-back" => Action::History(true),
        "tab-forward" => Action::History(false),
        _ => Action::Nth(id.strip_prefix("tab-")?.parse().ok().filter(|n| (1..=9).contains(n))?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_item_does_something_and_no_other_id_does() {
        for (id, _, _) in ITEMS {
            assert!(action(id).is_some(), "{id}");
        }
        assert_eq!(action("tab-1"), Some(Action::Nth(1)));
        assert_eq!(action("tab-9"), Some(Action::Nth(9)));
        for id in ["tab-0", "tab-10", "tab-", "close-tab", "bar:1"] {
            assert_eq!(action(id), None, "{id}");
        }
    }
}
