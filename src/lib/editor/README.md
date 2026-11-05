# The editor

The text editor of Papers Review and Papers Write: CodeMirror 6, Markdown with the markup dimmed (or plain text in monospace), the symbols typed as sequences or by LaTeX name, the sections of a file as tabs, a list of the references of the project to cite from. It is a self-contained module: only relative imports, no `$lib`, no Tauri. What belongs to an app is passed in.

## Use

Svelte 5 and CodeMirror 6 (`@codemirror/*`, `@lezer/highlight`) are the app's dependencies. From an app that has this repository as an npm git dependency:

```ts
import { FileEditor, RefList, SymbolList, makeEditor, prefsFrom, setEditorFont } from "papers-write/editor";
```

## What the app supplies

`FileEditor.svelte`:
- `load: () => Promise<{ path: string; text: string }>` and `save: (path: string, text: string) => Promise<void>`: where the file is read from and written to.
- `prefs: EditorPrefs`, made with `prefsFrom(settings)` from anything shaped as `EditorSettings` (`{ indent, readable_width, line_numbers }`); `symbols`, the user's own `[sequence, symbol]` pairs; `mono`; `onchange`; the bindable `status`, `error`, `path`, `selected`.
- `extensions: Extension[]`: more CodeMirror extensions, for what is the app's own (Papers Review colours the page and line of a PDF quote this way).
- It calls `changedOnDisk()` when the file changed elsewhere; `flush`, `reload`, `focus`, `view`, `undo`, `redo`, `indent`, `outdent`, `list` are exported.

`RefList.svelte` (the references over the editor):
- `search: (query: string, limit: number) => Promise<Found>`, `closest: (text: string, limit: number) => Promise<Found>`, `sources: () => Promise<string[]>` (the .bib files shown in the footer).
- `text` (a selected reference: the closest papers are listed first), `oninsert(text)`, `onopen(path)` (a paper's PDF), `onclose()`.
- It gives `cite(form)` (`"citation" | "reference" | "pandoc" | "latex"`): the paper marked put in. macOS takes ⌘. for Cancel before the page sees it, so the app needs a menu item with ⌘. that calls `cite("pandoc")` on the list.

`SymbolList.svelte`: `entries` (`DEFAULTS` and `LATEX` merged with the user's, see `merged`), `oninsert`, `onclose`.

`setEditorFont(font, size)` sets `--editor-font` and `--editor-size` on the page; call it when the settings change. The types of the references (`Item`, `Hit`, `Found` ...) are in `types.ts`; the app's Rust mirrors them.

## CSS variables

The editor and the components use these, set by the app's theme (light and dark); Papers Review's `src/lib/theme.css` has values to start from:

| variable | used for | Papers Review: light / dark |
|---|---|---|
| `--bg` | the reference list's background | `#f6f6f8` / `#262628` |
| `--fg` | text | `#1d1d1f` / `#f2f2f4` |
| `--muted` | markup, placeholder, line numbers, secondary text | `#6e6e73` / `#9a9aa0` |
| `--border` | the outline of the lists | `rgba(0,0,0,.14)` / `rgba(255,255,255,.16)` |
| `--line` | thin separators | `rgba(0,0,0,.08)` / `rgba(255,255,255,.08)` |
| `--accent` | cursor, links | `#2f6fde` / `#3a78e6` |
| `--selection` | selected text, the marked row of a list | `rgba(47,111,222,.16)` / `rgba(58,120,230,.35)` |
| `--chip` | fields and small buttons | `rgba(0,0,0,.05)` / `rgba(255,255,255,.07)` |
| `--error` | an error shown | `#c4321f` / `#ff7a66` |
| `--md-heading` | Markdown headings | `#2f6fde` / `#7ea6f5` |
| `--md-quote` | Markdown quotes | `#5f5f66` / `#b4b4bc` |
| `--editor-font`, `--editor-size` | the font and size of the Markdown (with a default); set by `setEditorFont` | |
