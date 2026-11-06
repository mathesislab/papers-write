//! The document windows as macOS tabs, whatever the system's setting: macOS merges the windows of one tabbing
//! identifier into tabs only when the user prefers tabs (by default, in full screen only), so a window made joins
//! the tabs of the window in front itself. Which window is in front is AppKit's to say (its key window).

use tauri::Window;

/// `window` added as a tab of the window `to` (macOS; nothing elsewhere).
pub fn join(window: &Window, to: &Window) {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSWindow, NSWindowOrderingMode};
        let (Ok(new), Ok(old)) = (window.ns_window(), to.ns_window()) else {
            return;
        };
        let (new, old) = (new as usize, old as usize);
        let _ = window.run_on_main_thread(move || {
            // SAFETY: the NSWindows of two Tauri windows alive as this runs, used on the main thread
            let (new, old) = unsafe { (&*(new as *const NSWindow), &*(old as *const NSWindow)) };
            old.addTabbedWindow_ordered(new, NSWindowOrderingMode::Above);
            new.makeKeyAndOrderFront(None);
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (window, to);
}

/// The label of the key window (the one in front, the tab chosen), as AppKit says; none off the main thread (the
/// menu's events come on it) or when no window of the app is key.
pub fn key_window(app: &tauri::AppHandle) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        use tauri::Manager;
        let mtm = MainThreadMarker::new()?;
        let key = NSApplication::sharedApplication(mtm).keyWindow()?;
        let key = objc2::rc::Retained::as_ptr(&key) as usize;
        app.webview_windows()
            .into_iter()
            .find(|(_, w)| w.ns_window().is_ok_and(|p| p as usize == key))
            .map(|(label, _)| label)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        None
    }
}
