// What the app says of keys and of the file manager, as the platform has them: macOS's symbols (⌘ ⌥ ⇧ ⌃ ↵) and the
// Finder there; elsewhere the keys written out (Ctrl+Shift+E, Ctrl+Enter) and the file manager's folder. The texts of
// the app are written with macOS's symbols and go through `keys` where they are shown.

/** The app runs on macOS (its webview says so). */
export const mac = typeof navigator !== "undefined" && /Mac/.test(navigator.userAgent);

const NAMES: Record<string, string> = { "⌃": "Ctrl", "⌥": "Alt", "⇧": "Shift", "⌘": "Ctrl" };
const ORDER = ["⌃", "⌘", "⌥", "⇧"];
const KEYS: Record<string, string> = { "↵": "Enter", "−": "-", "⌫": "Backspace", "⌦": "Delete", "⎋": "Esc" };

/** `text` with its keys as the platform writes them: on macOS as it is; elsewhere each run of modifier symbols and the
 *  key after it written out (`(⇧⌘E)` → `(Ctrl+Shift+E)`, `⌘↵` → `Ctrl+Enter`), on its own `↵` as Enter. */
export function keys(text: string, onMac = mac): string {
  if (onMac) return text;
  return text
    .replace(/([⌃⌥⇧⌘]+)(↵|⌫|⌦|⎋|Tab|[^\s)⌃⌥⇧⌘,·])/gu, (_, mods: string, key: string) => {
      const names = [...new Set(ORDER.filter((m) => mods.includes(m)).map((m) => NAMES[m]))];
      return [...names, KEYS[key] ?? (key.length === 1 ? key.toUpperCase() : key)].join("+");
    })
    .replace(/↵/g, "Enter");
}

/** The keys of a key press, as `trashKey` reads them. */
type Press = { key: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean };

/** What a key does to the file chosen in a list of files, as the file manager has it: moves it to the Trash asked first
 *  ("ask") or at once ("now": it is in the Trash still), or nothing (null). On macOS ⌘⌫ at once, as the Finder does,
 *  and ⌫ (or ⌦) alone asked first; elsewhere Delete (or Backspace) asked first, ⇧Delete at once. */
export function trashKey(e: Press, onMac = mac): "ask" | "now" | null {
  if ((e.key !== "Delete" && e.key !== "Backspace") || e.altKey || e.ctrlKey) return null;
  if (onMac) return e.shiftKey ? null : e.metaKey ? "now" : "ask";
  if (e.metaKey) return null;
  return e.shiftKey ? "now" : "ask";
}

/** What showing a file in the file manager is called: the Finder's on macOS, the folder's elsewhere. */
export const REVEAL = mac ? "Reveal in Finder" : "Show in Folder";
/** The project's folder opened in the file manager. */
export const OPEN_FOLDER = mac ? "Open Project Folder in Finder" : "Open Project Folder";
