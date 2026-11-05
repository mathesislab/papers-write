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
- `placeholder`: what an empty editor shows (none by default; Papers Review says how to write a review).
- `completion: Completion` (optional; without it the editor has no completion, as in Papers Review): what the app gives for the popup of citations, references and LaTeX names (`makeEditor` takes it as `options.completion`). The types are in `types.ts`:
  - `citations(prefix) => Promise<CitationOption[]>`: the papers for `@key`, `[@key`, `[-@key`, `; @key` and `\cite{`, `\citep{`, `\citet{`, `\citeauthor{` (after `{` or `,`). A `CitationOption` is `{ key, label, detail?, inBib }`: `label` is shown ("Schoof 2007"), `detail` beside it (the title); those not `inBib` are listed under "From the library" with a "＋" before the detail. The app does the matching of `prefix`; the list is asked again after each key typed.
  - `oncite(key)`: a citation key was accepted (the app adds it to the bibliography). Accepting inserts the key, nothing else.
  - `paths(prefix) => Promise<string[]>` (optional): the files for `![…](` and `\includegraphics{`, `\input{`, `\include{`.
  - Not the app's: the labels of the document (`{#fig:x}`, `\label{x}`) after `@fig:`, `@eq:`, `@tbl:`, `@sec:`, `@lst:` (of that kind) and in `\ref{`, `\eqref{`; LaTeX names after a backslash: in math and LaTeX environments the usual ones (`lang/latex-names.ts`), in the text only the structural commands and the environments (`\begin{equation}`, `align`, `figure`, `table`), so that `\alpha` and Tab there is still α.
  - Tab and Enter accept the option marked when the list is open (Tab before the symbols and the indent; Enter does what it did otherwise when it is not), Esc closes it. Ordinary prose never opens it.
- The sequences typed (`->`, `^2`, `--`) and `\name` with Tab do not apply in code, in math (`$…$`, `$$…$$`, `equation`, `align`...), in LaTeX environments, in the front matter, nor in citation keys, labels and paths. In math and LaTeX Tab in the middle of a line leaves the text alone (no indent).
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
| `--md-meta` | the keys of the YAML front matter | `--md-heading` |
| `--md-math` | the LaTeX commands, in math and in the text, and the names of environments | `--accent` |
| `--md-math-number` | the numbers in math | `--md-quote`, else `--muted` |
| `--md-cite` | the keys of citations (`[@key]`, `@key`, `\cite{key}`) | `--accent` |
| `--md-ref` | cross-references (`@fig:x`, `\ref{x}`) and labels (`{#fig:x}`, `\label{x}`) | `--md-cite` |
| `--editor-font`, `--editor-size` | the font and size of the Markdown (with a default); set by `setEditorFont` | |
