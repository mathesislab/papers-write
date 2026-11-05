//! A file or folder moved to the Trash, as the Finder does: taken back from there, not deleted (as papers-review).

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
    Err(format!("{}: the Trash on macOS only", path.display()))
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
