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

Status: all of the design is built (docs/design.md), not yet released. The
editor is written here and shared with papers-review as `papers-write/editor`,
and so is the project bar (`papers-write/project`).

Builds use texmark. The fixes it needs here (a `journal:` given as a name, the
default template, several cross-references in one bracket and the words before
them) are proposed upstream; until they are released,
`pip install git+https://github.com/alex-robinson/texmark@all-fixes`.

## Use

- Any file opens at once: File ▸ Open… (⌘O), Open With or a double click
  (md, markdown, txt, tex, bib, yaml, toml, pdf), a drop on the Dock icon or on a
  window. A file already open comes forward.
- Each file is a tab. A window shows one project at a time, with the tabs of
  its files: a file opens in its project's tabs, the window switched to it
  (the window that shows the project, if one does, else the window in front);
  a file outside any project joins what the window shows (Settings ▸ Window ▸
  Open files as tabs off: each file in a window of its own). Each project's
  tabs are kept as left, also across restarts, and the windows open when the
  app quits open again at the next start; a window closed by hand does not. The tabs
  are a strip at the top of the text, each as wide as its name, the close
  button at its right (a middle click closes too); a tab in the back keeps
  its text, undo, scroll and find. ⌘W closes the tab, the last tab its
  window. ⌘N is a new tab of untitled text; ⌘S asks where to save it, and
  from then on it is that file. A tab or window with untitled text asks
  before closing.
- A file is saved as typed, half a second after the typing stops. If it
  changes on disk (another editor, a sync client), it is read again, or the
  bar asks which to keep.
- A PDF opens in a tab like the text files (PDFKit, as Preview shows it): the
  page in view (click it to go to a page), zoom in, out and to fit the width
  (⌘= ⌘- ⌘0, PDF menu), Find (Edit ▸ Find: ⌘F, next ⌘G, previous ⇧⌘G). A PDF that changes on
  disk, a build rewriting it, is read again at the same page.
- Build ▸ Build (⌘B) writes the document and runs `texmark <file> --pdf` in its
  folder, into `build/` beside it (`build/name.tex`, `name.pdf`, `name.log`;
  Markdown documents, saved; LaTeX below). Build and Show PDF (⇧⌘B) then opens the PDF
  in a tab, which reloads by itself on each rebuild. The foot of the window
  says Build (a click builds), building, built (the time; a click shows the
  PDF) or failed; a failure lists the first errors
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
- A `.tex` file's `\input`, `\include` and `\subfile` are followed (relative to
  the file, `.tex` added to a name without it, ten deep, a cycle stopped): the
  `\bibliography` / `\addbibresource` of those files count for the references
  and their `\cite`s for Check Citations, and their `\label`s are known to the
  editor (`document_labels`).
- A Markdown build's LaTeX errors come with the line of the Markdown when it
  can be told: texmark keeps no source map, so the `.tex` line at the error is
  matched against the Markdown lines by their words (`\textbf{x}` as `x`,
  `\citep{k}` as `@k`, math as written); the line with the longest run of
  words in common is offered ("line N", a click goes there). A tie, or less
  than two words in common, gives no line (the `.tex` line is shown).
- File ▸ Exchange ▸ Send for Review… makes one file, `<name> — <round>.review`
  (round: "Draft of <date>" to begin with; saved beside the document unless a
  dialog says otherwise), for someone who has the app but not git: a zip (the
  system's `ditto`, so macOS) with a `manifest.toml`, the text as sent, the
  figures it shows (`![…](…)`, `\includegraphics{…}`) and the bibliography
  files of its front matter at their relative paths, and the review files of
  the round. Opened (double click, Open With, ⌘O, a drop), it unpacks in the
  app's data folder (`reviews/<name>-<hash>/`, once: opening it again finds what
  was done there; a folder not opened or worked in for 60 days is moved to the Trash
  when the app starts, never one that is open) and the document opens titled "<name> — review for <who>",
  suggesting, with a bar at the top (edits made in the text itself do not go
  back, only suggestions and comments). Send Back… (the bar, File ▸ Exchange)
  makes `<name> — <you>.review` with your own review file and the texts it was
  made on. Opened where the document is (the open window with that text, else
  the document asked for), it joins that review: missing bases copied, your
  file copied or its new records appended (never rewritten, so the same file
  twice adds nothing), a sheet on its window saying "12 suggestions and 3 comments
  from Jane Doe" (and the window's log of what it did, for the next commit's
  message, "review of Jane Doe received: 12 suggestions, 3 comments"). If the text changed since, they are carried like any others.
- File ▸ Exchange ▸ Compare as Suggestions… turns another version into
  suggestions: a branch or a commit (typed), or an edited copy of the file (any
  path). From git, only what was done there since it left your branch (`git
  merge-base`) comes in, each change under the author of the commit that made
  it; for a file, the word-level changes from this document to it, under the
  name asked for (round "Compared with <file>"). The author need not have the
  app. The same comparison twice adds nothing ("already in the review").
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
  Quit); ⌘Q quits (from the menu bar icon or the Dock too). If untitled windows have
  text that was never saved, it asks once: "2 windows have text that was never
  saved": Quit and discard, or Cancel.
- Projects: a project is a folder with a `write.toml` (below). File ▸ Open
  Project… (⇧⌘O) chooses a folder (File ▸ Open Recent lists the last ten
  projects opened, most recent first, a missing one disabled; Clear Menu
  forgets them; a choice opens the project as Open Project… does): with `write.toml` it joins the project bar
  and its main document opens (or comes forward); without, a card asks for
  the name, the short name of its tile, the colour and the main document (a
  Markdown or LaTeX file of the folder, or a new paper: the front matter of
  the paper-draft skeleton, with title, authors, date, bibliography and the
  journal to choose, and the headings of the sections), and Create writes
  `write.toml` (Cancel leaves nothing). A document in the folder of a project,
  or below it, belongs to it: its window shows the project bar at the left, a
  tile per project in its colour with its short name (else its initials), the
  main document's words and its open suggestions ("6.2k words", "3 open"),
  the current one marked (and, if Settings ▸ Window says so, a thin bar in the
  project's colour at the top). A click on a tile switches the window to that
  project (its tabs as left, else its main document), or brings forward the
  window that shows it; on the current project's tile it shows or hides the
  sidebar; a right-click: Open in New Window, Open Project Folder in Finder, Project Info…,
  Remove from Bar; `+` is Open Project…. File ▸ Project Info… (⌘I) is a card
  of the fields of `write.toml`, written on Done (Esc drops it). The bar and
  the projects opened last are kept in the app's `state.json`.

  ```toml
  name = "Ice sheet paper"
  short = "ISP"          # the tile: ten letters at most
  colour = "teal"        # blue teal green yellow orange red pink purple graphite none
  main = "paper.md"      # or ["paper.md", "supplement.md"]
  build = "build"        # the build folder (the default)
  library = "refs.bib"   # a .bib, or a papers library folder (.papers/config.json), or an absolute path
  ```

  The file is changed key by key: comments and other keys stay. A document
  of the project builds into its `build` folder (relative to the project's
  folder, `build` without; for texmark, latexmk, the marked-up PDF's `diff/`
  and Build and Show PDF); outside a project, into `build/` beside the
  document. A document of a project with `library` searches that (a `.bib`
  path, or a folder that has a papers `.papers/config.json`, relative to the
  project's folder or absolute) instead of the papers library found from its
  folder; the document's own bibliography is unchanged.
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
  pandoc-crossref, which need not be installed) as texmark does: bare numbers
  (`1`, `(1)` for an equation, `1 and 2`, `1, 2 and 3`), with words in front
  only when the front matter gives them: `crossref: {fig: Fig., figs: Figs.,
  eq: Eq., eqs: Eqs., tbl: Table, tbls: Tables, sec: Sect., secs: Sects., lst:
  Listing, lsts: Listings}` (a plural left out is the singular; empty is bare).
  Drag the split to resize; its width is kept for the window.
- The sidebar at the left (View ▸ Show Sidebar, ⌥⌘S, or a click on the current
  project's tile) has three tabs; shown or not, its width and its tab are the same in all the
  project's tabs, and kept for the next time. Files: the project's folder as a tree (the
  document's folder outside a project); a click opens a text file or PDF in a
  tab, anything else in its default app; a right-click makes a Markdown file,
  shows it in Finder, opens it in its default app or moves it to the Trash.
  Outline (⌥⌘O): the headings (`#`… in Markdown, `\section`… in a `.tex`
  file), indented by level, the one the cursor is under marked; a click goes
  there. Search (Edit ▸ Find ▸ Find in Project…, ⇧⌘F): the project's text
  files (`.md`, `.tex`, `.bib`, `.txt`, …; not hidden folders or `build/`),
  with case, whole word and regular expression; the matches by file, a click
  opens the file at the match; Replace All… asks first how many, in how many
  files (a file open in a window is changed by its editor, so undo and suggest
  mode apply). Shown or hidden, the tab and the width are kept.
- Edit ▸ Find: Find… (⌘F) and Find and Replace… (⌥⌘F) open a bar above the
  text: the matches counted, case, whole word, regular expression, previous
  and next (⇧⌘G ⌘G, ⇧↵ ↵), Replace (↵ in its field) and All (⌘↵). Use
  Selection for Find (⌘E). In suggest mode a replacement is a suggestion.
- With a readable width, the margins beside the column are a shade darker.
- Hover (a pause of 300 ms; none while the mouse is down): over math
  (`$..$`, `$$..$$`, `equation`, `align`...) the equation, rendered by pandoc
  as MathML (cached); over a citation key its reference (authors, year, title,
  journal); over an image path (`![..](path)`, `\includegraphics{path}`) a
  thumbnail; over a cross-reference (`@fig:x`, `\ref{x}`) what the label is
  on: "Figure: The hysteresis loop", "Equation: …".
- Underlines: a citation key found neither in the document's `.bib` files nor
  in the library (checked 800 ms after the typing stops, and when the
  references change), a reference to a label the document does not define
  (`@fig:x`, `\ref{x}`), and a label
  defined twice (both). Hover says why.
- Spelling: the system's spell checker (macOS) underlines misspelled words in
  the prose of Markdown text with a red dotted line, not in `.tex` and
  monospace files, and not in math, code, raw LaTeX, the front matter,
  citation and cross-reference keys, labels, URLs, link targets, attributes
  or HTML comments. It checks 500 ms after the typing stops and on scrolling;
  hover on a word for the suggestions (click one to replace), Learn (your
  dictionary) and Ignore (this document). The language is the system's, or
  `lang:` (e.g. `en-GB`, `de`) in the front matter. Settings ▸ Editor ▸
  Check spelling turns it off.
- In a `.tex` file with `\input`, `\include` or `\subfile`, a `\ref` to a label
  defined in an included file is not flagged.
- Review menu: suggestions and comments kept in files beside the document
  (`.papers-write/review/<file>/`, one file per person, only appended to).
  Suggest Changes (⇧⌘E) makes typing a suggestion of yours instead of an edit
  (the mode at the top right of the text, per window: Editing, or Suggesting
  in your colour, with the open items; a click shows the review panel, its ▾
  the modes and views). Add Comment… (⌥⌘M)
  comments on the selection (⌘↵ posts). Show Review Panel (⌥⌘R) lists the
  open ones in text order at the right, in place of the preview: who and when,
  what changes, the thread, and Accept, Reject, Reply, Resolve, Suggest
  Instead (a counter-suggestion), Withdraw (your own); filter by person and by
  round, show the resolved. With a person or a round chosen, the foot of the
  panel says "Accept all of Jane Doe" or "Accept all of round 'Draft of 5
  Nov'" (and Reject all) and takes just those. Accept All and Reject All of
  the menu take everyone's open suggestions, after asking; Accept All leaves
  the outdated ones and those a counter-suggestion answers. Next and Previous
  Change (⌥⌘↓, ⌥⌘↑). All Markup (⌥⌘1), Accepted (⌥⌘2) and Original (⌥⌘3),
  per window, change only what is drawn: the suggestions as marked, as if all
  were accepted (what they replace hidden, what they put in as plain text in
  their author's colour), or as if none were (comments still marked). The text
  is the file's in all three and edits as usual; suggesting is All Markup
  only, and choosing another view leaves it. Clean Up Review… removes from
  your own file the records of what is resolved (accepted, rejected, resolved,
  withdrawn) and the bases nothing needs, after a sheet says what goes; other
  people's files are not touched (they clean up their own) and git keeps
  everything. You are the `user.name` of the
  git configuration of the document's folder, else your macOS full name. Needs
  a saved document. Changes others make to the review files (a pull, a sync)
  show at once.
- Git (the `git` command): when the document is in a repository, a quiet item
  at the right of the window's foot says the branch, the files changed and how far it
  is from its upstream ("main · 3 changed · ↓1"; the upstream is fetched when
  the window gets the keys, at most every 5 minutes). A click on it, or Git ▸
  Commit… (⌥⌘K), opens the files changed: the document, its `.bib` files and
  its review folder are ticked, the rest is not. Commit adds and commits those
  paths only (never `-a`, never an amend). It is the one way to commit, for
  writing and reviewing alike: its message is filled in from what the window
  did since the last commit ("Accept 3 suggestions of Jane Doe; review of
  jane/edits as suggestions"), empty when you only wrote. Pull is `git pull
  --ff-only`: if the branch cannot go forward nothing is done and it is said
  (resolve it in a terminal). Push is `git push`, never forced, and needs an
  upstream. Git ▸ Compare with Commit… opens a tab "paper.md since v2": a tag
  or a commit that touched the file (or any revision typed), its text read
  only, with the changes since drawn as tracked changes.
- Reviewing a co-author's edits made in git (without the app, without suggest
  mode), to accept or reject one by one:
  - On a branch: Git ▸ Review Branch… fetches (quietly) and lists the local
    and remote branches with commits touching the document that yours has not,
    with their authors, how many and when. The one chosen becomes suggestions:
    the word-level changes from where it left your branch to its tip, each under
    the author of its commit (a file per person), the round being the commit
    ("a1b2c3d Tighten the abstract") and the time its date. What you wrote since
    stays: their suggestions are carried onto your text, and one on words you
    changed too shows as outdated, never as an undoing of yours.
  - Already committed on your branch (they pushed to main): Git ▸ Review
    Commits… lists the last commits touching the document; choose the first to
    review (to begin with, the first after your own last commit). Its changes
    and all after it become suggestions of their authors, and the document goes
    back to the text before them, so that accepting a suggestion brings its
    change back and rejecting it leaves it out. The document must have nothing
    uncommitted. A sheet says what happened ("Jane Doe's edits since a1b2c3d,
    back as suggestions"); commit when you like (Git ▸ Commit…).
  - Who made what comes from `git blame`, by line: two commits that changed the
    same line both count as the later one.
- Settings (⌘, in the app menu and the menu bar icon; one window, every change
  saved at once in `settings.json` of the app's data folder, every window told).
  Editor: font (system, serif, monospace) and size, what Tab inserts, readable
  line length, line numbers, spelling,
  the preview following the cursor, the symbols typed (the defaults each turned
  off, and your own). Review: your name and e-mail (empty: the git
  configuration's, else the system's; when set they win, each on its own) and
  your colour among the review's authors (or automatic, from the name). It is
  yours for your co-authors too: written as `colour = 1..6` in the `[person]`
  header of your review file (only the header is rewritten, when you change
  it), so that they see your suggestions in it. Build: the path of texmark, latexmk and
  pandoc (empty: found as usual, which the window shows as "Found: …"; a path
  that is not a file is an error where the tool is used). Window: files open as
  tabs (off: a window each, for the files opened from then on) and the app
  stays in the menu bar when its last window closes (off: it quits).

## Command line

`pw` opens files in the app and works on a document's review without it.

```bash
cargo install --path crates/cli     # puts `pw` in ~/.cargo/bin

pw paper.md refs.bib                # open in Papers Write (macOS: `open -a`, starting the app if needed)
pw review list paper.md [--json]    # open suggestions and comments: id, line:col, author, "old" → "new"
pw review accept paper.md --all     # or --author NAME, or --id ID...
pw review reject paper.md --author "Jane Doe"
pw review export paper.md --word paper.docx      # tracked changes and comments (pandoc)
pw review export paper.md --markdown paper.crit.md   # CriticMarkup: {++ ++} {-- --} {~~ ~> ~~} {>> <<}
pw review import paper.md reviewed.docx          # a Word review (--untracked: also changes made with tracking off)
pw review import paper.md back.review            # a review sent back, merged
```

Accept writes the suggestions into the file (from the end of the text) and appends the decisions to the review
under the user's name, as the app does (git `user.name` of the document's folder, else the system's full name, else
`$USER`). A suggestion whose text has changed since (outdated) or that overlaps another is skipped, said, and stays
open; Reject only records the decisions. Exit status 0 on success, 1 on an error, 2 on a command line that cannot be
understood.

## Develop

```bash
npm install
npm run tauri dev
```

Rust (stable) and Node.js.

## License

MIT.
