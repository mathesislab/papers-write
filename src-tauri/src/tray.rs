//! The icon in the menu bar: the app stays there when its last window closes. New, Open… and Quit.

use tauri::AppHandle;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;

use crate::documents;

#[cfg(target_os = "macos")]
const ICON: &[u8] = include_bytes!("../icons/tray.png");
#[cfg(not(target_os = "macos"))]
const ICON: &[u8] = include_bytes!("../icons/tray-light.png");

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    // ids of their own: the handlers of the menus are global, and the menu bar has a New and an Open… too
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "tray-new", "New", true, None::<&str>)?,
            &MenuItem::with_id(app, "tray-open", "Open…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "tray-quit", "Quit Papers Write", true, None::<&str>)?,
        ],
    )?;
    TrayIconBuilder::with_id("tray")
        .icon(Image::from_bytes(ICON)?)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("Papers Write")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray-new" => {
                if let Err(e) = documents::make(app, None) {
                    eprintln!("{e}");
                }
            }
            "tray-open" => documents::choose(app),
            "tray-quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
