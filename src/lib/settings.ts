// The app's settings (settings.rs), for the pages: read, kept as they change, and the part of them the editors take.

import { invoke } from "@tauri-apps/api/core";
import { prefsFrom, setEditorFont, type EditorFont, type EditorPrefs, type Indent } from "papers-write/editor";
import { listen } from "$lib/events";

/** mirrors settings::Settings */
export type Settings = {
  editor_font: EditorFont;
  editor_size: number;
  indent: Indent;
  readable_width: boolean;
  /** the width of the column of text (em) */
  text_width: number;
  line_numbers: boolean;
  /** the figures shown in the text, under their lines, rather than in a tooltip */
  inline_figures: boolean;
  spelling: boolean;
  /** a file written as it is typed; off: only when saved (⌘S), the text typed kept meanwhile as a draft */
  autosave: boolean;
  /** the user's own sequences typed that become symbols; an empty replacement turns off a default */
  symbols: [string, string][];
  review_name: string;
  review_email: string;
  /** put after the suggestions made in the text itself ({>>@MP<<}); empty: none */
  review_initials: string;
  /** 1 to 6, or null: from the name */
  review_colour: number | null;
  preview_follow: boolean;
  texmark_path: string;
  quarto_path: string;
  latexmk_path: string;
  pandoc_path: string;
  tabs: boolean;
  /** light or dark, or as the system is */
  appearance: Appearance;
  project_colour_bar: boolean;
  /** look for updates at start and twice a day; they are installed only when asked */
  check_updates: boolean;
};

export type Appearance = "system" | "light" | "dark";

/** The page light or dark as the settings say (`data-theme` on its root, which theme.css reads); none: as the system
 *  is. */
export function applyAppearance(appearance: Appearance | undefined) {
  const root = document.documentElement;
  if (appearance === "light" || appearance === "dark") root.dataset.theme = appearance;
  else delete root.dataset.theme;
}

/** The settings now, and again with each change (in this or another window): `apply` is called with them at once,
 *  the page's appearance set first. The function that stops it. */
export function watchSettings(apply: (settings: Settings) => void): () => void {
  const both = (s: Settings) => (applyAppearance(s.appearance), apply(s));
  invoke<Settings>("settings").then(both);
  const unlisten = listen<Settings>("settings-changed", (e) => both(e.payload));
  return () => void unlisten.then((f) => f());
}

/** The editors of the window set to the font and size of the settings; their prefs. */
export function applyEditor(settings: Settings): EditorPrefs {
  setEditorFont(settings.editor_font, settings.editor_size);
  return prefsFrom(settings);
}
