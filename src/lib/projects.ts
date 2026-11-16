// The projects, as the backend gives them (projects.rs).

import type { Colour, Tile } from "papers-write/project";

/** mirrors write_core::project::Manifest: what write.toml says */
export type Manifest = {
  name: string | null;
  short: string | null;
  colour: Colour | null;
  /** the main document(s), relative to the folder */
  main: string[];
  build: string | null;
  /** what builds the Markdown documents: texmark (none) or Quarto; a `.qmd` is always Quarto's */
  engine: "texmark" | "quarto" | null;
  library: string | null;
};

/** mirrors projects::Card */
export type Card = { root: string; defined: boolean; manifest: Manifest; documents: string[] };

/** mirrors projects::Bar: the project of the window's document (none: no bar), and the tiles */
export type Bar = { current: { root: string; name: string; colour: Colour } | null; tiles: Tile[] };
