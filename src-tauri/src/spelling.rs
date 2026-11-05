//! The system's spelling as text is typed, in the web views: WebKit checks it only when the app's
//! `WebContinuousSpellCheckingEnabled` default is on (Safari turns it on; an app of its own must), read as WebKit
//! starts, so it is set before the first window. Where it applies is the editor's: `spellcheck` on the Markdown
//! editors, from Settings ▸ Editor ▸ Check spelling; off in LaTeX and plain text.

/// The default on, for the web views made from now on.
pub fn enable() {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSString, NSUserDefaults};
        let defaults = NSUserDefaults::standardUserDefaults();
        defaults.setBool_forKey(true, &NSString::from_str("WebContinuousSpellCheckingEnabled"));
    }
}
