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
- `prefs: EditorPrefs`, made with `prefsFrom(settings)` from anything shaped as `EditorSettings` (`{ indent, readable_width, line_numbers }`); `symbols`, the user's own `[sequence, symbol]` pairs; `mono`; `latex` (a `.tex`, `.sty` or `.cls` file: highlighted by `lang/tex.ts`, in monospace, completed as Markdown is but with every LaTeX name after a backslash and the labels read from `\label{}`; the sequences typed and `\name` with Tab are off, the file stays ASCII LaTeX); `onchange`; the bindable `status`, `error`, `path`, `selected`.
- `extensions: Extension[]`: more CodeMirror extensions, for what is the app's own (Papers Review colours the page and line of a PDF quote this way).
- `placeholder`: what an empty editor shows (none by default; Papers Review says how to write a review).
- `hover: HoverSources` (optional; without it there are no tooltips, as in Papers Review): after 300 ms over a formula (`$…$`, `$$…$$`, a math environment; in a `.tex` file found in the text by `latexMathAt(text, pos)`, with `\$` and `%` comments respected), a citation key, an image path (`![…](path)`, `\includegraphics{path}`) or a cross-reference key, a tooltip in the editor's CSS variables; none while a mouse button is down (a selection being made). `HoverSources` is `{ math?(tex, display) => Promise<string>, citation?(key) => Promise<{ label, detail? } | null>, image?(path) => Promise<string | null> }`: `math` gives HTML with a `<math>` element (MathML: Papers Write's `render_math`, pandoc), `citation` the reference (`label` "Schoof (2007)", `detail` the title and journal), `image` a data URL (shown at 240 px wide at most). A cross-reference needs no source: its target is read from the text, as "Figure: The hysteresis loop", "Equation: \tau_b = …", "Section: Introduction" (`labelTarget(state, key)`; Markdown attributes and LaTeX `\label`, with the `\caption` of the figure or table around it). `targetAt(state, pos)` is what the pointer is over.
- `diagnostics: Diagnostics` (optional; none without it): wavy underlines with the reason on hover. The editor finds by itself the cross-references to a label the document does not define (`no label fig:x in the document`; not in a LaTeX file that `\input`s others) and the labels defined twice (both; `labelProblems(state)`). `missingKeys?(keys: string[]) => Promise<Set<string>>` is the app's: of the citation keys of the text (each once) those found neither in the bibliography nor in the library, asked 800 ms after the typing stops (a key under the cursor is not underlined) and again on `recheckKeys()` of FileEditor (`recheck(view)`). The underlines are plain decorations (`.cm-problem-warning`, `.cm-problem-error`), no `@codemirror/lint`.
- `spellcheck` (boolean; Markdown only): the system's spell checker on the prose (`contentAttributes` `spellcheck`; autocorrect and autocapitalize stay off). Math, code, keys, paths, the front matter and LaTeX commands are marked `spellcheck="false"` (`notProse`), as far as WebKit honours the attribute on part of a text.
- `outline` (boolean): the outline in a column at the left of the text (`Outline.svelte`: the headings by level, the current one marked, a click goes there). `outline(state)` is the list, `{ level, title, from }[]` (Markdown headings from the syntax tree, ATX and underlined, none in code; `\section`… lines of a `.tex` file), `currentHeading(headings, pos)` the one the cursor is under. The menu item that toggles it is the app's.
- `autofocus`: the keys to the editor once the file is read (a window that is the editor).
- `completion: Completion` (optional; without it the editor has no completion, as in Papers Review): what the app gives for the popup of citations, references and LaTeX names (`makeEditor` takes it as `options.completion`). The types are in `types.ts`:
  - `citations(prefix) => Promise<CitationOption[]>`: the papers for `@key`, `[@key`, `[-@key`, `; @key` and `\cite{`, `\citep{`, `\citet{`, `\citeauthor{` (after `{` or `,`). A `CitationOption` is `{ key, label, detail?, inBib }`: `label` is shown ("Schoof 2007"), `detail` beside it (the title); those not `inBib` are listed under "From the library" with a "＋" before the detail. The app does the matching of `prefix`; the list is asked again after each key typed.
  - `oncite(key)`: a citation key was accepted (the app adds it to the bibliography). Accepting inserts the key, nothing else.
  - `paths(prefix) => Promise<string[]>` (optional): the files for `![…](` and `\includegraphics{`, `\input{`, `\include{`.
  - Not the app's: the labels of the document (`{#fig:x}`, `\label{x}`) after `@fig:`, `@eq:`, `@tbl:`, `@sec:`, `@lst:` (of that kind) and in `\ref{`, `\eqref{`; LaTeX names after a backslash: in math and LaTeX environments the usual ones (`lang/latex-names.ts`), in the text only the structural commands and the environments (`\begin{equation}`, `align`, `figure`, `table`), so that `\alpha` and Tab there is still α.
  - Tab and Enter accept the option marked when the list is open (Tab before the symbols and the indent; Enter does what it did otherwise when it is not), Esc closes it. Ordinary prose never opens it.
- The sequences typed (`->`, `^2`, `--`) and `\name` with Tab do not apply in code, in math (`$…$`, `$$…$$`, `equation`, `align`...), in LaTeX environments, in the front matter, nor in citation keys, labels and paths. In math and LaTeX Tab in the middle of a line leaves the text alone (no indent).
- It calls `changedOnDisk()` when the file changed elsewhere; `flush`, `reload`, `focus`, `view`, `undo`, `redo`, `indent`, `outdent`, `list` are exported.
- `review: ReviewHandlers` (optional; without it there is no review, as in Papers Review): the suggestions and comments drawn over the text, and suggest mode. Then `setReview(items)`, `setMode(mode)`, `accept(item)`, `scrollTo(id)` are exported too (see below); `undo` and `redo` step through the user's suggestions in suggest mode.

`RefList.svelte` (the references over the editor):
- `search: (query: string, limit: number) => Promise<Found>`, `closest: (text: string, limit: number) => Promise<Found>`, `sources: () => Promise<string[]>` (the .bib files shown in the footer).
- `text` (a selected reference: the closest papers are listed first), `enter` (what ↵ puts in: `"citation"`, Schoof (2007), by default, or `"pandoc"`, `[@key]`), `oninsert(text)`, `onopen(path)` (a paper's PDF), `onclose()`.
- It gives `cite(form)` (`"citation" | "reference" | "pandoc" | "latex"`): the paper marked put in. macOS takes ⌘. for Cancel before the page sees it, so the app needs a menu item with ⌘. that calls `cite("pandoc")` on the list.

`SymbolList.svelte`: `entries` (`DEFAULTS` and `LATEX` merged with the user's, see `merged`), `oninsert`, `onclose`.

## The review

The suggestions and comments of everyone, drawn over the text as Word's All Markup: the editor's text is always the file's, and nothing of the review is ever in it. The types are in `review/types.ts` (the app's Rust, papers-write-core's `review::Item`, mirrors them); positions are UTF-16 code units of the editor's text.

- Drawn: a suggestion's text struck through and what it puts there after it, both in the colour of its author (from a hash of the person's slug, the `<slug>` of `<slug>-<n>` ids, over `--review-1` … `--review-6`); a comment as a light highlight of its text, or a small mark where it has none. Items whose text changed or is gone (`status` outdated, orphaned) are dimmed; those not `open` are not drawn. A tooltip on hover says who, when and what (the note or comment, how many replies). A click on one calls `select(id)`.
- Edit mode: typing changes the text as usual; the items move with it until the app gives them again.
- Suggest mode: what the user types (CodeMirror's `input`, `delete` and `move` changes) becomes their suggestions and the text does not change. Typing at a point is an insertion; typing on, or in the text one's own suggestion replaces, adds to it; a selection typed over or deleted is one suggestion taking in one's own suggestions it covers; Backspace takes from what one's own suggestion inserts, else strikes the character before (the deletion growing left, the cursor at its start); Delete strikes the character after. One's own suggestions that come to touch are one; one typed away to nothing (or back to the text) is withdrawn. Someone else's are left alone: typing in their range suggests on the text beneath. The cursor stays at the end of the suggestion typed into, drawn after what it inserts. The symbols typed as sequences and the completion are off (what is typed is not in the text before the cursor). The app's own changes (reading the file again, `replaceText`, `accept`) go through as edits.
- The user's suggestions (theirs: ids `<me>-<n>`) are kept in the editor: made by typing, or taken from the app's items when open and in place. Each is sent with `suggest(s)` 400 ms after its last change, with its `id` once known; the answer is its id (a new one when the app writes an edited suggestion as a new record: the old one is then no longer taken from the items). One request at a time for each: typing on while a new one's id is awaited is sent after the answer, with the id. A suggestion typed away is withdrawn (`withdraw(id)`). What has not gone out when the editor closes is sent then. An item of the user's being typed into is shown as typed, not as the app last gave it.
- Undo and redo (⌘Z, ⇧⌘Z, ⌘Y, and FileEditor's `undo`, `redo`) in suggest mode step through the history of the user's suggestions (typing within half a second, into the same suggestion, is one step); the text's own history is not touched. A step back sends what it changes like any typing (a suggestion withdrawn and brought back is a new one).
- The mode indicator, the review panel, replies and decisions are the app's.

```ts
import { review, setReview, setReviewMode, reviewMode, accept, scrollToItem, itemsAt, undoSuggestion, redoSuggestion } from "papers-write/editor";
```

- `review(handlers: ReviewHandlers): Extension`: the review of an editor (it can sit in a Compartment). `ReviewHandlers` is `{ me, suggest(s: Suggested): Promise<string>, withdraw(id): Promise<void>, select?(id) }`; `Suggested` is `{ id?, from, to, insert }`.
- `setReview(view, items: ReviewItem[])`: the app's items on the editor's current text; they replace those drawn.
- `setReviewMode(view, "edit" | "suggest")`, `reviewMode(state)`.
- `accept(view, item)`: a suggestion's `replacement` put at `at` (where it is now in the editor) as an ordinary edit, `userEvent` `"review.accept"`, in either mode; it is not drawn until the app gives the items again. Writing the decision is the app's.
- `scrollToItem(view, id)`: scrolled to and its range selected (false if not drawn). `itemsAt(state, pos)`: the items drawn whose range holds `pos`, where they are now (a suggestion of the user's not yet answered has an empty `id`).
- `undoSuggestion(view)`, `redoSuggestion(view)`: a step of the suggestions' history in suggest mode (true), false in edit mode.

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
| `--error` | an error shown, the underline of a reference to a label not defined | `#c4321f` / `#ff7a66` |
| `--warning` | the underline of a citation key not found (default `#d98e04`) | |
| `--md-heading` | Markdown headings | `#2f6fde` / `#7ea6f5` |
| `--md-quote` | Markdown quotes | `#5f5f66` / `#b4b4bc` |
| `--md-meta` | the keys of the YAML front matter | `--md-heading` |
| `--md-math` | the LaTeX commands, in math and in the text, and the names of environments | `--accent` |
| `--md-math-number` | the numbers in math | `--md-quote`, else `--muted` |
| `--md-cite` | the keys of citations (`[@key]`, `@key`, `\cite{key}`) | `--accent` |
| `--md-ref` | cross-references (`@fig:x`, `\ref{x}`) and labels (`{#fig:x}`, `\label{x}`) | `--md-cite` |
| `--review-1` … `--review-6` | the colours of the reviewers (suggestions, comments) | `#c2410c`, `#7c3aed`, `#0f766e`, `#be185d`, `#1d4ed8`, `#4d7c0f` |
| `--editor-font`, `--editor-size` | the font and size of the Markdown (with a default); set by `setEditorFont` | |
