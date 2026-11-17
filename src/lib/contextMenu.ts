// A right-click menu shown once the right button is up. A two-finger tap on a touchpad (Linux) sends the button's
// press and release at once; a menu shown on the press (when WebKit says `contextmenu`) was closed by the release
// that followed, at once. Shown after the release, it stays.

import type { Menu } from "@tauri-apps/api/menu";
import type { LogicalPosition } from "@tauri-apps/api/dpi";

let held = false;
let waiting: (() => void)[] = [];

/** The right button followed in `target` (the page, and a frame of it: the preview). */
export function followButtons(target: Document | Window) {
  target.addEventListener("mousedown", (e) => {
    if ((e as MouseEvent).button === 2) held = true;
  });
  target.addEventListener("mouseup", (e) => {
    if ((e as MouseEvent).button !== 2) return;
    held = false;
    const now = waiting;
    waiting = [];
    now.forEach((go) => go());
  });
}

if (typeof window !== "undefined") followButtons(window);

/** `menu` shown at `at` (none: at the pointer) once the right button is up (or after a while, if its release was not
 *  seen: a press outside the page's documents). */
export async function popupReleased(menu: Menu, at?: LogicalPosition) {
  if (held) await Promise.race([new Promise<void>((r) => waiting.push(r)), new Promise((r) => setTimeout(r, 800))]);
  held = false;
  // the release handled by the system before the menu takes the pointer
  await new Promise((r) => setTimeout(r, 30));
  await menu.popup(at);
}
