# Design (draft)

A desktop app to write papers in plain text: Markdown with LaTeX where
needed, cited from the papers library, built into a journal's format by
texmark, and reviewed by co-authors in a suggesting mode that leaves the
manuscript untouched until a change is accepted, and that travels through
git or as a file.

First of all a text editor: it opens any file at once, in as many windows
as wanted, with or without a project.

Status: the plan; the editor is the first part built.

## What exists

| Kind | Tools | Suggest / comment | Plain files, local, git |
|---|---|---|---|
| Word processors | Word, LibreOffice, Pages | yes, mature | no: binary, own format |
| Online, rich text | Google Docs | yes, the reference UX | no: online only |
| Online, LaTeX | Overleaf | track changes on paid plans (and the community fork *overleaf-cep*, self-hosted) | git bridge on paid plans; online |
| Online, other | Typst app, Authorea, Curvenote/MyST, HackMD | some comments; suggestions partial or paid | own formats, online |
| Self-hosted, academic | Fidus Writer (AGPL) | track changes, citations, LaTeX/JATS export | its own database, not files |
| Local Markdown | Zettlr, Obsidian, Typora, MarkText, iA Writer, Quarto / Positron | none of their own | yes |
| Local LaTeX | TeXstudio, TeXShop, VS Code + LaTeX Workshop / texlab | none | yes |
| Markup for changes | CriticMarkup (`{++ ++}` `{-- --}` `{~~ ~> ~~}` `{>> <<}`); LaTeX `changes` package | inline in the file | yes, but the file itself changes |
| Plugins on that markup | Obsidian: CriticMarkup (Fevol, beta), Inkling Markup; VS Code: Critique Markup | suggest mode writing CriticMarkup into the file | yes |
| Sidecar review files | MRSF (draft spec, 2025–26) with Sidemark for Obsidian and VS Code; markdown-review; mdreview | comments, and (Sidemark) replacements to accept or decline, in `paper.md.review.yaml` | yes; nothing git-specific |
| Diff of versions | latexdiff, `git diff --word-diff`, GitHub "suggested changes" in a PR | after the fact; GitHub's are line-based and online | yes |
| Research | Ink & Switch Patchwork (Automerge: branches, edit groups, diff views for prose) | the closest to the idea | a prototype, not a tool |

What no tool does: a local, plain-file editor for academic Markdown with the
library's citations and a journal build, whose suggestions and comments are
files of their own in the repository. The nearest kit is VS Code with
Sidemark, LaTeX Workshop and a citation extension: most of the parts, none of
them made for each other, nothing for the library or texmark, and too much
for co-authors who are not programmers.

What to take rather than make:

- The editor: CodeMirror 6, as papers-review.
- The conversion: pandoc and texmark (already the pipeline). Pandoc also
  writes Word documents with tracked changes and comments and reads them
  back (`--track-changes=all`; checked with pandoc 3.9).
- The library: papers-core / papers-rs, as Papers Desktop and papers-review.
- The anchoring of comments: the W3C text-quote selector (prefix, exact,
  suffix), as Hypothesis and MRSF use it.
- The PDF: tauri-plugin-pdfkit. latexdiff and latexmk from the TeX
  installed.

## Principles

- A text editor first: any file opens at once, alone, in its own window.
  A project adds to it; it is never required.
- The text is edited as text. Previews show the result; nothing is edited in
  a rendered view.
- Plain files, no database. A project is a folder, usually a git repository.
- Suggestions and comments never touch the manuscript. It changes only when
  someone edits it directly or accepts a suggestion.
- Every file the app writes merges in git without conflicts in normal use,
  and reads in a diff.
- Co-authors without git, or without the app, are not shut out.
- Tauri 2 and SvelteKit, macOS first, as the rest of the family.

## The family: what is shared

### The editor, in papers-write

The editor lives in papers-write, and papers-review takes it from there at a
tag (npm and Cargo git dependencies), as papers-review took papers-core from
papers-desktop. It moves to a repository of its own only if a third app
needs it, as papers-rs did.

- `src/lib/editor/`, exported as `papers-write/editor`: the CodeMirror setup
  (fonts, readable width, word count, reload keeping cursor and undo), the
  Markdown + LaTeX language, the symbols and their list, the file editor
  (load, save, changed on disk, Reload or Keep mine), the outline, the
  completion with its sources given by the app (citations, labels, paths),
  the reference list, and the review overlay (suggest mode, comments). What
  is one app's own (the page and line of a quote in papers-review, the
  commands that search the references) is passed in by that app.
- A crate of papers-write's workspace (later): files written atomically and
  watched, the references (papers-core: the project's `.bib`, the papers
  library), and the review files (read, written, anchored, accepted).
- The project bar and info card, once papers-write has projects: the
  machinery shared, each app giving its tile and its card (below).

The editor came out of papers-review with its history.

### One app or two

papers-review and papers-write as one app:

- For: one install, one updater, one settings window; one project model, one
  project bar; reviewing a manuscript and writing one's reply side by side;
  no code kept twice.
- Against: they are centred on different things (the PDF fills
  papers-review, with the review over it; the text fills papers-write, with
  the preview beside it), and every view of each would carry the other's
  menus and commands. A quick plain editor is harder to keep quick inside a
  larger app. Their project files say different things (`review.toml`:
  journal, due date, manuscript; `write.toml`: build, bib, journal
  template). Two simple apps are easier to explain than one with modes.

Chosen: two apps, everything below the window shared, from papers-write.
One app later is then a matter of windows, not code.

## The app

- Opens any text file: Open With and the Dock, `⌘O`, `⌘N` for a blank one,
  a command (`pw paper.md`, not yet), files dropped. A new window is untitled
  text kept in the window, saved with `⌘S` (a file is saved as typed).
  Files open as macOS window tabs;
  a tab dragged out is a window of its own, and as many windows as wanted.
  Opens in well under a second: the library, pandoc and git are loaded when
  first needed. It stays in the menu bar when the last window closes, as
  Papers Desktop does, so the next window is there at once.
- A file alone already has most of it: its front matter says the
  bibliography and the journal, the papers library is found as papers-review
  finds it, the preview and the build work.
- A project (File ▸ Open Project, a folder) adds the project bar, as in
  papers-review and with its machinery and look: a tile per project in its
  colour with its short name, the files of its folder, the build, the review
  of its files. A file opened from inside a project's folder opens in it.
- What differs from papers-review is what a project says. No due date: the
  tile shows the words, the suggestions waiting, and whether the last build
  passed. The info card asks for the main file, the bib, the journal
  template and the build folder, not a manuscript ID.

## The project

```
ice-sheet-paper/              a git repository, or any folder
├── write.toml                what the files do not say
├── paper.md                  the manuscript, texmark's yaml front matter
├── references.bib            grows as keys are cited
├── figures/
├── build/                    texmark's output (ignored by git)
└── .papers-write/            the review (below)
```

texmark's front matter stays the truth for what it reads (`journal`,
`template`, `bibliography`, `figure_folders`, `copy_figures`, …).
`write.toml` holds the rest: name, short name and colour, the main file(s),
the build folder, the library searched. A folder without it opens with a
card asking for these, and offers to start from the paper-draft skeleton.

## The editor

- From papers-editor (fonts, readable width, symbols, word count, reload).
- Highlighting, as Lezer extensions of the Markdown parser: the yaml front
  matter; math inline and display (`$…$`, `$$…$$`, `\begin{equation}`…),
  highlighted as LaTeX inside; raw LaTeX commands; pandoc citations
  (`[@key]`, `@key`, `[-@key; @key2, p. 3]`) and `\cite{}`; cross-references
  (`@fig:x`, `\ref{}`, `{#fig:x}`); fenced divs and attributes; CriticMarkup
  if present.
- Completion (`Tab` / as typed): citation keys after `[@`, `@`, `\cite{`,
  from the bib first, then the library (marked "adds to references.bib");
  labels after `@fig:`, `\ref{`, `\eqref{`; figure paths; LaTeX commands and
  environments, with snippets for equations, figures, tables; the symbols;
  front-matter fields and texmark's journals.
- The bib grows: citing a library key copies its entry into the bib of the
  front matter. A check lists keys cited but missing (offered from the
  library), and entries never cited (offered for removal).
- Hover: an equation rendered (MathJax), a figure's thumbnail, a
  reference's citation, a label's target.
- Outline of the sections; diagnostics for undefined labels and keys;
  spelling from the system. Grammar (Harper, a local Rust checker) later.
- `.tex` files: a LaTeX mode, the same completion, built with latexmk; the
  texlab language server later.

## Previews and builds

- Quick preview, beside the text: pandoc to HTML with `--citeproc` and
  MathML (WebKit renders it natively, so no MathJax and no network), the
  parser texmark uses, so what shows is what the build reads. One pandoc run
  per window at a time (a newer request kills the old), images embedded as
  data URIs (under 5 MB; no asset protocol or CSP to scope), in a sandboxed
  frame without scripts. Cross-references stay as pandoc leaves them.
  A 9,400-word paper with 60 display equations takes 0.22 s, fast enough to
  run a moment after the typing stops. It follows the cursor.
- Build: texmark with `--pdf` into `build/` beside the document (⌘B; ⇧⌘B
  shows the PDF in a tab, PDFKit, which reloads by itself when the file
  changes), one build per document at a time, the status only told to the
  page. Failures list the `!` errors of `build/name.log` with their `.tex`
  lines. Not yet: rebuilt on save; the errors mapped back to the Markdown's
  lines.
- Marked-up PDF: two versions each converted by texmark to LaTeX, then
  latexdiff and latexmk: the marked-up manuscript journals ask for with a
  revision. Built: a tag or one of the last 30 commits of the file, or
  another file, against the text now (`build/diff/old` the earlier text as
  LaTeX only, `build/diff/new` the current one built, as texmark copies the
  journal's class files and the bibliography only when it compiles, and there
  `diff.tex` is made and compiled). Not yet: a base and its suggestions
  accepted, two commits.
- TeX: the one installed (MacTeX / TeX Live), through latexmk; the app says
  so when there is none. Not tectonic: latexdiff and the journals' classes
  need a full TeX anyway, and texmark already calls the installed engine.

## Suggesting and comments

### Choices weighed

1. CriticMarkup in the file: plain text, readable everywhere. But the
   manuscript itself changes with every suggestion, the build must strip
   it, and it fights with math and LaTeX. Kept as an export.
2. A whole copy of the document per reviewer (or a git branch): the
   reviewer edits freely, and the copy is compared with the manuscript.
   But each copy drifts from the manuscript as the author works, so every
   comparison mixes the reviewer's changes with the author's own; a
   rejection has nowhere to live; and comments need anchors anyway. Kept as
   a way in: a copy or branch can be turned into suggestions.
3. References into a version (chosen): each suggestion or comment says
   which version of the text it was made on and where in it, and that
   version is kept. The manuscript is untouched, and a suggestion can be
   carried onto any later version exactly.

### The review files

```
.papers-write/review/paper.md/
├── bases/
│   ├── 3f2a91c….md       each version of paper.md that was reviewed,
│   └── 8b04e7d….md       named by its git blob hash
├── jane-doe.toml         Jane's suggestions, comments, replies, decisions
└── alex-robinson.toml
```

- A base is written when the first suggestion or comment is made on that
  version. In git it costs nothing when that version was committed: git
  stores the same content once, whatever its name. Without git it is what
  lets the review stand alone.
- One file per person, which only that person's app writes and only by
  appending. Two people reviewing at once never touch the same file, so git
  merges them without conflict, and a file sent back by e-mail is simply
  added. The state of the review is all the files read together.
- Not a folder per round: a person's suggestions may be made on several
  versions in turn (they pull a new one halfway), and each record says its
  own base. A round is a label on the records, for showing, accepting and
  sending one round.
- Resolved records and the bases no record needs any more can be cleared
  (Clean Up Review); git history keeps them.

```toml
[person]
name = "Jane Doe"
email = "jane@example.org"

[[record]]
id = "jane-doe-7"
kind = "suggestion"        # suggestion | comment | reply | decision
time = "2026-11-05T14:02:11Z"
round = "second draft"
base = "3f2a91c…"
start = 10412              # UTF-8 byte offsets into the base
end = 10431
prefix = "the grounding line "
exact = "retreats rapidly"
suffix = " when the bed slopes"
insert = "retreats irreversibly"
text = "Stronger, and what Fig. 3 shows."

[[record]]
id = "jane-doe-8"
kind = "comment"
time = "2026-11-05T14:05:40Z"
base = "3f2a91c…"
start = 11020
end = 11076
prefix = "…"
exact = "…"
suffix = "…"
text = "Cite Schoof (2007) here?"

# in alex-robinson.toml
[[record]]
id = "alex-robinson-12"
kind = "decision"
time = "2026-11-06T09:00:00Z"
on = "jane-doe-7"
decision = "accepted"      # accepted | rejected | resolved | reopened | withdrawn
```

A reply has `on` (the record answered) and `text`. A counter-suggestion is a
suggestion with `on` (the suggestion it answers) and its own base. A
suggestion edited by its author is a new record with `supersedes`; the latest
one counts. TOML, because a record is appended as one `[[record]]` table at
the end of the file, which needs no rewriting of what is there. The hash is
git's blob hash of the text (`git hash-object`). The quote (`prefix`,
`exact`, `suffix`, up to 32 characters of context) is kept with the offsets.
The app talks in UTF-16 code units of the current text (what CodeMirror
counts); the core converts.

The format is the app's own, documented, with MRSF's field names where they
fit and an MRSF export. MRSF itself does not fit: one file per document,
which conflicts in git when two people append, anchors by line and column,
and no version or decisions.

### A new version

The manuscript changes under a review: the author goes on writing, or a
reviewer pulls a newer version halfway. Each record is carried from its base
to the current text through the diff between the two (word level), so it
lands where its text now is:

- its text unchanged: shown in place;
- the text around it changed: moved with it, nothing to do;
- its own text changed: the suggestion is merged three ways (base, current
  text, base with the suggestion). If the two changes do not overlap it
  still applies; if they do, it is shown as outdated, with what it said and
  what is there now, to apply by hand, re-anchor or drop. A comment on
  changed text stays, marked as such.

Without a base (an imported review whose version is lost) the quote is
searched for, fuzzily.

None of this needs the review to go one way. Several people review the same
version at once, each in their own file, and all of it shows together. The
author accepts some, which makes a new version; someone reviews that one
while their earlier suggestions are carried onto it; two people go back and
forth on a passage with counter-suggestions. Two suggestions on the same
text both show; accepting one leaves the other outdated, to be dealt with.

### In the editor

- Edit mode: typing changes the file.
- Suggest mode: typing changes only the reviewer's file. The editor shows
  the current text with the suggestions laid over it: insertions coloured
  by person, deletions struck through, as Word's All Markup. Typing runs
  together into one suggestion until the cursor leaves it, as in Google
  Docs.
- A suggestion can answer another: a counter-suggestion, in the thread of
  the first, made on the text as the first would leave it (its base is the
  first suggestion). Accepting it accepts both; rejecting the first rejects
  it too.
- Comments: on a selection, in the margin, with replies; resolved ones
  hidden.
- Accept or reject one, a selection, all of a person or a round, all.
  Accepting writes the file and appends the decision; with git the app
  offers a commit ("Accept 12 suggestions of Jane Doe").
- Views: All Markup, Accepted (as if all were accepted), Original.
- Tools can write a review file too: an agent (Claude) suggests in its own
  file instead of editing the manuscript.

### Sending a review, without git

- To someone with the app: Send for Review… makes one file,
  `paper — second draft.review` (a zip: the base, the figures it shows, the
  bib, the review files of the round). It opens in the app as the document with its
  changes; they suggest and comment, and Send Back makes their own
  `.review` holding only their file. Dropped on the author's window, it
  joins the review folder.
- To someone with Word: Export as Word with Changes writes a `.docx` with
  real tracked changes and comments (pandoc; the math as Word equations).
  They edit with Track Changes on and send it back; Import Word Review reads
  their changes and comments as theirs, anchored on the text around them.
  Edits made with tracking off are found by comparing the text with the
  base, and come in too. Only the text comes back: formatting Word adds is
  dropped, and what pandoc cannot carry is listed.
- To read only: a single HTML page (the preview, changes coloured, comments
  in the margin), or the marked-up PDF (latexdiff, comments as margin
  notes).
- From anyone: an edited copy of the file, a commit or a branch: Compare as
  Suggestions… turns the word-level diff from the base into suggestions
  under their name.

### Git

The app shows the repository's state and offers commits, pull and push for
the files it writes; the rest is left to git. The history of a file is
shown as tracked changes between two commits. Nothing is rewritten.

## Code

- The editor (TypeScript, Svelte), exported for papers-review; a crate for
  files, references, review files, anchoring (the diff with `similar` or
  `imara-diff`).
- The app (Tauri): windows, projects, previews and builds (pandoc, texmark,
  latexdiff), git (the `git` command), import and export. A small CLI on the
  review crate (`pw review list|accept|export`).

## Steps

1. The editor out of papers-review, with its history, into papers-write;
   papers-review on it.
2. papers-write: the plain editor (windows, files, highlighting, completion,
   the growing bib).
3. Previews and builds: the quick preview, texmark, the PDF tab, latexdiff.
4. Comments: the review files, bases and anchoring, on the simpler case.
5. Suggest mode: suggestions, accept and reject, the views, new versions.
6. Sending: `.review` files, Word, HTML and PDF; Compare as Suggestions.
7. Projects and git: the project bar, commits, history as changes.
8. LaTeX files.
