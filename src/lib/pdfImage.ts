// A PDF figure as an image the page can show: its first page drawn by pdf.js into a PNG. WebKit draws a PDF given to
// an <img> on macOS but not elsewhere (WebKitGTK), so the figures of a document (`![…](figure.pdf)`) are drawn here
// for the tooltips and the preview. Kept by their data, so that a figure drawn once is not drawn again.

import { GlobalWorkerOptions, getDocument } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

GlobalWorkerOptions.workerSrc = workerUrl;

/** How wide the PNG is drawn (px): sharp enough for a tooltip or a column of text on a dense screen. */
const WIDTH = 1200;
/** The figures drawn, by a key of their data; the oldest go past a few dozen. */
const drawn = new Map<string, Promise<string | null>>();
const KEEP = 48;

const isPdf = (uri: string) => uri.startsWith("data:application/pdf");

/** A key of a data URI that is cheap to make and unlikely to be shared: its length and a few slices of it. */
function keyOf(uri: string): string {
  const n = uri.length;
  return `${n}:${uri.slice(40, 120)}:${uri.slice(n / 2, n / 2 + 80)}:${uri.slice(-80)}`;
}

async function draw(uri: string): Promise<string | null> {
  try {
    const data = Uint8Array.from(atob(uri.slice(uri.indexOf(",") + 1)), (c) => c.charCodeAt(0));
    const task = getDocument({ data });
    try {
      const doc = await task.promise;
      const page = await doc.getPage(1);
      const base = page.getViewport({ scale: 1 });
      const viewport = page.getViewport({ scale: Math.min(4, WIDTH / base.width) });
      const canvas = document.createElement("canvas");
      canvas.width = Math.ceil(viewport.width);
      canvas.height = Math.ceil(viewport.height);
      const context = canvas.getContext("2d");
      if (!context) return null;
      // on white, as on paper: a figure's background is often left transparent
      context.fillStyle = "#fff";
      context.fillRect(0, 0, canvas.width, canvas.height);
      await page.render({ canvas, canvasContext: context, viewport }).promise;
      return canvas.toDataURL("image/png");
    } finally {
      void task.destroy();
    }
  } catch {
    return null;
  }
}

/** The image to show for a data URI: a PDF's first page as a PNG, anything else as it is. */
export function shownImage(uri: string): Promise<string | null> {
  if (!isPdf(uri)) return Promise.resolve(uri);
  const key = keyOf(uri);
  let png = drawn.get(key);
  if (!png) {
    png = draw(uri);
    drawn.set(key, png);
    if (drawn.size > KEEP) drawn.delete(drawn.keys().next().value!);
  }
  return png;
}

/** The PDF figures of a document shown as images: each <img> of `root` whose source is a PDF given the PNG of its
 *  first page (the preview's frame, which pandoc's HTML embeds them in). */
export async function showPdfImages(root: ParentNode) {
  const images = [...root.querySelectorAll<HTMLImageElement>("img")].filter((img) => isPdf(img.getAttribute("src") ?? ""));
  await Promise.all(
    images.map(async (img) => {
      const png = await shownImage(img.getAttribute("src")!);
      if (png) img.src = png;
    }),
  );
}
