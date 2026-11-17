<script lang="ts">
  // A PDF by pdf.js, where PDFKit is not (Linux), as Papers Desktop shows them there: the props and the commands of
  // PdfView (zoom, back and forward, a page, a turn, find), in the page itself, so a card over it covers it as any
  // element. Back and Forward are the pages the links and the jumps (Go to Page, first, last) left.

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

  onMount(() => {
    let task: PDFDocumentLoadingTask | null = null;
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
