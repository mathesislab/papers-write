// Read Aloud in the menus of the text, the preview and the PDF: the text given to the command of the settings
// (readaloud.rs). The items are there only when a command is set; Stop Reading while it reads.

import { invoke } from "@tauri-apps/api/core";
import { MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { plainOf } from "$lib/pdfSearch";
import type { Settings } from "$lib/settings";

/** Markdown or LaTeX as it is read: the marks, citations, comments, math and code left out, line by line. */
export function spokenOf(source: string): string {
  return source
    .replace(/<!--[\s\S]*?-->/g, " ")
    .split("\n")
    .map(plainOf)
    .filter(Boolean)
    .join(" ");
}

/** The items of a menu for `text` (what is read: the selection, else what is under the pointer), with the label for
 *  it; none without a command. A separator after them. A command that cannot be run says so in a dialog (Rust). */
export async function readAloudItems(text: string, label = "Read Aloud"): Promise<(MenuItem | PredefinedMenuItem)[]> {
  const settings = await invoke<Settings>("settings").catch(() => null);
  if (!settings?.read_aloud.trim()) return [];
  const reading = await invoke<boolean>("reading").catch(() => false);
  return Promise.all([
    MenuItem.new({ text: label, enabled: !!text.trim(), action: () => void invoke("read_aloud", { text }).catch(() => {}) }),
    ...(reading ? [MenuItem.new({ text: "Stop Reading", action: () => void invoke("stop_reading") })] : []),
    PredefinedMenuItem.new({ item: "Separator" }),
  ]);
}
