// File ▸ Print…: what the tab shows, through the system's print dialog (`print_page`: Tauri's print of the page).
// The page's own content is hidden for the printer and a sheet of its own printed instead: a text's preview (its HTML,
// on paper colours), the text itself where it has none (LaTeX), or a PDF's pages drawn by pdf.js.

import { invoke } from "@tauri-apps/api/core";
import { GlobalWorkerOptions, getDocument } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

GlobalWorkerOptions.workerSrc = workerUrl;

const ID = "papers-print";

/** The preview's look on paper: black on white, the width of the page. */
const PAPER = `
  @media screen { #${ID} { display: none; } }
  @media print {
    body > *:not(#${ID}) { display: none !important; }
    html, body { background: #fff !important; height: auto !important; overflow: visible !important; }
    #${ID} { display: block; color: #000; font: 11pt/1.45 "Iowan Old Style", "Palatino", "Charter", Georgia, serif; hyphens: auto; }
    #${ID} h1, #${ID} h2, #${ID} h3, #${ID} h4 { line-height: 1.25; margin: 1.4em 0 .5em; break-after: avoid; }
    #${ID} h1 { font-size: 1.6em; } #${ID} h2 { font-size: 1.3em; } #${ID} h3 { font-size: 1.1em; } #${ID} h4 { font-size: 1em; font-style: italic; }
    #${ID} p { margin: 0 0 .8em; text-align: justify; orphans: 3; widows: 3; }
    #${ID} a { color: inherit; text-decoration: none; }
    #${ID} code, #${ID} pre { font: .85em ui-monospace, Menlo, monospace; }
    #${ID} pre { white-space: pre-wrap; }
    #${ID} blockquote { margin: 1em 0; padding-left: 1em; border-left: 2px solid #888; }
    #${ID} figure { margin: 1.2em 0; text-align: center; break-inside: avoid; }
    #${ID} img { max-width: 100%; height: auto; }
    #${ID} figcaption, #${ID} caption { font-size: .85em; margin-top: .4em; text-align: left; }
    #${ID} table { border-collapse: collapse; margin: 1em auto; break-inside: avoid; }
    #${ID} th, #${ID} td { border-bottom: 1px solid #888; padding: .2em .6em; }
    #${ID} math[display="block"] { display: block; margin: 1em 0; text-align: center; }
    #${ID} .pdf-page { display: block; width: 100%; break-after: page; }
    #${ID} .pdf-page:last-child { break-after: auto; }
  }
`;

/** The sheet to print, in place of the one printed before. */
function sheet(): HTMLElement {
  document.getElementById(ID)?.remove();
  document.getElementById(`${ID}-style`)?.remove();
  const style = document.createElement("style");
  style.id = `${ID}-style`;
  style.textContent = PAPER;
  document.head.append(style);
  const root = document.createElement("div");
  root.id = ID;
  document.body.append(root);
  return root;
}

/** Its images loaded (a figure printed blank otherwise). */
async function loaded(root: HTMLElement) {
  await Promise.all(
    [...root.querySelectorAll("img")].map((img) => (img.complete ? null : new Promise((done) => ((img.onload = done), (img.onerror = done))))),
  );
}

/** A text's preview (its HTML) printed. */
export async function printHtml(html: string) {
  const root = sheet();
  root.innerHTML = html;
  await loaded(root);
  await invoke("print_page");
}

/** A text with no preview (LaTeX) printed as it is. */
export async function printText(text: string) {
  const root = sheet();
  const pre = document.createElement("pre");
  pre.textContent = text;
  root.append(pre);
  await invoke("print_page");
}

/** The PDF at `path` printed: the file itself (outside macOS), else its pages drawn at 2× (about 150 dpi on A4), one
 *  per sheet. */
export async function printPdf(path: string) {
  // outside macOS: the file itself sent to the printer from GTK's dialog (printpdf.rs), its text kept; pictures of
  // its pages, as below, only for a printer that takes no PDF
  const printed = await invoke<"sent" | "cancelled" | "unsupported">("print_pdf", { path });
  if (printed !== "unsupported") return;
  const data = await invoke<ArrayBuffer>("read_pdf", { path });
  const task = getDocument({ data: new Uint8Array(data) });
  const pdf = await task.promise;
  const root = sheet();
  try {
    for (let n = 1; n <= pdf.numPages; n++) {
      const page = await pdf.getPage(n);
      const viewport = page.getViewport({ scale: 2 });
      const canvas = document.createElement("canvas");
      [canvas.width, canvas.height] = [viewport.width, viewport.height];
      await page.render({ canvas, canvasContext: canvas.getContext("2d")!, viewport }).promise;
      const blob = await new Promise<Blob | null>((done) => canvas.toBlob(done, "image/png"));
      const img = document.createElement("img");
      img.className = "pdf-page";
      if (blob) img.src = URL.createObjectURL(blob);
      root.append(img);
    }
  } finally {
    await task.destroy();
  }
  await loaded(root);
  await invoke("print_page");
}
