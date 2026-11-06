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
  line_numbers: boolean;
  spelling: boolean;
  /** the user's own sequences typed that become symbols; an empty replacement turns off a default */
  symbols: [string, string][];
  review_name: string;
  review_email: string;
  /** 1 to 6, or null: from the name */
  review_colour: number | null;
  preview_follow: boolean;
  texmark_path: string;
  latexmk_path: string;
  pandoc_path: string;
  tabs: boolean;
  project_colour_bar: boolean;
};

/** The settings now, and again with each change (in this or another window): `apply` is called with them at once. The
 *  function that stops it. */
export function watchSettings(apply: (settings: Settings) => void): () => void {
  invoke<Settings>("settings").then(apply);
  const unlisten = listen<Settings>("settings-changed", (e) => apply(e.payload));
  return () => void unlisten.then((f) => f());
}

/** The editors of the window set to the font and size of the settings; their prefs. */
export function applyEditor(settings: Settings): EditorPrefs {
  setEditorFont(settings.editor_font, settings.editor_size);
  return prefsFrom(settings);
}
