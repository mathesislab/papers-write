<script lang="ts">
  // A PDF by pdf.js, where PDFKit is not (Linux), as Papers Desktop shows them there: the props and the commands of
  // every view of a PDF (`PdfViewer`, pdf.ts), in the page itself, so a card over it covers it as any element. Back and
  // Forward are the pages the links and the jumps (Go to Page, first, last) left. A ⌃-click on the text gives the
  // place clicked (`onsource`), for the page to open its source there; the text layer gives the line and the words.

  import { bestPlace, better, spoken, wordAround, withNext, type At, type Found, type PdfViewProps, type Spot, type Target } from "$lib/pdf";
  import { pdfMenu } from "$lib/pdfMenu";
  import { invoke } from "@tauri-apps/api/core";
  import { GlobalWorkerOptions, getDocument, type PDFDocumentLoadingTask } from "pdfjs-dist";
  import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
  import { EventBus, PDFFindController, PDFLinkService, PDFViewer } from "pdfjs-dist/web/pdf_viewer.mjs";
  import "pdfjs-dist/web/pdf_viewer.css";
  import { onMount } from "svelte";
  import type { Turn } from "tauri-plugin-pdfkit-api";

  GlobalWorkerOptions.workerSrc = workerUrl;

  // `covered`: the pdf.js view is part of the page, which a card covers as any element
  let { path, page = 1, onpage, onhistory, onopen, onsource, menuItem = null, onoutline }: PdfViewProps = $props();

  let area: HTMLDivElement;
  let container: HTMLDivElement;
  let viewer: PDFViewer | undefined;
  let bus: EventBus | undefined;
  let count = 0;
  let error = $state("");
  /** the pages fit the width, until zoomed */
  let fit = true;
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
   *  order; with the next line when it is short (`withNext`). */
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
    const below = rects.map(middle).filter((y) => y > middle(own) + own.height / 2).sort((a, b) => a - b)[0];
    return withNext(textAt(middle(own)), below === undefined ? "" : textAt(below));
  }

  function sourceClick(e: MouseEvent) {
    if (!onsource || !(e.ctrlKey || e.metaKey) || e.button !== 0) return;
    const span = (e.target as HTMLElement).closest<HTMLElement>(".textLayer span");
    if (!span) return;
    e.preventDefault();
    e.stopPropagation();
    const spot = spotOf(e, span);
    if (spot.line) onsource(spot);
  }

  /** The click on `span`: its line, its place, and the word under the pointer with the words around it in the page's
   *  text (the source has them around the same word): the word as the line reads, its pieces joined (P4 of P and 4),
   *  else as the pointer is over it. */
  function spotOf(e: MouseEvent, span: HTMLElement): Spot {
    const around = aroundOf(e, span);
    return { line: lineOf(span), at: pointOf(e), word: around.word || wordAt(e), before: around.before, after: around.after };
  }

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

  /** The word under the pointer and the words around it, in the text layer's text in reading order. */
  function aroundOf(e: MouseEvent, span: HTMLElement): { before: string[]; after: string[]; word: string } {
    const layer = span.closest(".textLayer");
    const range = document.caretRangeFromPoint?.(e.clientX, e.clientY);
    const none = { before: [], after: [], word: "" };
    if (!layer || !range) return none;
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
    return at < 0 ? none : wordAround(text, at);
  }

  /** A right-click on the text: the menu of a PDF's text (pdfMenu). */
  async function sourceMenu(e: MouseEvent) {
    if (!onsource) return;
    const span = (e.target as HTMLElement).closest<HTMLElement>(".textLayer span");
    if (!span) return;
    e.preventDefault();
    const spot = spotOf(e, span);
    const selected = window.getSelection()?.toString().trim() ?? "";
    await pdfMenu(spot.line ? spot : null, selected, selected ? "" : fromHere(span), { onsource, menuItem }, e.clientX, e.clientY);
  }

  /** The page's text from `span` to its end, as a selection of it would copy it (its lines and spaces). */
  function fromHere(span: HTMLElement): string {
    const layer = span.closest(".textLayer");
    const selection = window.getSelection();
    if (!layer || !selection) return span.textContent ?? "";
    const range = document.createRange();
    range.setStartBefore(span);
    range.setEndAfter(layer.lastChild ?? span);
    selection.removeAllRanges();
    selection.addRange(range);
    const text = selection.toString();
    selection.removeAllRanges();
    return spoken(text);
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

  /** Show in PDF: `box`, the line of the PDF the source line most likely is, scrolled to; the word marked in the
   *  paragraph's pages (`regions`) where its neighbours are most like the source's (`bestPlace`), the nearest to `box`
   *  among equals. What was marked, said. */
  export async function show({ box, word, regions, before, after }: Target): Promise<string> {
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

  /** The mark drawn in the text layers, as pdf.js marks what Find found (a box laid over the page is not drawn over
   *  its canvas by WebKitGTK): the word where its neighbours are most like the source's, else the most likely line's
   *  spans. What was marked, said. */
  function drawMark(): string {
    if (!marked || !viewer) return "";
    const m = marked;
    if (container.querySelector(".synced-word, .synced-line")) return "marked";
    let best: (Found & { parts: { span: HTMLElement; from: number; to: number }[] }) | null = null;
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
      // the pieces a stretch of the text is in (one, or several where the PDF writes a word in pieces)
      const partsOf = (from: number, to: number) =>
        pieces
          .filter((p) => p.start < to && p.start + (p.span.textContent?.length ?? 0) > from)
          .map((p) => ({ span: p.span, from: Math.max(0, from - p.start), to: Math.min(p.span.textContent?.length ?? 0, to - p.start), middle: p.middle }));
      const found = bestPlace(flat, m.word, m.before, m.after, (from, to) => {
        const first = partsOf(from, to)[0];
        return lineMiddle === null || !first ? 1e6 : Math.abs(first.middle - lineMiddle);
      });
      if (found) best = better(best, { ...found, parts: partsOf(found.from, found.to) });
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
  function pointOf(e: MouseEvent): At | null {
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
      if (page > 1) view.currentPageNumber = Math.min(page, count);
    });
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
