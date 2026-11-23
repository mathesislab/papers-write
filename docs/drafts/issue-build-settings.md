# Build profiles: what Build does, and which PDF goes with a source

Draft of an issue for mathesislab/papers-write (proposal). Not posted. Implemented as PR #16.

## Today

- Build (⇧⌘B) builds **the file in front** with a fixed command: `texmark name.md -d <build> --pdf` (latexmk for
  `.tex`, Quarto by `engine`).
- `write.toml` says `main` (used by the project bar only), `build` (the output folder) and `engine`.

That does not fit:

- **A book in chapters.** Building from a chapter builds the chapter alone, into the book's build folder.
- **Options a project needs.** `--copy-figures`, `--project-root .`, `--only …`, `--progressive`, `-o`.
- **Builds kept in a Makefile.**
- **A build running elsewhere.** `texmark --watch` in a terminal keeps the PDF up to date.
- **Which PDF goes with a source.** The PDF beside the text (#15) and source ↔ PDF navigation need to know.

## Proposal

**One build profile is active at a time.** A profile is a small text file in the project, committed with it:

```toml
# <project>/.papers-write/build/synopsis.toml
command = "make synopsis"   # any shell command, run in the project folder, e.g.
# command = "texmark thesis.md --copy-figures --project-root . --only introduction.md,discussion.md --pdf -o build/synopsis.pdf"
pdf = "build/synopsis.pdf"  # the PDF it makes: the default beside the text, where navigation looks
run = "build"               # "build": run by Build; "watch": started by the app and kept running (`make watch`),
                            # stopped on quit; "external": never run, the PDF only followed
```

- **Where the profiles live.** `.papers-write/` is already the app's folder (the review files are in
  `.papers-write/review/`), so profiles go next to them. This keeps `write.toml` short, and avoids its `build` key,
  which already names the output folder.
- **No profile: nothing changes.** Build builds the document in front, from its own front matter, as it does today.
- **Priority.** The document's front matter is the base, read by texmark. A profile only adds a command, whose
  arguments override the front matter as texmark's command line does. The same document builds the same in any
  editor.
- **Which profile is active** is the user's, kept in the app's state (as VS Code keeps the selected launch
  configuration), not in the repository:
  - It is set per source file and becomes the project's current one.
  - A file never set follows the project's current profile, so the chapters of a book follow one choice.
  - `write.toml` may name a default for a fresh checkout: `profile = "synopsis"` (not `build`, which already names the
    output folder).
  - Build ▸ Profile lists them, and the active one is named beside the Build button.
- **Build** does what the active profile says: run it, start its watch, or nothing.
- **The PDF beside a source**, in order:
  1. the user's choice for that file (#15);
  2. the active profile's `pdf`;
  3. `build/<name>.pdf`;
  4. the project's PDFs to choose from.

Not proposed, to keep it simple: several builds at once, settings inherited between profiles, a profile per source.

## Source ↔ PDF navigation

It needs none of these settings beyond which PDF. Ctrl-click in a PDF finds the source by its words (#8, #15). The
exact path through SyncTeX reads files a build leaves next to its PDF: `<name>.synctex.gz`, and texmark's
`<name>.srcmap.json` mapping the generated TeX back to the Markdown. That is texmark's contract
(`docs/synctex-design.md` there), so no editor needs configuring for it.
