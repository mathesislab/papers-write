//! The system's spell checker (NSSpellChecker) for the prose of the editor, which draws the underlines itself: WebKit's
//! own checking cannot be told to leave inline math and code alone, so it is off in the web views. The editor sends the
//! prose ranges of the visible lines and gets back where the misspelled words are (UTF-16 offsets, as the editor counts).
//!
//! Threading: NSSpellChecker is main-thread API. Each command hops to the main thread (`run_on_main_thread`), does its
//! batch there (one call per chunk the editor sends) and hands the answer back through a channel, waited for on a
//! blocking thread so neither the main thread nor the async runtime is held up. Each window has a spell document tag
//! (`uniqueSpellDocumentTag`), so that "Ignore" holds for one document and not for the others; it is closed with the window.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewWindow};

/// A text to check, with the editor's name for it.
#[derive(Deserialize)]
pub struct Text {
    pub id: u32,
    pub text: String,
}

/// A misspelled word: where it starts and ends in its text (UTF-16 code units).
#[derive(Serialize, Debug, PartialEq)]
pub struct Miss {
    pub from: usize,
    pub to: usize,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Checked {
    pub id: u32,
    pub misses: Vec<Miss>,
}

/// The spell document tag of each window.
#[derive(Default)]
pub struct Tags(Mutex<HashMap<String, isize>>);

/// The language NSSpellChecker is given for what the document asks (`en-GB`, `de`): the identifier as it spells it
/// (`en_GB`), else the language alone (`en`); none (the system's choice) when it has neither.
pub fn pick_language(asked: Option<&str>, available: &[String]) -> Option<String> {
    let asked = asked?.trim().replace('-', "_");
    if asked.is_empty() {
        return None;
    }
    let has = |l: &str| available.iter().find(|a| a.eq_ignore_ascii_case(l)).cloned();
    has(&asked).or_else(|| has(asked.split('_').next().unwrap_or("")))
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{pick_language, Checked, Miss, Text};
    use objc2_app_kit::NSSpellChecker;
    use objc2_foundation::{NSNotFound, NSRange, NSString};
    use std::ptr::null_mut;

    fn language_of(checker: &NSSpellChecker, asked: Option<&str>) -> Option<objc2::rc::Retained<NSString>> {
        let available: Vec<String> = checker.availableLanguages().iter().map(|l| l.to_string()).collect();
        pick_language(asked, &available).map(|l| NSString::from_str(&l))
    }

    pub fn tag() -> isize {
        NSSpellChecker::uniqueSpellDocumentTag()
    }

    pub fn close(tag: isize) {
        NSSpellChecker::sharedSpellChecker().closeSpellDocumentWithTag(tag);
    }

    /// The misspelled words of each text, one after the other (`checkSpellingOfString:startingAt:…` from the start,
    /// then from the end of each miss; no wrapping).
    pub fn check(texts: &[Text], language: Option<&str>, tag: isize) -> Vec<Checked> {
        let checker = NSSpellChecker::sharedSpellChecker();
        let language = language_of(&checker, language);
        texts
            .iter()
            .map(|t| {
                let string = NSString::from_str(&t.text);
                let len = string.length();
                let mut misses = Vec::new();
                let mut at = 0;
                while at < len {
                    // SAFETY: no word count is asked for (null)
                    let r: NSRange = unsafe {
                        checker.checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(
                            &string,
                            at as isize,
                            language.as_deref(),
                            false,
                            tag,
                            null_mut(),
                        )
                    };
                    if r.location == NSNotFound as usize || r.length == 0 {
                        break;
                    }
                    misses.push(Miss { from: r.location, to: r.location + r.length });
                    at = r.location + r.length;
                }
                Checked { id: t.id, misses }
            })
            .collect()
    }

    pub fn guesses(word: &str, language: Option<&str>, tag: isize) -> Vec<String> {
        let checker = NSSpellChecker::sharedSpellChecker();
        let language = language_of(&checker, language);
        let string = NSString::from_str(word);
        let range = NSRange { location: 0, length: string.length() };
        checker
            .guessesForWordRange_inString_language_inSpellDocumentWithTag(range, &string, language.as_deref(), tag)
            .map(|a| a.iter().map(|g| g.to_string()).collect())
            .unwrap_or_default()
    }

    pub fn learn(word: &str) {
        NSSpellChecker::sharedSpellChecker().learnWord(&NSString::from_str(word));
    }

    pub fn ignore(word: &str, tag: isize) {
        NSSpellChecker::sharedSpellChecker().ignoreWord_inSpellDocumentWithTag(&NSString::from_str(word), tag);
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        // NSSpellChecker is main-thread API, which a test thread is not; the checker answers from any thread in practice
        // (the commands hop to the main thread, which this does not test).
        #[test]
        fn a_misspelled_word_is_found() {
            let texts = [Text { id: 7, text: "Plain wrod here".into() }, Text { id: 8, text: "All fine".into() }];
            let got = check(&texts, Some("en-US"), tag());
            assert_eq!(got, vec![Checked { id: 7, misses: vec![Miss { from: 6, to: 10 }] }, Checked { id: 8, misses: vec![] }]);
        }

        #[test]
        fn guesses_for_a_misspelling() {
            assert!(guesses("wrod", Some("en"), tag()).iter().any(|g| g == "word"));
        }
    }
}

/// Runs `job` on the main thread, and waits for what it returns.
async fn on_main<T: Send + 'static>(app: &AppHandle, job: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(job());
    })
    .ok()?;
    tauri::async_runtime::spawn_blocking(move || rx.recv().ok()).await.ok()?
}

/// The spell document tag of the window (made the first time).
#[cfg(target_os = "macos")]
fn tag_of(app: &AppHandle, label: &str) -> isize {
    *app.state::<Tags>().0.lock().unwrap().entry(label.to_string()).or_insert_with(mac::tag)
}

/// The window is gone: its tag is closed (its ignored words with it).
pub fn closed(app: &AppHandle, label: &str) {
    let tag = app.state::<Tags>().0.lock().unwrap().remove(label);
    #[cfg(target_os = "macos")]
    if let Some(tag) = tag {
        let _ = app.run_on_main_thread(move || mac::close(tag));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = tag;
}

/// The misspelled words of each text (none where there is no system checker).
#[tauri::command]
pub async fn check_spelling(window: WebviewWindow, texts: Vec<Text>, language: Option<String>) -> Vec<Checked> {
    #[cfg(target_os = "macos")]
    {
        let tag = tag_of(window.app_handle(), window.label());
        on_main(window.app_handle(), move || mac::check(&texts, language.as_deref(), tag)).await.unwrap_or_default()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, language);
        texts.into_iter().map(|t| Checked { id: t.id, misses: Vec::new() }).collect()
    }
}

/// What the checker suggests for a misspelled word.
#[tauri::command]
pub async fn spelling_guesses(window: WebviewWindow, word: String, language: Option<String>) -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        let tag = tag_of(window.app_handle(), window.label());
        on_main(window.app_handle(), move || mac::guesses(&word, language.as_deref(), tag)).await.unwrap_or_default()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, word, language);
        Vec::new()
    }
}

/// The word added to the user's dictionary (all documents, all apps that use it).
#[tauri::command]
pub async fn learn_word(window: WebviewWindow, word: String) {
    #[cfg(target_os = "macos")]
    on_main(window.app_handle(), move || mac::learn(&word)).await;
    #[cfg(not(target_os = "macos"))]
    let _ = (window, word);
}

/// The word ignored in this window's document only.
#[tauri::command]
pub async fn ignore_word(window: WebviewWindow, word: String) {
    #[cfg(target_os = "macos")]
    {
        let tag = tag_of(window.app_handle(), window.label());
        on_main(window.app_handle(), move || mac::ignore(&word, tag)).await;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (window, word);
}

#[cfg(test)]
mod tests {
    use super::pick_language;

    fn available() -> Vec<String> {
        ["en", "en_GB", "en_US", "de"].map(String::from).to_vec()
    }

    #[test]
    fn the_language_of_a_document() {
        assert_eq!(pick_language(Some("en-GB"), &available()).as_deref(), Some("en_GB"));
        assert_eq!(pick_language(Some("de-AT"), &available()).as_deref(), Some("de"));
        assert_eq!(pick_language(Some("fr"), &available()), None);
        assert_eq!(pick_language(Some(" "), &available()), None);
        assert_eq!(pick_language(None, &available()), None);
    }
}
