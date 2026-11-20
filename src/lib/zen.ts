// View ▸ Distraction Free (zen.rs): the page's window showing the text alone. The class `zen` on the page's document
// while it does; what it hides says so (`data-zen`: theme.css).

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "$lib/events";

/** The page's document following its window's Distraction Free: as the page loads, and as it changes. */
export function followDistractionFree() {
  const set = (on: boolean) => document.documentElement.classList.toggle("zen", on);
  void invoke<boolean>("distraction_free_of").then(set).catch(() => {});
  const mine = getCurrentWindow().label;
  return listen<[string, boolean]>("distraction-free", (e) => {
    if (e.payload[0] === mine) set(e.payload[1]);
  });
}
