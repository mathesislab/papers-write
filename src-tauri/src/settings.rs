//! The app's settings, in its data folder (`settings.json`), set in the Settings window (`settings`, `set_settings`).
//! A change is kept at once and every window told (`settings-changed`). They are read from threads that have no app
//! handle (finding a tool, who the user is), so the current ones are kept here, in one place, for the whole process.

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, OnceLock, RwLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

const FILE: &str = "settings.json";

/// The font of the editors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorFont {
    #[default]
    System,
    Serif,
    Mono,
}

/// The colours of the windows: the system's, or light or dark whatever the system's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

/// What Tab puts in at the cursor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Indent {
    /// two spaces
    #[default]
    Two,
    /// four spaces
    Four,
    /// a tab character
    Tab,
}

/// The number of colours of the review's authors (`--review-1` … `--review-6` of the editor).
pub const REVIEW_COLOURS: u8 = 6;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// the font of the editors and its size (points)
    pub editor_font: EditorFont,
    pub editor_size: f64,
    pub indent: Indent,
    /// the text a column of a readable width, a margin on each side down to a narrow window
    pub readable_width: bool,
    /// the width of that column (em), dragged at its edges or set from View and the toolbar
    pub text_width: f64,
    /// light or dark, or as the system is
    pub appearance: Appearance,
    /// the windows' zoom, all they show (⌘= ⌘− ⌘0 on a text): 1 is as designed
    pub zoom: f64,
    pub line_numbers: bool,
    /// the spelling of the text checked as it is typed
    pub spelling: bool,
    /// the user's own sequences typed that become symbols, each with what it becomes; an empty replacement turns off
    /// a default sequence
    pub symbols: Vec<(String, String)>,

    /// who the user is in the review: empty, the git configuration's (the system's) name and e-mail
    pub review_name: String,
    pub review_email: String,
    /// the user's colour among the review's authors, 1 to 6; none: from the name, as everyone's
    pub review_colour: Option<u8>,

    /// the preview scrolled to the block the cursor is in
    pub preview_follow: bool,

    /// the tools, each the file to run; empty: found as usual (PATH and the folders the app looks in)
    pub texmark_path: String,
    pub quarto_path: String,
    pub latexmk_path: String,
    pub pandoc_path: String,

    /// the files open as tabs of one window; off: each in a window of its own
    pub tabs: bool,
    /// a thin bar in the project's colour along the top of its windows
    pub project_colour_bar: bool,
    /// look for updates at start and twice a day; they are installed only when asked
    pub check_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            editor_font: EditorFont::default(),
            editor_size: 13.5,
            indent: Indent::default(),
            readable_width: true,
            text_width: 46.0,
            appearance: Appearance::System,
            zoom: 1.0,
            line_numbers: false,
            spelling: true,
            symbols: Vec::new(),
            review_name: String::new(),
            review_email: String::new(),
            review_colour: None,
            preview_follow: true,
            texmark_path: String::new(),
            quarto_path: String::new(),
            latexmk_path: String::new(),
            pandoc_path: String::new(),
            tabs: true,
            project_colour_bar: false,
            check_updates: true,
        }
    }
}

impl Settings {
    /// What `dir` keeps; the defaults when the file is missing or broken, and keys this version does not know are
    /// left alone.
    pub fn load(dir: &Path) -> Settings {
        std::fs::read_to_string(dir.join(FILE))
            .ok()
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
            .map(Settings::normalised)
            .unwrap_or_default()
    }

    /// Written to `dir` atomically; what went wrong is told on stderr, as nothing depends on it.
    pub fn save(&self, dir: &Path) {
        let written = std::fs::create_dir_all(dir).and_then(|()| {
            let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
            crate::files::write(&dir.join(FILE), &text)
        });
        if let Err(e) = written {
            eprintln!("{}: {e}", dir.join(FILE).display());
        }
    }

    /// The settings with what cannot be used put right: texts trimmed, the size and the colour in their ranges.
    pub fn normalised(mut self) -> Settings {
        for text in [&mut self.review_name, &mut self.review_email, &mut self.texmark_path, &mut self.quarto_path, &mut self.latexmk_path, &mut self.pandoc_path] {
            *text = text.trim().to_string();
        }
        self.editor_size = if self.editor_size.is_finite() { self.editor_size.clamp(9.0, 32.0) } else { Settings::default().editor_size };
        self.review_colour = self.review_colour.filter(|c| (1..=REVIEW_COLOURS).contains(c));
        self.zoom = if self.zoom.is_finite() && self.zoom > 0.0 { self.zoom.clamp(ZOOM.0, ZOOM.1) } else { 1.0 };
        self.text_width = if self.text_width.is_finite() {
            self.text_width.clamp(TEXT_WIDTH.0, TEXT_WIDTH.1)
        } else {
            Settings::default().text_width
        };
        self
    }

    /// The file set for the tool `name` (texmark, quarto, latexmk, pandoc); none when it is to be found as usual.
    pub fn tool(&self, name: &str) -> Option<PathBuf> {
        let set = match name {
            "texmark" => &self.texmark_path,
            "quarto" => &self.quarto_path,
            "latexmk" => &self.latexmk_path,
            "pandoc" => &self.pandoc_path,
            _ => return None,
        };
        (!set.is_empty()).then(|| PathBuf::from(set))
    }
}

static CURRENT: LazyLock<RwLock<Settings>> = LazyLock::new(|| RwLock::new(Settings::default()));
static DIR: OnceLock<PathBuf> = OnceLock::new();

/// The settings read from the data folder, to be the current ones (when the app is set up).
pub fn init(app: &AppHandle) {
    if let Ok(dir) = app.path().app_data_dir() {
        *CURRENT.write().unwrap() = Settings::load(&dir);
        let _ = DIR.set(dir);
    }
    chosen_tools(&get());
    themed(app, get().appearance);
}

/// The windows' own parts (title bar, menus, dialogs) light or dark as the settings say; the pages follow the
/// settings themselves (`data-theme`).
fn themed(app: &AppHandle, appearance: Appearance) {
    app.set_theme(match appearance {
        Appearance::System => None,
        Appearance::Light => Some(tauri::Theme::Light),
        Appearance::Dark => Some(tauri::Theme::Dark),
    });
}

/// The tools the settings name, where the core finds them itself (pandoc, for the Word round trip).
fn chosen_tools(settings: &Settings) {
    let pandoc = settings.pandoc_path.trim();
    write_core::exchange::set_pandoc((!pandoc.is_empty()).then(|| PathBuf::from(pandoc)));
}

/// The current settings.
pub fn get() -> Settings {
    CURRENT.read().unwrap().clone()
}

/// `settings` the current ones: kept in the data folder, and every window told.
fn put(app: &AppHandle, settings: Settings) {
    let settings = settings.normalised();
    let who_changed = {
        let mut current = CURRENT.write().unwrap();
        let changed = (&current.review_name, &current.review_email, current.review_colour)
            != (&settings.review_name, &settings.review_email, settings.review_colour);
        *current = settings.clone();
        changed
    };
    chosen_tools(&settings);
    themed(app, settings.appearance);
    for webview in app.webviews().values() {
        let _ = webview.set_zoom(settings.zoom);
    }
    if let Some(dir) = DIR.get() {
        settings.save(dir);
    }
    // the windows' reviews know the user before the pages are told to ask who it is
    if who_changed {
        crate::review::identity_changed(app);
    }
    if let Err(e) = app.emit("settings-changed", settings) {
        eprintln!("settings-changed: {e}");
    }
}

/// The narrowest and widest column of text (em).
pub const TEXT_WIDTH: (f64, f64) = (24.0, 120.0);
/// How much View ▸ Wider Text and Narrower Text change it (em).
const TEXT_STEP: f64 = 4.0;

/// The column of text set to `width` (em), or none: as wide as the window (dragged at its edges, the toolbar's
/// control).
#[tauri::command]
pub fn set_text_width(app: AppHandle, width: Option<f64>) {
    let mut settings = get();
    match width {
        Some(w) => (settings.readable_width, settings.text_width) = (true, w),
        None => settings.readable_width = false,
    }
    put(&app, settings);
}

/// View ▸ Wider Text (`by` > 0) and Narrower Text: the column a step wider or narrower (a column again when the text
/// was as wide as the window).
pub fn step_text_width(app: &AppHandle, by: f64) {
    let mut settings = get();
    if settings.readable_width {
        settings.text_width += by * TEXT_STEP;
    }
    settings.readable_width = true;
    put(app, settings);
}

/// The window zoomed a step in or out (the toolbar's A+ and A−), all it shows.
#[tauri::command]
pub fn zoom_window(app: AppHandle, by: f64) {
    step_zoom(&app, by);
}

/// Light, dark, or as the system is (View ▸ Appearance, the toolbar).
#[tauri::command]
pub fn set_appearance(app: AppHandle, appearance: Appearance) {
    let mut settings = get();
    settings.appearance = appearance;
    put(&app, settings);
}

/// View ▸ Full Width: the text as wide as the window, or a column again.
pub fn toggle_full_width(app: &AppHandle) {
    let mut settings = get();
    settings.readable_width = !settings.readable_width;
    put(app, settings);
}

/// The settings, for the Settings window and the document pages; the page asking zoomed as they say.
#[tauri::command]
pub fn settings(webview: tauri::Webview) -> Settings {
    let settings = get();
    if settings.zoom != 1.0 {
        let _ = webview.set_zoom(settings.zoom);
    }
    settings
}

/// The least and most zoom of the windows.
const ZOOM: (f64, f64) = (0.5, 2.0);

/// ⌘= and ⌘− on a text, View ▸ Zoom In and Zoom Out, the toolbar's A+ and A− (`by` 1 or −1): the windows zoomed a
/// step in or out, all they show (text, icons, fields); ⌘0, View ▸ Actual Size (`by` 0): as designed.
pub fn step_zoom(app: &AppHandle, by: f64) {
    let mut settings = get();
    settings.zoom = if by == 0.0 { 1.0 } else { ((settings.zoom + by * 0.1) * 10.0).round() / 10.0 };
    put(app, settings);
}

/// The settings changed in the Settings window.
#[tauri::command]
pub async fn set_settings(app: AppHandle, settings: Settings) {
    put(&app, settings);
}

/// The label of the Settings window: one, not a document.
pub const WINDOW: &str = "settings";

/// The Settings window: the one open brought forward, else made; at the tab `section` (`updates`) when it is given.
pub fn show(app: &AppHandle, section: Option<&str>) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        if let Some(section) = section {
            let _ = app.emit_to(WINDOW, "section", section);
        }
        return;
    }
    let url = match section {
        Some(section) => format!("/settings#{section}"),
        None => "/settings".to_string(),
    };
    let builder = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App(url.into()))
        .title("Settings")
        .inner_size(600.0, 680.0)
        .min_inner_size(480.0, 420.0);
    // a window of its own, not a tab of a document's (macOS)
    #[cfg(target_os = "macos")]
    let builder = builder.tabbing_identifier("papers-write-settings");
    let built = builder.build();
    match built {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => eprintln!("Settings: {e}"),
    }
}

/// Where each tool is found as usual (not what the settings say), for the Settings window to show.
#[derive(Serialize)]
pub struct Found {
    texmark: Option<String>,
    quarto: Option<String>,
    latexmk: Option<String>,
    pandoc: Option<String>,
}

#[tauri::command]
pub fn tools_found() -> Found {
    let found = |name| crate::build::found_tool(name).ok().map(|p| p.to_string_lossy().into_owned());
    Found { texmark: found("texmark"), quarto: found("quarto"), latexmk: found("latexmk"), pandoc: found("pandoc") }
}

/// A file chosen for the tool `name`; none when the dialog was cancelled.
#[tauri::command]
pub async fn choose_tool(app: AppHandle, name: String) -> Option<String> {
    app.dialog()
        .file()
        .set_title(format!("Choose {name}"))
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("papers-write-settings-{name}-{}", std::process::id()))
    }

    #[test]
    fn updates_are_looked_for_unless_turned_off() {
        assert!(Settings::default().check_updates);
        assert!(serde_json::from_str::<Settings>("{}").unwrap().check_updates);
        assert!(!serde_json::from_str::<Settings>(r#"{"check_updates": false}"#).unwrap().check_updates);
    }

    #[test]
    fn the_defaults_are_the_app_as_it_was_without_settings() {
        let s = Settings::default();
        assert_eq!((s.editor_font, s.editor_size, s.indent), (EditorFont::System, 13.5, Indent::Two));
        assert!(s.readable_width && !s.line_numbers && s.spelling && s.symbols.is_empty());
        assert!((s.review_name.as_str(), s.review_email.as_str(), s.review_colour) == ("", "", None));
        assert!(s.preview_follow && s.tabs && !s.project_colour_bar);
        assert!(["texmark", "quarto", "latexmk", "pandoc"].iter().all(|t| s.tool(t).is_none()));
        // a file with nothing in it is the defaults
        assert_eq!(serde_json::from_str::<Settings>("{}").unwrap(), s);
    }

    #[test]
    fn a_missing_key_keeps_its_default() {
        let s: Settings = serde_json::from_str(r#"{"tabs": false, "indent": "tab"}"#).unwrap();
        assert!(!s.tabs && s.preview_follow);
        assert_eq!(s.indent, Indent::Tab);
    }

    #[test]
    fn keys_this_version_does_not_know_are_tolerated() {
        // menu_bar: the app stayed in the menu bar, before it stayed in the Dock
        let s: Settings = serde_json::from_str(r#"{"menu_bar": false, "tabs": false, "later": {"a": 1}}"#).unwrap();
        assert!(!s.tabs);
    }

    #[test]
    fn saved_and_loaded_back_a_missing_or_broken_file_is_the_defaults() {
        let dir = dir("round");
        assert_eq!(Settings::load(&dir), Settings::default());
        let settings = Settings {
            editor_font: EditorFont::Serif,
            editor_size: 16.0,
            indent: Indent::Four,
            readable_width: false,
            text_width: 60.0,
            appearance: Appearance::Dark,
            zoom: 1.2,
            line_numbers: true,
            spelling: false,
            symbols: vec![("(c)".into(), "©".into()), ("->".into(), String::new())],
            review_name: "A. Reviewer".into(),
            review_email: "a@example.org".into(),
            review_colour: Some(3),
            preview_follow: false,
            texmark_path: "/opt/bin/texmark".into(),
            quarto_path: "/opt/bin/quarto".into(),
            latexmk_path: "/opt/bin/latexmk".into(),
            pandoc_path: "/opt/bin/pandoc".into(),
            tabs: false,
            project_colour_bar: true,
            check_updates: false,
        };
        settings.save(&dir);
        assert_eq!(Settings::load(&dir), settings);
        std::fs::write(dir.join(FILE), "{").unwrap();
        assert_eq!(Settings::load(&dir), Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_cannot_be_used_is_put_right() {
        let s = Settings {
            review_name: "  Jane  ".into(),
            pandoc_path: " /usr/bin/pandoc ".into(),
            editor_size: 400.0,
            review_colour: Some(9),
            ..Settings::default()
        }
        .normalised();
        assert_eq!((s.review_name.as_str(), s.editor_size, s.review_colour), ("Jane", 32.0, None));
        assert_eq!(s.tool("pandoc"), Some(PathBuf::from("/usr/bin/pandoc")));
        assert_eq!(Settings { editor_size: f64::NAN, ..Settings::default() }.normalised().editor_size, 13.5);
        assert_eq!(Settings { review_colour: Some(6), ..Settings::default() }.normalised().review_colour, Some(6));
        assert_eq!(s.tool("git"), None);
    }
}
