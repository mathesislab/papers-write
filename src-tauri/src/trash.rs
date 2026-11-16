//! A file or folder moved to the Trash, as the Finder does: taken back from there, not deleted (as papers-review). On
//! Linux the desktop's Trash (`gio trash`, the freedesktop.org one).

use std::path::Path;

#[cfg(target_os = "macos")]
pub fn trash(path: &Path) -> Result<(), String> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| format!("{}: {}", path.display(), e.localizedDescription()))
}

#[cfg(not(target_os = "macos"))]
pub fn trash(path: &Path) -> Result<(), String> {
    let out = std::process::Command::new("gio")
        .arg("trash")
        .arg(path)
        .output()
        .map_err(|e| format!("{}: no Trash (gio: {e})", path.display()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("{}: {}", path.display(), String::from_utf8_lossy(&out.stderr).trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Puts a folder in the Trash of whoever runs it: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn a_folder_goes_to_the_trash() {
        let dir = std::env::temp_dir().join("papers-write-trash-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "taken back from the Trash").unwrap();
        trash(&dir).unwrap();
        assert!(!dir.exists());
        assert!(trash(&dir).is_err());
    }
}
