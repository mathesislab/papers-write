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
  (md, markdown, txt, tex, bib, yaml, toml), a drop on the Dock icon or on a
  window. A file already open comes forward.
- Each file is a macOS window tab (System Settings ▸ Desktop & Dock ▸ Prefer
  tabs when opening documents); a tab dragged out is a window. ⌘N is a new
  window of untitled text, kept in the window; ⌘S asks where to save it, and
  from then on it is that file.
- A file is saved as typed, half a second after the typing stops. If it
  changes on disk (another editor, a sync client), it is read again, or the
  bar asks which to keep.
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

## Develop

```bash
npm install
npm run tauri dev
```

Rust (stable) and Node.js.

## License

MIT.
