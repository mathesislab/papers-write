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

/** What showing a file in the file manager is called: the Finder's on macOS, the folder's elsewhere. */
export const REVEAL = mac ? "Reveal in Finder" : "Show in Folder";
/** The project's folder opened in the file manager. */
export const OPEN_FOLDER = mac ? "Open Project Folder in Finder" : "Open Project Folder";
