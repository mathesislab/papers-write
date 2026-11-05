//! The document windows as macOS tabs, whatever the system's setting: macOS merges the windows of one tabbing
//! identifier into tabs only when the user prefers tabs (by default, in full screen only), so a window made joins
//! the tabs of the window in front itself.

use tauri::WebviewWindow;

/// `window` added as a tab of the window `to` (macOS; nothing elsewhere).
pub fn join(window: &WebviewWindow, to: &WebviewWindow) {
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
