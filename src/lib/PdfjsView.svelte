<script lang="ts">
  // A PDF by pdf.js, where PDFKit is not (Linux), as Papers Desktop shows them there: the props and the commands of
  // PdfView (zoom, back and forward, a page, a turn, find), in the page itself, so a card over it covers it as any
  // element. Back and Forward are the pages the links and the jumps (Go to Page, first, last) left. A ⌃-click on the
  // text gives the line clicked (`onsource`), for the page to open its source there.

  import { invoke } from "@tauri-apps/api/core";
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
  } = $props();

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

  function sourceClick(e: MouseEvent) {
    if (!onsource || !(e.ctrlKey || e.metaKey) || e.button !== 0) return;
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
    const placed = spans.map((s) => ({ s, r: s.getBoundingClientRect() })).sort((a, b) => Math.round(a.r.top - b.r.top) || a.r.left - b.r.left);
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
      } catch (e) {
        error = String(e);
      }
    })();
    return () => {
      container.removeEventListener("click", sourceClick, true);
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
  .error {
    position: absolute;
    inset: 40px 0 auto;
    color: var(--error);
    text-align: center;
  }
</style>
