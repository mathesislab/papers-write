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
  Markdown documents, saved; LaTeX below). Build and Show PDF (⇧⌘B) then opens the PDF
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
- LaTeX files (`.tex`, also `.sty` and `.cls` for the editing) are edited in
  monospace with the commands, environment names, math (`$…$`, `\(…\)`,
  `\[…\]`, `equation` and `align`...), comments and the keys of `\cite`,
  `\ref` and `\label` coloured as in Markdown. The file stays ASCII LaTeX: no
  symbols typed (`->`), and `\alpha` with Tab stays as written. The completion
  is on: citation keys after `\cite{` (and `\citep{`...), from the
  `\bibliography{a,b}` / `\addbibresource{a.bib}` of the file and the papers
  library (an accepted key is added to the first of those `.bib` files, which
  is `references.bib` with its `\bibliography` line put in if the file names
  none), labels after `\ref{` and `\eqref{`, every command name after a
  backslash, and environments (`\begin{`). Build (⌘B, ⇧⌘B) runs `latexmk -pdf
  -interaction=nonstopmode -file-line-error -outdir=build` in the file's folder
  (`build/name.pdf`, `build/name.log`, as for texmark); an error at a line of
  the file is listed with it, and a click on it puts the cursor there (errors
  in an included file show its name). Marked-up PDF… runs `latexdiff` on the
  earlier and current `.tex` directly (no texmark) and compiles with latexmk
  into `build/diff/name-diff-<version>.pdf`. `.bib` files stay plain text.
- File ▸ Exchange ▸ Send for Review… makes one file, `<name> — <round>.review`
  (round: "Draft of <date>" to begin with; saved beside the document unless a
  dialog says otherwise), for someone who has the app but not git: a zip (the
  system's `ditto`, so macOS) with a `manifest.toml`, the text as sent, the
  figures it shows (`![…](…)`, `\includegraphics{…}`) and the bibliography
  files of its front matter at their relative paths, and the review files of
  the round. Opened (double click, Open With, ⌘O, a drop), it unpacks in the
  app's data folder (`reviews/<name>-<hash>/`, once: opening it again finds what
  was done there) and the document opens titled "<name> — review for <who>",
  suggesting, with a bar at the top (edits made in the text itself do not go
  back, only suggestions and comments). Send Back… (the bar, File ▸ Exchange)
  makes `<name> — <you>.review` with your own review file and the texts it was
  made on. Opened where the document is (the open window with that text, else
  the document asked for), it joins that review: missing bases copied, your
  file copied or its new records appended (never rewritten, so the same file
  twice adds nothing), a dialog saying "12 suggestions and 3 comments from Jane
  Doe". If the text changed since, they are carried like any others.
- File ▸ Exchange ▸ Compare as Suggestions… turns another version into
  suggestions: a commit, tag or branch of the document in git, or an edited
  copy of the file (any path). The word-level changes from this document to
  that version become one suggestion each, under the name of whoever made it
  (the commit's author, filled in; asked for a file) in a review file of their
  own, with the round "Compared with <file or revision>". The author need not
  have the app.
- File ▸ Exchange ▸ Export as Word with Changes… writes a `.docx` (pandoc;
  saved where a dialog says, `<name>.docx` beside the document to begin with)
  in which the open suggestions are Word's tracked changes and the comments
  Word's comments, under their authors' names and dates. Math becomes Word
  equations; equations, figures, tables and sections are numbered and their
  references resolved as in the preview (`@fig:x`, `\ref{eq:y}`); citations are
  rendered by citeproc when the front matter names a bibliography. A suggestion in math, code or raw LaTeX, which a tracked
  change cannot be put in, is a comment that says what it suggests. A sheet
  lists what was left out (a suggestion that overlaps an earlier one, one in
  the front matter).
- File ▸ Exchange ▸ Import Word Review… reads a `.docx` back (pandoc): its
  tracked changes and comments become suggestions and comments, each under the
  name Word has for its author (their own review file). Each is placed in the
  Markdown by the words it concerns and the words around them, ignoring the
  Markdown's punctuation (`*`, `[@key]`, `$..$`), so it finds its place
  whatever the markup. Changes made with Track Changes off are looked for only
  if the sheet's box is ticked: the whole text is compared with the document
  made into a .docx as an export does (citations resolved or as typed), a
  change the tracked pass has is not found again, and the rest are given to
  the author named in the sheet ("Word" if none). A sheet lists what could not be placed (words
  not found, or in two places the words around do not tell apart): those are
  not guessed. Only the text comes back: Word's formatting is dropped, and a
  change that takes in a citation, math or markup is placed on the words
  around it.
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
- Review menu: suggestions and comments kept in files beside the document
  (`.papers-write/review/<file>/`, one file per person, only appended to).
  Suggest Changes (⇧⌘E) makes typing a suggestion of yours instead of an edit
  (a "Suggesting" badge at the top right, per window). Add Comment… (⌥⌘M)
  comments on the selection (⌘↵ posts). Show Review Panel (⌥⌘R) lists the
  open ones in text order at the right, in place of the preview: who and when,
  what changes, the thread, and Accept, Reject, Reply, Resolve, Suggest
  Instead (a counter-suggestion), Withdraw (your own); filter by person, show
  the resolved. Accept All and Reject All (everyone's open suggestions, after
  asking; Accept All leaves the outdated ones and those a counter-suggestion
  answers); Next and Previous Change (⌥⌘↓, ⌥⌘↑). You are the `user.name` of the
  git configuration of the document's folder, else your macOS full name. Needs
  a saved document. Changes others make to the review files (a pull, a sync)
  show at once.
- Git (the `git` command): when the document is in a repository, a quiet line
  at the foot of the window says the branch, the files changed and how far it
  is from its upstream ("main · 3 changed · ↓1"; the upstream is fetched when
  the window gets the keys, at most every 5 minutes). A click on it, or Git ▸
  Commit… (⌥⌘K), opens the files changed: the document, its `.bib` files and
  its review folder are ticked, the rest is not. Commit adds and commits those
  paths only (never `-a`, never an amend). After Accept, Accept All or Reject
  All the review panel offers the commit with a message that says what was
  decided. Pull is `git pull --ff-only`: if the branch cannot go forward
  nothing is done and it is said (resolve it in a terminal). Push is `git
  push`, never forced, and needs an upstream. Git ▸ Compare with Commit…
  opens a tab "paper.md since v2": a tag or a commit that touched the file (or
  any revision typed), its text read only, with the changes since drawn as
  tracked changes.

## Develop

```bash
npm install
npm run tauri dev
```

Rust (stable) and Node.js.

## License

MIT.
