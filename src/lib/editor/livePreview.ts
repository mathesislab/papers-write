// Live Preview (Settings ▸ Editor, View ▸ Live Preview): the text shown as the preview has it, but where the cursor
// is. Everything of it is in this module, as one extension the editor holds in a Compartment: off, the editor is as
// it was without it. In a LaTeX file it is the figures under their lines alone.

import type { Extension } from "@codemirror/state";
import { inlineFigures } from "./figures.ts";
import type { HoverSources } from "./types.ts";

/** What Live Preview takes from the app: the images of the figures (a data URI; null: not found), the renderings of
 *  the tooltips (math, citations), and whether the file is LaTeX. */
export type LivePreviewSources = {
  image: (path: string) => Promise<string | null>;
  hover?: HoverSources;
  latex?: boolean;
};

/** The extension of Live Preview. */
export function livePreview(sources: LivePreviewSources): Extension {
  return inlineFigures(sources.image);
}
