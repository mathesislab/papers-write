<script lang="ts">
  // A PDF by pdf.js, where PDFKit is not (Linux), as Papers Desktop shows them there: the props and the commands of
  // PdfView (zoom, back and forward, a page, a turn, find), in the page itself, so a card over it covers it as any
  // element. Back and Forward are the pages the links and the jumps (Go to Page, first, last) left. A ⌃-click on the
  // text gives the line clicked (`onsource`), for the page to open its source there.

  import { popupReleased } from "$lib/contextMenu";
  import { invoke } from "@tauri-apps/api/core";
  import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  import { GlobalWorkerOptions, getDocument, type PDFDocumentLoadingTask } from "pdfjs-dist";
  import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
  import { EventBus, PDFFindController, PDFLinkService, PDFViewer } from "pdfjs-dist/web/pdf_viewer.mjs";
  import "pdfjs-dist/web/pdf_viewer.css";
  import { onMount } from "svelte";
  import type { History, Turn } from "tauri-plugin-pdfkit-api";

  GlobalWorkerOptions.workerSrc = workerUrl;

  let {
    path,
    page = 1,
    onpage,
    onhistory,
    onopen,
    onsource,
    menuItem = null,
    onoutline,
  }: {
    path: string;
    /** the page to open at (from 1) */
    page?: number;
    /** the page shows a card over it (the pdf.js view is part of the page: nothing to do) */
    covered?: boolean;
    /** the page in view */
    onpage: (page: number) => void;
    /** whether there is a page to go back to (the one a link or a jump left) or forward */
    onhistory?: (history: History) => void;
    /** the PDF open: its page count */
    onopen: (count: number) => void;
    /** a ⌃-click (⌘ on macOS) on the text: the line of text clicked (and the next when it is short), and the place
     *  clicked: its page and its point in PDF points from the page's top left, as SyncTeX takes it */
    onsource?: (text: string, at: { page: number; x: number; y: number } | null, word: string, around: { before: string[]; after: string[] }) => void;
    /** an item of the page's own in the menu, after Go to Source (the pane: Show Markdown Preview) */
    menuItem?: { text: string; action: () => void } | null;
    /** the PDF's outline (its bookmarks), in order, each with its depth and the page it goes to (none: unknown) */
    onoutline?: (items: { level: number; title: string; page: number | null }[]) => void;
  } = $props();

  let area: HTMLDivElement;
  let container: HTMLDivElement;
  let viewer: PDFViewer | undefined;
  let bus: EventBus | undefined;
  let count = 0;
  let error = $state("");
  /** the pages fit the width, until zoomed */
  let fit = true;

  /** Where the PDF was read last, per file (the pages' storage, the same in the PDF's tab and beside its text): the
   *  page, how far down it (0 to 1), the zoom (none: the width of the area). Kept as the PDF is scrolled or zoomed. */
  type Place = { page: number; down: number; scale: number | null };
  const placeKey = () => `pdf-place:${path}`;
  function readPlace(): Place | null {
    try {
      return JSON.parse(localStorage.getItem(placeKey()) ?? "null");
    } catch {
      return null;
    }
  }
  let keepTimer: ReturnType<typeof setTimeout> | undefined;
  function keepPlace() {
    clearTimeout(keepTimer);
    keepTimer = setTimeout(() => {
      if (!viewer || !container || !count) return;
      const page = viewer.currentPageNumber;
      const div = viewer.getPageView(page - 1)?.div as HTMLElement | undefined;
      const down = div && div.offsetHeight ? Math.min(1, Math.max(0, (container.scrollTop - div.offsetTop) / div.offsetHeight)) : 0;
      try {
        localStorage.setItem(placeKey(), JSON.stringify({ page, down, scale: fit ? null : viewer.currentScale } satisfies Place));
      } catch {}
    }, 400);
  }
  /** The PDF where it was read last, if it was. */
  function restorePlace(): boolean {
    const place = readPlace();
    if (!viewer || !container || !place || place.page > count) return false;
    if (place.scale) {
      fit = false;
      viewer.currentScale = place.scale;
    }
    viewer.currentPageNumber = place.page;
    const div = viewer.getPageView(place.page - 1)?.div as HTMLElement | undefined;
    if (div) container.scrollTop = div.offsetTop + place.down * div.offsetHeight;
    return true;
  }
  /** the pages left by links and jumps, and those gone back from */
  let behind: number[] = [];
  let ahead: number[] = [];
  /** the answer of a find under way: found, or not */
  let answer: ((found: boolean) => void) | null = null;

  const told = () => onhistory?.({ back: behind.length > 0, forward: ahead.length > 0 });
  const here = () => viewer?.currentPageNumber ?? 1;

  /** To `page`, the page in view kept to come back to. */
  function jump(to: number) {
    if (!viewer || to === here() || to < 1 || to > count) return;
    behind.push(here());
    ahead = [];
    viewer.currentPageNumber = to;
    told();
  }

  /** In (1), out (-1), the width of the area (0). */
  export function zoom(step: number) {
    if (!viewer || !count) return;
    fit = step === 0;
    if (step > 0) viewer.increaseScale();
    else if (step < 0) viewer.decreaseScale();
    else viewer.currentScaleValue = "page-width";
  }

  /** The keys to the PDF. */
  export function focus() {
    container?.focus();
  }

  /** Back to the page the last link or jump left, and forward again. */
  export function back() {
    const to = behind.pop();
    if (!viewer || to === undefined) return;
    ahead.push(here());
    viewer.currentPageNumber = to;
    told();
  }

  export function forward() {
    const to = ahead.pop();
    if (!viewer || to === undefined) return;
    behind.push(here());
    viewer.currentPageNumber = to;
    told();
  }

  /** To a page (from 1). */
  export function pageTo(to: number) {
    jump(to);
  }

  /** The destinations of the outline's items, in the order told (`onoutline`). */
  let destinations: unknown[] = [];
  let following: ((dest: unknown) => Promise<void>) | undefined;

  /** To the item `i` of the outline, the page in view kept to come back to. */
  export function goToOutline(i: number) {
    const dest = destinations[i];
    if (dest) void following?.(dest);
  }

  /** To the first, previous, next or last page. */
  export function turn(to: Turn) {
    if (!viewer) return;
    if (to === "first") jump(1);
    else if (to === "last") jump(count);
    else if (to === "previous") viewer.previousPage();
    else viewer.nextPage();
  }

  /** Find `text`: the first place, or `again` the next (the one before, `backwards`); an empty text clears. */
  export function find(text: string, again = false, backwards = false): Promise<boolean> {
    answer?.(false);
    answer = null;
    if (!bus) return Promise.resolve(false);
    if (!text) {
      bus.dispatch("findbarclose", { source: null });
      return Promise.resolve(false);
    }
    const found = new Promise<boolean>((resolve) => (answer = resolve));
    bus.dispatch("find", {
      source: null,
      type: again ? "again" : "",
      query: text,
      caseSensitive: false,
      entireWord: false,
      highlightAll: true,
      findPrevious: backwards,
      matchDiacritics: false,
    });
    return found;
  }

  /** The text of the line of the text layer that `span` is on (the spans whose middle is level with its own), in
   *  order; with the next line when it has fewer than six words, for enough of them to find. */
  function lineOf(span: HTMLElement): string {
    const layer = span.closest(".textLayer");
    if (!layer) return span.textContent ?? "";
    const middle = (r: DOMRect) => r.top + r.height / 2;
    const spans = [...layer.querySelectorAll<HTMLElement>("span")].filter((s) => !s.querySelector("span") && s.textContent?.trim());
    const rects = spans.map((s) => s.getBoundingClientRect());
    const own = span.getBoundingClientRect();
    const level = (r: DOMRect, y: number) => Math.abs(middle(r) - y) < Math.max(own.height, 4) / 2;
    const textAt = (y: number) =>
      spans
        .map((s, i) => ({ s, r: rects[i] }))
        .filter(({ r }) => level(r, y))
        .sort((a, b) => a.r.left - b.r.left)
        .map(({ s }) => s.textContent)
        .join(" ")
        .replace(/\s+/g, " ")
        .trim();
    let text = textAt(middle(own));
    if (text.split(" ").length < 6) {
      const below = rects.map(middle).filter((y) => y > middle(own) + own.height / 2).sort((a, b) => a - b)[0];
      if (below !== undefined) text = `${text}\n${textAt(below)}`;
    }
    return text;
  }

  /** where the second press of a double-click was: a double-click whose pointer moved before it was let go is a
   *  selection (double-tap and drag), not a double-click */
  let secondPress: { x: number; y: number } | null = null;
  function pressed(e: MouseEvent) {
    if (e.detail >= 2) secondPress = { x: e.clientX, y: e.clientY };
  }

  /** ⌃-click, or a double-click (one that did not drag: a selection made by tap and drag stays one), on the text:
   *  its place in the source (Go to Source). */
  function sourceClick(e: MouseEvent) {
    if (!onsource || e.button !== 0) return;
    if (e.type === "dblclick") {
      const moved = secondPress && Math.hypot(e.clientX - secondPress.x, e.clientY - secondPress.y) > 4;
      secondPress = null;
      if (moved) return;
      window.getSelection()?.removeAllRanges();
    } else if (!(e.ctrlKey || e.metaKey)) return;
    const span = (e.target as HTMLElement).closest<HTMLElement>(".textLayer span");
    if (!span) return;
    e.preventDefault();
    e.stopPropagation();
    const text = lineOf(span);
    const around = aroundOf(e, span);
    // the word as the line reads, its pieces joined (P4 of P and 4), else as the pointer is over it
    if (text) onsource(text, pointOf(e), around.word || wordAt(e), around);
  }

  /** The words around the word under the pointer, in its line of the PDF and the lines before and after it (the
   *  source has them around the same word): six before, six after. */
  /** The text of spans in reading order, with where each starts: a space between them, except between two that touch
   *  on a line (a word the PDF writes in pieces: P and 4 in another font, a ligature). */
  function joined(spans: { s: HTMLElement; r: DOMRect }[]): { text: string; starts: number[] } {
    let text = "";
    const starts: number[] = [];
    spans.forEach(({ s, r }, i) => {
      const prev = spans[i - 1]?.r;
      const touching = prev && Math.abs(prev.top + prev.height / 2 - (r.top + r.height / 2)) < r.height / 2 && r.left - prev.right < r.height * 0.2;
      if (i > 0 && !touching) text += " ";
      starts.push(text.length);
      text += s.textContent ?? "";
    });
    return { text, starts };
  }

  function aroundOf(e: MouseEvent, span: HTMLElement): { before: string[]; after: string[]; word?: string } {
    const layer = span.closest(".textLayer");
    const range = document.caretRangeFromPoint?.(e.clientX, e.clientY);
    if (!layer || !range) return { before: [], after: [] };
    // the text layer's spans in reading order, and the place of the pointer in their text
    const spans = [...layer.querySelectorAll<HTMLElement>("span")].filter((s) => !s.querySelector("span") && s.textContent);
    // in reading order: grouped in lines (middles within half a line: a citation, a figure in another font sit a
    // little higher or lower), each line from left to right
    const mid = (r: DOMRect) => r.top + r.height / 2;
    const bySide = spans.map((s) => ({ s, r: s.getBoundingClientRect() })).sort((a, b) => mid(a.r) - mid(b.r));
    const lines: (typeof bySide)[] = [];
    for (const p of bySide) {
      const last = lines[lines.length - 1];
      if (last && Math.abs(mid(last[0].r) - mid(p.r)) < Math.max(p.r.height, last[0].r.height) / 2) last.push(p);
      else lines.push([p]);
    }
    const placed = lines.flatMap((line) => line.sort((a, b) => a.r.left - b.r.left));
    const { text, starts } = joined(placed);
    let at = -1;
    placed.forEach(({ s }, i) => {
      if (s.contains(range.startContainer)) at = starts[i] + range.startOffset;
    });
    if (at < 0) return { before: [], after: [] };
    const words = (t: string) => t.toLowerCase().split(/[^\p{L}\p{N}]+/u).filter((w) => w.length > 1);
    // the word itself out of both sides
    let [from, to] = [at, at];
    while (from > 0 && /[\p{L}\p{N}_-]/u.test(text[from - 1])) from--;
    while (to < text.length && /[\p{L}\p{N}_-]/u.test(text[to])) to++;
    return { before: words(text.slice(Math.max(0, from - 160), from)).slice(-6), after: words(text.slice(to, to + 160)).slice(0, 6), word: text.slice(from, to) };
  }

  /** A right-click on the text: Go to Source (as a ⌃-click), the page's own item, and Copy. */
  async function sourceMenu(e: MouseEvent) {
    if (!onsource) return;
    const span = (e.target as HTMLElement).closest<HTMLElement>(".textLayer span");
    if (!span) return;
    e.preventDefault();
    const [text, at, word, around] = [lineOf(span), pointOf(e), wordAt(e), aroundOf(e, span)];
    const mac = navigator.userAgent.includes("Mac");
    const items = await Promise.all([
      MenuItem.new({ text: `Go to Source (${mac ? "⌘" : "Ctrl"}-click)`, action: () => text && onsource?.(text, at, word, around) }),
      ...(menuItem ? [MenuItem.new(menuItem)] : []),
      PredefinedMenuItem.new({ item: "Separator" }),
      PredefinedMenuItem.new({ item: "Copy" }),
    ]);
    // at the pointer (without a place, it opens elsewhere in the window)
    await popupReleased(await Menu.new({ items }), new LogicalPosition(e.clientX, e.clientY));
  }

  /** The word under the pointer, in the text layer. */
  function wordAt(e: MouseEvent): string {
    const range = document.caretRangeFromPoint?.(e.clientX, e.clientY);
    const node = range?.startContainer;
    if (!range || !node || node.nodeType !== Node.TEXT_NODE) return "";
    const text = node.textContent ?? "";
    const isWord = (c: string) => /[\p{L}\p{N}_-]/u.test(c);
    let [from, to] = [range.startOffset, range.startOffset];
    while (from > 0 && isWord(text[from - 1])) from--;
    while (to < text.length && isWord(text[to])) to++;
    return text.slice(from, to);
  }

  /** The place of a source line shown and marked (Show in PDF): `box`, the line of the PDF it most likely is, scrolled
   *  to; `regions`, where the paragraph the source line made is (each page's band, in PDF points from the page's
   *  top); `word` and the words around it in the source (`before`, `after`), which pick the place of the word in the
   *  regions: the one whose neighbours in the PDF are most like them, the nearest to `box` among equals. What was
   *  marked, said. */
  export function showBox(
    box: { page: number; x: number; y: number; width: number; height: number },
    word = "",
    regions: { page: number; top: number; bottom: number }[] = [],
    before: string[] = [],
    after: string[] = [],
  ): string {
    const pageView = viewer?.getPageView(box.page - 1);
    if (!viewer || !pageView) return "the PDF is not read yet";
    const [, , , top] = pageView.viewport.viewBox;
    // PDF space: from the bottom left
    const [x1, y1, x2, y2] = [box.x, top - box.y - 3, box.x + box.width, top - box.y + box.height];
    behind.push(here());
    ahead = [];
    told();
    viewer.scrollPageIntoView({ pageNumber: box.page, destArray: [null, { name: "XYZ" }, x1, y2 + 40, null] });
    // marked once the pages are drawn, and again each time pdf.js draws them anew, till the next click in the PDF
    clearMarks();
    const own = { page: box.page, top: box.y - box.height * 3, bottom: box.y + box.height };
    marked = { page: box.page, line: [x1, y1, x2, y2], word, regions: regions.length ? regions : [own], before, after };
    return drawMark();
  }

  /** The marks of Show in PDF taken off: the word's highlight unwrapped, the line's spans let go. */
  function clearMarks() {
    for (const mark of container.querySelectorAll<HTMLElement>(".synced-word")) {
      const parent = mark.parentNode;
      mark.replaceWith(...mark.childNodes);
      // its text one node again, as pdf.js made it: split, the next search would read only a part of it
      parent?.normalize();
    }
    for (const span of container.querySelectorAll<HTMLElement>(".synced-line")) span.classList.remove("highlight", "selected", "synced-line");
  }

  /** The words of a text as they compare: lower case, letters and digits. */
  const wordsOf = (text: string) => text.toLowerCase().split(/[^\p{L}\p{N}]+/u).filter((w) => w.length > 1);

  /** The mark drawn in the text layers, as pdf.js marks what Find found (a box laid over the page is not drawn over
   *  its canvas by WebKitGTK): the word where its neighbours are most like the source's, else the most likely line's
   *  spans. What was marked, said. */
  function drawMark(): string {
    if (!marked || !viewer) return "";
    const m = marked;
    if (container.querySelector(".synced-word, .synced-line")) return "marked";
    const pattern = m.word ? new RegExp(`(?<![\\p{L}\\p{N}])${m.word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![\\p{L}\\p{N}])`, "giu") : null;
    const before = new Set(m.before.map((w) => w.toLowerCase()));
    const after = new Set(m.after.map((w) => w.toLowerCase()));
    let best: { score: number; distance: number; parts: { span: HTMLElement; from: number; to: number }[] } | null = null;
    let waiting = false;
    // the pages of the paragraph, whole: the map can give a paragraph's lines to other lines of the source, and the
    // words around the word, not the place, pick the right one; the most likely line only breaks ties
    for (const page of [...new Set(m.regions.map((r) => r.page))]) {
      const pageView = viewer.getPageView(page - 1);
      const pageDiv = pageView?.div as HTMLElement | undefined;
      if (!pageView || !pageDiv) continue;
      // the text layer's spans of text: not the wrappers of others (pdf.js's marked content), not the highlights inside
      // them (Find's, the marks), which are of their span's text
      const isMark = (e: Element) => e.classList.contains("highlight") || e.classList.contains("synced-word");
      const leaves = [...pageDiv.querySelectorAll<HTMLElement>(".textLayer span")].filter(
        (s) => !isMark(s) && ![...s.children].some((c) => c.tagName === "SPAN" && !isMark(c)) && !!s.textContent,
      );
      if (!leaves.length) {
        waiting = true;
        continue;
      }
      const [, , , top] = pageView.viewport.viewBox;
      const yOf = (fromTop: number) => pageView.viewport.convertToViewportPoint(0, top - fromTop)[1] as number;
      const origin = pageDiv.getBoundingClientRect().top + pageDiv.clientTop;
      const lineMiddle = page === m.page ? (yOf(top - m.line[1]) + yOf(top - m.line[3])) / 2 : null;
      // the spans in reading order: grouped in lines (their middles within half a line of each other: a citation, a
      // figure in another font sit a little higher or lower), each line from left to right
      const placed = leaves.map((span) => ({ span, rect: span.getBoundingClientRect() })).sort((x, y) => x.rect.top + x.rect.height / 2 - (y.rect.top + y.rect.height / 2));
      const lines: (typeof placed)[] = [];
      for (const p of placed) {
        const last = lines[lines.length - 1];
        const mid = (r: DOMRect) => r.top + r.height / 2;
        if (last && Math.abs(mid(last[0].rect) - mid(p.rect)) < Math.max(p.rect.height, last[0].rect.height) / 2) last.push(p);
        else lines.push([p]);
      }
      // their text, a space between spans except between two that touch (a word written in pieces: P and 4)
      let flat = "";
      const pieces: { span: HTMLElement; start: number; middle: number }[] = [];
      for (const line of lines) {
        line.sort((x, y) => x.rect.left - y.rect.left);
        line.forEach(({ span, rect }, i) => {
          const prev = line[i - 1]?.rect;
          if (flat && !(prev && rect.left - prev.right < rect.height * 0.2)) flat += " ";
          pieces.push({ span, start: flat.length, middle: rect.top + rect.height / 2 - origin });
          flat += span.textContent ?? "";
        });
      }
      if (!pattern) continue;
      for (const found of flat.matchAll(pattern)) {
        const at = found.index ?? 0;
        const end = at + found[0].length;
        // the pieces the word is in (one, or several where the PDF writes it in pieces)
        const parts = pieces
          .filter((p) => p.start < end && p.start + (p.span.textContent?.length ?? 0) > at)
          .map((p) => ({ span: p.span, from: Math.max(0, at - p.start), to: Math.min(p.span.textContent?.length ?? 0, end - p.start), middle: p.middle }));
        if (!parts.length) continue;
        const score =
          wordsOf(flat.slice(Math.max(0, at - 90), at)).slice(-6).filter((w) => before.has(w)).length +
          wordsOf(flat.slice(end, end + 90)).slice(0, 6).filter((w) => after.has(w)).length;
        const distance = lineMiddle === null ? 1e6 : Math.abs(parts[0].middle - lineMiddle);
        if (!best || score > best.score || (score === best.score && distance < best.distance)) best = { score, distance, parts };
      }
    }
    if (best) {
      // each piece marked (from the last, so that the earlier ones' places hold)
      for (const part of [...best.parts].reverse()) {
        if (part.to <= part.from) continue;
        markText(part.span, part.from, part.to);
      }
      return best.score ? "word marked" : "word marked (its neighbours differ)";
    }
    if (waiting) return "marked once the page is drawn";
    // the word is not in the text (math, a citation): the most likely line
    const pageView = viewer.getPageView(m.page - 1);
    const pageDiv = pageView?.div as HTMLElement | undefined;
    if (!pageView || !pageDiv) return "no such page";
    const origin = pageDiv.getBoundingClientRect().top + pageDiv.clientTop;
    const ys = [m.line[1], m.line[3]].map((y) => pageView.viewport.convertToViewportPoint(0, y)[1] as number);
    const middle = (ys[0] + ys[1]) / 2;
    const height = Math.abs(ys[1] - ys[0]);
    let lines = 0;
    for (const span of pageDiv.querySelectorAll<HTMLElement>(".textLayer span:not(.synced-word)")) {
      const r = span.getBoundingClientRect();
      if (Math.abs(r.top + r.height / 2 - origin - middle) <= height / 2 + 2) {
        span.classList.add("highlight", "selected", "synced-line");
        lines++;
      }
    }
    return lines ? "line marked (the word is not in the PDF's text there)" : "nothing of the page's text there to mark";
  }


  /** The characters [from, to) of a span's text wrapped in a mark, through whichever text nodes hold them (a span's
   *  text can be in several: Find's highlights split it). */
  function markText(span: HTMLElement, from: number, to: number) {
    const walker = document.createTreeWalker(span, NodeFilter.SHOW_TEXT);
    const nodes: { node: Text; start: number }[] = [];
    let at = 0;
    for (let n = walker.nextNode() as Text | null; n; n = walker.nextNode() as Text | null) {
      nodes.push({ node: n, start: at });
      at += n.length;
    }
    for (const { node, start } of nodes.reverse()) {
      const a = Math.max(from, start) - start;
      const b = Math.min(to, start + node.length) - start;
      if (b <= a) continue;
      const range = document.createRange();
      range.setStart(node, a);
      range.setEnd(node, b);
      const mark = document.createElement("span");
      mark.className = "highlight selected synced-word";
      range.surroundContents(mark);
    }
  }

  /** A page's text layer drawn anew (pdf.js makes it again as it draws the page): the mark drawn on it again. */
  function redrawn({ pageNumber }: { pageNumber: number }) {
    if (marked?.regions.some((r) => r.page === pageNumber)) drawMark();
  }

  /** The place marked by Show in PDF: the page and line (PDF space) most likely, the word, the paragraph's regions,
   *  the source's words around the word. */
  let marked: {
    page: number;
    line: number[];
    word: string;
    regions: { page: number; top: number; bottom: number }[];
    before: string[];
    after: string[];
  } | null = null;










  /** The place of a click in the PDF: its page, and its point in PDF points from the page's top left. */
  function pointOf(e: MouseEvent): { page: number; x: number; y: number } | null {
    const pageDiv = (e.target as HTMLElement).closest<HTMLElement>(".page[data-page-number]");
    const page = Number(pageDiv?.dataset.pageNumber);
    const pageView = page && viewer?.getPageView(page - 1);
    if (!pageDiv || !pageView) return null;
    const box = pageDiv.getBoundingClientRect();
    const viewport = pageView.viewport;
    const [x, y] = viewport.convertToPdfPoint(e.clientX - box.left - pageDiv.clientLeft, e.clientY - box.top - pageDiv.clientTop);
    // PDF space is from the bottom left; SyncTeX's from the top left of the page's box
    const [, , , top] = viewport.viewBox;
    return { page, x, y: top - y };
  }

  onMount(() => {
    let task: PDFDocumentLoadingTask | null = null;
    container.addEventListener("click", sourceClick, true);
    container.addEventListener("dblclick", sourceClick, true);
    container.addEventListener("mousedown", pressed, true);
    container.addEventListener("contextmenu", sourceMenu);
    const pages = document.createElement("div");
    pages.className = "pdfViewer";
    container.append(pages);
    bus = new EventBus();
    const links = new PDFLinkService({ eventBus: bus });
    // a link followed leaves its page to come back to
    const follow = links.goToDestination.bind(links);
    links.goToDestination = async (dest) => {
      const from = here();
      await follow(dest);
      if (here() !== from) {
        behind.push(from);
        ahead = [];
        told();
      }
    };
    const finder = new PDFFindController({ eventBus: bus, linkService: links });
    const view = (viewer = new PDFViewer({ container, viewer: pages, eventBus: bus, linkService: links, findController: finder }));
    links.setViewer(view);
    bus.on("pagesinit", () => {
      view.currentScaleValue = "page-width";
      // where it was read last; else the page asked
      if (!restorePlace() && page > 1) view.currentPageNumber = Math.min(page, count);
    });
    bus.on("scalechanging", keepPlace);
    container.addEventListener("scroll", keepPlace, { passive: true });
    bus.on("pagechanging", ({ pageNumber }: { pageNumber: number }) => onpage(pageNumber));
    bus.on("textlayerrendered", redrawn);
    // a click in the PDF: the mark of Show in PDF gone
    container.addEventListener("pointerdown", () => {
      marked = null;
      clearMarks();
    });
    // FOUND 0, NOT_FOUND 1, WRAPPED 2, PENDING 3
    bus.on("updatefindcontrolstate", ({ state }: { state: number }) => {
      if (state === 3 || !answer) return;
      answer(state !== 1);
      answer = null;
    });
    const resize = new ResizeObserver(() => {
      if (fit && count) view.currentScaleValue = "page-width";
    });
    resize.observe(area);
    (async () => {
      try {
        const data = await invoke<ArrayBuffer>("read_pdf", { path });
        task = getDocument({ data: new Uint8Array(data) });
        const doc = await task.promise;
        count = doc.numPages;
        view.setDocument(doc);
        links.setDocument(doc);
        onopen(count);
        following = (dest) => links.goToDestination(dest as Parameters<typeof links.goToDestination>[0]);
        if (onoutline) {
          // the bookmarks, depth first, with the page each goes to
          type Node = { title: string; dest: unknown; items: Node[] };
          const items: { level: number; title: string; page: number | null }[] = [];
          destinations = [];
          const pageOf = async (dest: unknown): Promise<number | null> => {
            try {
              const explicit = typeof dest === "string" ? await doc.getDestination(dest) : (dest as unknown[] | null);
              const ref = explicit?.[0];
              if (ref == null) return null;
              return typeof ref === "number" ? ref + 1 : (await doc.getPageIndex(ref as Parameters<typeof doc.getPageIndex>[0])) + 1;
            } catch {
              return null;
            }
          };
          const walk = async (nodes: Node[], level: number) => {
            for (const n of nodes) {
              destinations.push(n.dest);
              items.push({ level, title: n.title, page: await pageOf(n.dest) });
              if (n.items?.length) await walk(n.items, level + 1);
            }
          };
          await walk(((await doc.getOutline()) ?? []) as Node[], 1);
          onoutline(items);
        }
      } catch (e) {
        error = String(e);
      }
    })();
    return () => {
      container.removeEventListener("click", sourceClick, true);
      container.removeEventListener("dblclick", sourceClick, true);
      container.removeEventListener("mousedown", pressed, true);
      container.removeEventListener("contextmenu", sourceMenu);
      resize.disconnect();
      task?.destroy();
    };
  });
</script>

<div class="pdf" bind:this={area}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="pdfjs" bind:this={container} tabindex="0"></div>
  {#if error}<p class="error">{error}</p>{/if}
</div>

<style>
  .pdf {
    position: absolute;
    inset: 0;
  }
  /* pdf.js wants its container positioned, the pages in it */
  .pdfjs {
    position: absolute;
    inset: 0;
    overflow: auto;
    outline: none;
    background: var(--sidebar, #eeeef1);
  }
  /* what Find found, in a highlighter's yellow as the place Show in PDF found */
  .pdf :global(.textLayer .highlight) {
    --highlight-bg-color: rgba(255, 221, 0, 0.45);
    --highlight-selected-bg-color: rgba(255, 170, 0, 0.6);
  }
  .error {
    position: absolute;
    inset: 40px 0 auto;
    color: var(--error);
    text-align: center;
  }
</style>
