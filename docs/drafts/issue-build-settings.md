# Build settings: what Build builds, with which options (discussion)

Draft of an issue for mathesislab/papers-write. Not posted.

## Today

- Build (⇧⌘B) builds **the file in front**, not the project's `main`.
- The texmark command is fixed: `texmark name.md -d <build> --pdf`.
- `write.toml` says `main` (used by the project bar only), `build` (the output folder) and `engine` (texmark or Quarto, for Markdown).

## What does not fit

- **A book in chapters.** Building from a chapter builds that chapter alone, into the folder the whole book is built in.
- **Options a project needs.** For example `--copy-figures`, `--project-root .`, `--only a.md,b.md`, `--progressive` and journal templates. These live in a Makefile or a terminal today.
- **A build that runs elsewhere.** `texmark --watch` in a terminal already keeps the PDF up to date. Papers Write follows the PDF (it reloads on change), but its Build button would build into the same folder alongside the watch.
- **Pure-LaTeX projects.** They have no setting beyond the file in front (latexmk).

## Options

1. **More top-level keys** (the smallest change, backwards compatible):
   ```toml
   main = "thesis.md"          # also what Build builds, when set
   engine = "texmark"          # adds "latex"
   args = ["--copy-figures", "--project-root", "."]
   pdf = "build/synopsis.pdf"  # the PDF that goes with main (default build/<main>.pdf)
   ```
2. **A `[build]` table.** This is tidier, but `build` is already the output folder's key, so the old keys would need reading for a while.
3. **Build Main Document** as a separate menu item: Build keeps building the file in front.
4. **`watch = "external"`**: Build does nothing but follow the PDF, for projects built by a watch process.

## Source ↔ PDF navigation

That does not need any of these settings. Ctrl-click in a PDF finds the source by its words (#8). The exact path through SyncTeX reads files that the build leaves next to the PDF (`<name>.synctex.gz`, and texmark's `<name>.srcmap.json`). That contract is texmark's (`docs/synctex-design.md` there), so editors need no configuration.
