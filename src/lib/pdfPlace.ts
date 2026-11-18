// A place of a source shown in its PDF (Show in PDF) by a page that is not the text's: the PDF's own tab, asked by the
// text with the panes in tabs (panetabs.rs). The lines the source line made, from the map the build left beside the
// PDF (texmark's, or SyncTeX's), the one as far through them as the cursor is through the source line, marked with the
// word by the view (`PdfViewer.show`), as SidePdf does beside the text.

import { invoke } from "@tauri-apps/api/core";
import type { Box, PdfViewer } from "$lib/pdf";

/** The place shown in `view`; its page, or none (no map beside the PDF, or nothing there), and what was done. */
export async function showSourceIn(
  view: PdfViewer,
  pdf: string,
  source: string,
  line: number,
  word = "",
  along = 0,
  before: string[] = [],
  after: string[] = [],
): Promise<{ page: number | null; status: string }> {
  let error = "";
  const boxes = await invoke<Box[] | null>("pdf_boxes", { pdf, source, line }).catch((e) => ((error = String(e)), null));
  if (!boxes?.length) return { page: null, status: error || "No map from the build beside the PDF: by the words" };
  const lines = [...boxes].sort((a, b) => a.page - b.page || a.y - b.y || a.x - b.x);
  const box = lines[Math.min(lines.length - 1, Math.max(0, Math.round(along * (lines.length - 1))))];
  // the paragraph's band on each page, from its first line (one more above: the map can miss it) to its last
  const regions = [...new Set(lines.map((l) => l.page))].map((page) => {
    const own = lines.filter((l) => l.page === page);
    const height = Math.max(...own.map((l) => l.height));
    return { page, top: Math.min(...own.map((l) => l.y)) - height * 2.6, bottom: Math.max(...own.map((l) => l.y)) + height * 0.4 };
  });
  const how = await view.show({ box, regions, word, before, after });
  return { page: box.page, status: `→ page ${box.page}: ${how}` };
}
