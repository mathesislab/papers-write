// What lies over the content of a page for a while: the project bar slid in at the window's left edge (Frame, on
// hover), and the cards shown over the page (a sheet, a list to choose from, the project's card). PDFKit's view of a
// PDF (PdfView), drawn above all of the page, keeps clear of the bar and hides while a card is shown; pdf.js's, in the
// page, is covered as any element is.

import { untrack } from "svelte";
import type { Attachment } from "svelte/attachments";

/** The CSS pixels from the left of the window that the project bar covers (0: none), and how many cards are shown. */
export const over = $state({ left: 0, cards: 0 });

/** A card over the page while its element is there (`{@attach overPage}`). */
export const overPage: Attachment = () => {
  untrack(() => over.cards++);
  return () => {
    untrack(() => over.cards--);
  };
};
