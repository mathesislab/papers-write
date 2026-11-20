// A right-click menu shown once the right button is up. A two-finger tap on a touchpad (Linux) sends the button's
// press and release at once; a menu shown on the press (when WebKit says `contextmenu`) was closed by the release
// that followed, at once. Shown after the release, it stays.

import { invoke } from "@tauri-apps/api/core";
import type { Menu } from "@tauri-apps/api/menu";
import type { LogicalPosition } from "@tauri-apps/api/dpi";

let held = false;
let waiting: (() => void)[] = [];
/** where the pointer last pressed, in the page (a frame's press moved by the frame's place) */
let pressed: { x: number; y: number } | null = null;

/** The right button followed in `target` (the page, and a frame of it: the preview), and where each press is. */
export function followButtons(target: Document | Window) {
  target.addEventListener("mousedown", (e) => {
    const m = e as MouseEvent;
    const frame = (m.view as Window | null)?.frameElement?.getBoundingClientRect();
    pressed = { x: m.clientX + (frame?.left ?? 0), y: m.clientY + (frame?.top ?? 0) };
    if (m.button === 2) held = true;
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
  await showMenu(menu, at);
}

const MAC = typeof navigator !== "undefined" && navigator.userAgent.includes("Mac");

/** `menu` shown at `at` (none: at the pointer) by the app's own popup (menu.rs: `popup_menu`), not Tauri's: Tauri's
 *  holds the page's menus while the menu shows, and a menu call of the page meanwhile froze the app (Linux). */
export async function showMenu(menu: Menu, at?: LogicalPosition) {
  if (MAC) return menu.popup(at);
  // always at a place in the window: a menu "at the pointer" is put on the screen's root window, which Wayland has no
  // popups on; GTK takes the pointer and the keys for it all the same, and the menu never shown is never closed (the
  // app's windows dead to clicks and keys)
  const place = at ?? pressed ?? { x: 0, y: 0 };
  await invoke("popup_menu", { rid: menu.rid, x: place.x, y: place.y });
}
