// View ▸ Distraction Free (zen.rs): the page's window showing the text alone. The class `zen` on the page's document
// while it does; what it hides says so (`data-zen`: theme.css).

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "$lib/events";

/** The page's document following its window's Distraction Free: as the page loads, and as it changes. */
export function followDistractionFree() {
  let on = false;
  const set = (now: boolean) => document.documentElement.classList.toggle("zen", (on = now));
  // a way out of it whatever the menu's keys do: F11, Shift+F11, and Esc when nothing on the page took it
  window.addEventListener("keydown", (e) => {
    if (on && (e.key === "F11" || (e.key === "Escape" && !e.defaultPrevented))) void invoke("leave_distraction_free").catch(() => {});
  });
  void invoke<boolean>("distraction_free_of").then(set).catch(() => {});
  const mine = getCurrentWindow().label;
  return listen<[string, boolean]>("distraction-free", (e) => {
    if (e.payload[0] === mine) set(e.payload[1]);
  });
}
