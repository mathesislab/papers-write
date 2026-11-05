# papers-write

A future app for writing scientific papers, made to work with the rest of
the papers family:

- [papers](https://github.com/perrette/papers) and
  [papers-desktop](https://github.com/mathesislab/papers-desktop) for the
  library: `.bib` files and their PDFs, searched and cited (`[@key]`,
  `\cite{key}`) from the writing.
- [papers-author](https://github.com/mathesislab/papers-author) for
  bibliometrics.

The likely shape: drafting in Markdown with citation keys from the library,
converted to journal-formatted LaTeX/PDF (as with texmark), with the
library's search, previews and missing-reference checks at hand while
writing.

Status: early. The editor (CodeMirror, Markdown) is written here and shared
with papers-review as `papers-write/editor`; the app is a plain-text editor.

## Use

- Any file opens at once: File ▸ Open… (⌘O), Open With or a double click
  (md, markdown, txt, tex, bib, yaml, toml, pdf), a drop on the Dock icon or on a
  window. A file already open comes forward.
- Each file is a macOS window tab (System Settings ▸ Desktop & Dock ▸ Prefer
  tabs when opening documents); a tab dragged out is a window. ⌘N is a new
  window of untitled text, kept in the window; ⌘S asks where to save it, and
  from then on it is that file.
- A file is saved as typed, half a second after the typing stops. If it
  changes on disk (another editor, a sync client), it is read again, or the
  bar asks which to keep.
- A PDF opens in a tab like the text files (PDFKit, as Preview shows it): the
  page in view (click it to go to a page), zoom in, out and to fit the width
  (⌘= ⌘- ⌘0, PDF menu), Find (⌘F, next ⌘G, previous ⇧⌘G). A PDF that changes on
  disk, a build rewriting it, is read again at the same page.
- Build ▸ Build (⌘B) writes the document and runs `texmark <file> --pdf` in its
  folder, into `build/` beside it (`build/name.tex`, `name.pdf`, `name.log`;
  Markdown documents only, saved). Build and Show PDF (⇧⌘B) then opens the PDF
  in a tab, which reloads by itself on each rebuild. A mark in the corner
  says building, done (the time) or failed; a failure lists the first errors
  of the LaTeX log (`!` lines with the line of the `.tex`) in a sheet, with
  the log to reveal in Finder or open as text. One build per document at a
  time; one asked meanwhile runs after it. texmark is found on PATH, in
  `/opt/homebrew/Caskroom/miniforge/base/bin` or `~/.local/bin` (`pip install
  texmark`); the LaTeX engine in `/Library/TeX/texbin`.
- Build ▸ Marked-up PDF… makes the marked-up manuscript for a revision: choose
  the earlier version, a tag or one of the last 30 commits that touched the
  file (when it is in a git repository), or another file. Each is converted
  to LaTeX by texmark, `latexdiff` marks the changes, `latexmk` makes the PDF:
  `build/diff/name-diff-<version>.pdf`, shown in a tab (the work is in
  `build/diff/old` and `build/diff/new`; the current version is built there in
  full, which takes a few seconds more than Build). Errors show as for a build.
- The app stays in the menu bar when the last window closes (New, Open…,
  Quit); ⌘Q quits.
- Edit ▸ References… (⇧⌘R) lists the references over the editor: the `.bib`
  files named by `bibliography:` in the front matter, and the papers library.
  Type to search (words, author, year); ↵ puts in a citation, ⌘. `[@key]`,
  ⌘L `\cite{key}`, ⌘↵ opens the PDF, esc closes. The references are read when
  first asked, and again when a `.bib` changes.
- The bibliography grows as you cite: a key put in with ⌘. or ⌘L is copied from
  the library into the document's own `.bib` (the first one the front matter
  names; else `references.bib` beside it, named in the front matter). It needs
  a saved document.
- Edit ▸ Check Citations… lists the keys cited that the document's `.bib` lacks
  (Add from library) and the entries it never cites.
- View ▸ Show Preview (⇧⌘P) shows the Markdown rendered beside the text (a
  Markdown document only; again hides it). It is pandoc's HTML (`brew install
  pandoc`): math as MathML, citations by citeproc from the `bibliography:` of
  the front matter, images beside the file shown. It is made 300 ms after the
  typing stops, and again when the file changes on disk; the pane scrolls to
  the block the cursor is in (matched by order, as pandoc gives no source
  positions). What pandoc says that went wrong is in a bar at its foot; a
  bibliography it cannot read leaves the citations as typed. Equations
  (`$$..$$ {#eq:x}`, `equation`/`align`/`gather`), figures, tables and
  sections (`{#fig:x}`, `{#tbl:x}`, `{#sec:x}`) are numbered, and `@fig:x`,
  `@eq:x`, `\eqref{x}` ... resolved, by a Lua filter of pandoc (not
  pandoc-crossref, which need not be installed) as texmark does.
  Drag the split to resize; its width is kept for the window.

## Develop

```bash
npm install
npm run tauri dev
```

Rust (stable) and Node.js.

## License

MIT.
