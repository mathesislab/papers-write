// What lies over the content of a page for a while: the project bar slid in at the window's left edge (Frame, on
// hover). PDFKit's view of a PDF (PdfView), drawn above all of the page, keeps clear of it; pdf.js's, in the page, is
// covered as any element is.

/** The CSS pixels from the left of the window that the project bar covers (0: none). */
export const over = $state({ left: 0 });
