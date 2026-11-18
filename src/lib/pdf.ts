// What the two views of a PDF share: PdfView (PDFKit, macOS) and PdfjsView (pdf.js elsewhere) take the same props and
// answer the same commands, so that a page holds either as a `PdfViewer`; and they read a click and pick the place
// Show in PDF marks the same way, from a page's text and a place in it (the text layer's, pdf.js; PDFKit's string).

import type { History, Turn } from "tauri-plugin-pdfkit-api";

/** A place of a PDF: its page (from 1) and its point, in PDF points from the page's top left, as SyncTeX has it. */
export type At = { page: number; x: number; y: number };

/** A click on the PDF's text (⌘-click, Go to Source), for its source to be found: the line clicked (with the next when
 *  it is short), its place, the word and the six words before and after it. */
export type Spot = { line: string; at: At | null; word: string; before: string[]; after: string[] };

/** An item of the PDF's outline (its bookmarks), in order: its depth, its title and the page it goes to (null: not
 *  known). */
export type OutlineItem = { level: number; title: string; page: number | null };

/** A line of the PDF as a build's map gives it, in PDF points from the page's top left. */
export type Box = { page: number; x: number; y: number; width: number; height: number };

/** A band of a page, in PDF points from its top. */
export type Region = { page: number; top: number; bottom: number };

/** What Show in PDF marks: the line the map gives (`box`), the bands of the paragraph it is in (`regions`), and the
 *  word with the source's words around it, which pick its place among the word's places there. */
export type Target = { box: Box; regions: Region[]; word: string; before: string[]; after: string[] };

/** What the pages ask of a view of a PDF. */
export interface PdfViewer {
  /** in (1), out (-1), the width of the area (0) */
  zoom(step: number): void;
  /** the keys to the PDF */
  focus(): void;
  /** back to the page the last link or jump left, and forward again */
  back(): void;
  forward(): void;
  pageTo(page: number): void;
  turn(to: Turn): void;
  /** `text` found: the first place, or `again` the next (the one before, `backwards`); an empty text clears */
  find(text: string, again?: boolean, backwards?: boolean): Promise<boolean>;
  /** to the item `i` of the outline told (`onoutline`) */
  goToOutline(i: number): void;
  /** Show in PDF: the place scrolled to and marked, till the next click; what was marked, said */
  show(target: Target): Promise<string>;
}

/** The props of a view of a PDF. */
export type PdfViewProps = {
  path: string;
  /** the page to open at (from 1) */
  page?: number;
  /** the page is not in front (a tab behind): PDFKit's view, above all of the page, hides; it hides too while a card is
   *  shown over the page (`over`, overlay.svelte.ts) */
  covered?: boolean;
  /** the page in view */
  onpage: (page: number) => void;
  /** the PDF open: its page count */
  onopen: (count: number) => void;
  /** whether there is a page to go back to (the one a link or a jump left) or forward */
  onhistory?: (history: History) => void;
  /** the PDF's outline, once open */
  onoutline?: (items: OutlineItem[]) => void;
  /** a ⌘-click (Ctrl elsewhere) on the text, or Go to Source in its menu */
  onsource?: (spot: Spot) => void;
  /** an item of the page's own in the right-click menu, after Go to Source */
  menuItem?: { text: string; action: () => void } | null;
};

/** The item of the outline the page `page` is under: the last that begins on it or before it; -1 if none. */
export function sectionAt(items: OutlineItem[], page: number): number {
  let at = -1;
  items.forEach((o, i) => {
    if (o.page !== null && o.page <= page) at = i;
  });
  return at;
}

/** The words of a text as they compare: lower case, letters and digits, of two characters or more. */
export const wordsOf = (text: string): string[] =>
  text
    .toLowerCase()
    .split(/[^\p{L}\p{N}]+/u)
    .filter((w) => w.length > 1);

const inWord = (c: string | undefined) => !!c && /[\p{L}\p{N}_-]/u.test(c);

/** The word of `text` at `at` (a place in it, or the place just after it) and the six words before and after it. */
export function wordAround(text: string, at: number): { word: string; before: string[]; after: string[] } {
  let [from, to] = [at, at];
  while (from > 0 && inWord(text[from - 1])) from--;
  while (to < text.length && inWord(text[to])) to++;
  return {
    word: text.slice(from, to),
    before: wordsOf(text.slice(Math.max(0, from - 160), from)).slice(-6),
    after: wordsOf(text.slice(to, to + 160)).slice(0, 6),
  };
}

/** A line clicked, with the next one when it has fewer than six words: enough of them to find its source. */
export function withNext(line: string, next: string): string {
  const own = line.replace(/\s+/g, " ").trim();
  const more = next.replace(/\s+/g, " ").trim();
  return own.split(" ").length < 6 && more ? `${own}\n${more}` : own;
}

/** The line of a page's text from `from` to `to`, with the next (`withNext`): the text up to the next line break. */
export function lineOfText(text: string, from: number, to: number): string {
  const next = text.slice(to).replace(/^[\r\n]+/, "").split(/[\r\n]/)[0] ?? "";
  return withNext(text.slice(from, to), next);
}

/** Text of a PDF as it is read aloud: a word hyphenated at a line's end joined again, one space between words. */
export const spoken = (text: string): string =>
  text
    .replace(/(\p{L})-\r?\n(\p{Ll})/gu, "$1$2")
    .replace(/\s+/g, " ")
    .trim();

/** A place of a word in a page's text: from and to, how many of the source's words around it are around it here
 *  (`score`), and how far it is from the place the map gives (`distance`). */
export type Found = { from: number; to: number; score: number; distance: number };

/** The place of `word` (whole, in any case) in `text` whose neighbours are most like the source's (`before`, `after`),
 *  the nearest (`distance` of its from and to) among equals; null where the word is not. */
export function bestPlace(text: string, word: string, before: string[], after: string[], distance: (from: number, to: number) => number): Found | null {
  if (!word) return null;
  const pattern = new RegExp(`(?<![\\p{L}\\p{N}])${word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![\\p{L}\\p{N}])`, "giu");
  const [wanted, following] = [new Set(before.map((w) => w.toLowerCase())), new Set(after.map((w) => w.toLowerCase()))];
  let best: Found | null = null;
  for (const found of text.matchAll(pattern)) {
    const from = found.index ?? 0;
    const to = from + found[0].length;
    const score =
      wordsOf(text.slice(Math.max(0, from - 90), from)).slice(-6).filter((w) => wanted.has(w)).length +
      wordsOf(text.slice(to, to + 90)).slice(0, 6).filter((w) => following.has(w)).length;
    best = better(best, { from, to, score, distance: distance(from, to) });
  }
  return best;
}

/** The better of two places: the higher score, the nearer among equals; the first among equals. */
export function better<T extends Found>(a: T | null, b: T | null): T | null {
  if (!a || !b) return a ?? b;
  return b.score > a.score || (b.score === a.score && b.distance < a.distance) ? b : a;
}
