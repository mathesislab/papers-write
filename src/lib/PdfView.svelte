<script lang="ts">
  // A PDF in PDFKit, as Preview shows it (tauri-plugin-pdfkit, as papers-review has it): a native view over the area
  // of this element, kept in place as the area changes; the props and the commands of every view of a PDF (`PdfViewer`,
  // pdf.ts). The commands go to the view in order: a search typed letter by letter must not land after the next one.
  // A ⌘-click and a right-click come from the view with the page's text and the place clicked in it (`onClick`).

  import { onMount } from "svelte";
  import * as pdfkit from "tauri-plugin-pdfkit-api";
  import type { Click, Entry, Place, Turn } from "tauri-plugin-pdfkit-api";
  import { bestPlace, better, lineOfText, spoken, wordAround, type Found, type OutlineItem, type PdfViewProps, type Spot, type Target } from "$lib/pdf";
  import { pdfMenu } from "$lib/pdfMenu";
  import { over } from "$lib/overlay.svelte";

  // `covered`: hidden while the tab is behind, as the view lies above all of the page (and while a card is over it)
  let { path, page = 1, covered = false, onpage, onhistory, onopen, onoutline, onsource, menuItem = null }: PdfViewProps = $props();

  let area: HTMLDivElement;
  /** the PDF is open: its view follows the area */
  let opened = false;
  // names this view for `cover` and `close`: the PDF read again has a view made anew while the old one goes
  const id = crypto.randomUUID();
  let error = $state("");
  let queue: Promise<unknown> = Promise.resolve();

  function send<T>(command: () => Promise<T>): Promise<T> {
    const sent = queue.then(command);
    queue = sent.catch(() => {});
    return sent;
  }

  /** In (1), out (-1), the width of the area (0). */
  export function zoom(step: number) {
    send(() => pdfkit.zoom(step));
  }

  /** The view hidden: the page shows a card over it (`covered`, or one of `over`'s). */
  const hidden = $derived(covered || over.cards > 0);

  /** The keys to the PDF, unless a card covers it. */
  export function focus() {
    if (!hidden) send(() => pdfkit.cover(id, false));
  }

  /** Back to the page the last link or jump left, and forward again. */
  export function back() {
    send(() => pdfkit.back());
  }

  export function forward() {
    send(() => pdfkit.forward());
  }

  /** To a page (from 1). */
  export function pageTo(page: number) {
    send(() => pdfkit.pageTo(page));
  }

  /** where each item of the outline told (`onoutline`) goes, in its order */
  let targets: number[][] = [];

  /** To the item `i` of the outline. */
  export function goToOutline(i: number) {
    const target = targets[i];
    if (target) send(() => pdfkit.go(target));
  }

  /** The outline told, depth first, each item with its depth. */
  function tellOutline(outline: Entry[]) {
    const items: OutlineItem[] = [];
    targets = [];
    const walk = (entries: Entry[], level: number) => {
      for (const e of entries) {
        targets.push(e.target);
        items.push({ level, title: e.title, page: e.page });
        walk(e.items, level + 1);
      }
    };
    walk(outline, 1);
    onoutline?.(items);
  }

  /** To the first, previous, next or last page. */
  export function turn(to: Turn) {
    send(() => pdfkit.turn(to));
  }

  /** Find `text`: the first place, or `again` the next (the one before, `backwards`); an empty text clears. */
  export function find(text: string, again = false, backwards = false): Promise<boolean> {
    return send(() => pdfkit.find(text, again, backwards));
  }

  /** Show in PDF: the word marked in the pages of the paragraph (`regions`) where its neighbours are most like the
   *  source's (`bestPlace`), the nearest to the line the map gives (`box`) among equals; else that line. What was
   *  marked, said. */
  export async function show({ box, regions, word, before, after }: Target): Promise<string> {
    // the line the map gives (the box is from its baseline up), and where it is in its page's text
    const own = await send(() => pdfkit.place(box.page, box.x + box.width / 2, box.y - box.height / 2)).catch(() => null);
    if (!own) return "the PDF is not read yet";
    let best: (Found & { page: number }) | null = null;
    for (const page of new Set([box.page, ...regions.map((r) => r.page)])) {
      const place = page === box.page ? own : await send(() => pdfkit.place(page, 0, 0)).catch(() => null);
      if (!place) continue;
      const near = (from: number) => (page === box.page && own.index !== null ? Math.abs(from - own.index) : 1e6);
      const found = bestPlace(place.text, word, before, after, near);
      if (found) best = better(best, { ...found, page });
    }
    const chosen = best;
    if (chosen && (await send(() => pdfkit.mark(chosen.page, chosen.from, chosen.to)))) {
      return chosen.score ? "word marked" : "word marked (its neighbours differ)";
    }
    // the word is not in the text (math, a citation): the line
    const line = own.line;
    if (line && (await send(() => pdfkit.mark(box.page, line[0], line[1])))) return "line marked (the word is not in the PDF's text there)";
    return "nothing of the page's text there to mark";
  }

  /** A click on a place of the PDF as Go to Source takes it: the line (with the next when it is short), the point, the
   *  word and the words around it; none off the text. */
  function spotOf(place: Place | null): Spot | null {
    if (!place?.line || place.index === null) return null;
    const at = { page: place.page, x: place.x, y: place.y };
    return { line: lineOfText(place.text, place.line[0], place.line[1]), at, ...wordAround(place.text, place.index) };
  }

  /** A ⌘-click: the source there; a right-click: the menu of a PDF's text (pdfMenu), at the pointer. */
  function clicked({ menu, client, place, selected }: Click) {
    const spot = spotOf(place);
    if (!menu) {
      if (spot) onsource?.(spot);
      return;
    }
    // the page from the line clicked, for Read Aloud from Here
    const fromHere = place?.line ? spoken(place.text.slice(place.line[0])) : "";
    void pdfMenu(spot, selected.trim(), fromHere, { onsource, menuItem }, client.x, client.y);
  }

  /** The area, less what of it the project bar covers while slid in (`over`): the view lies above all of the page, so
   *  it makes way, the PDF fitting the rest, as Preview's does for its sidebar. */
  function frame(): pdfkit.Frame {
    const f = pdfkit.frameOf(area);
    const left = Math.min(Math.max(f.x, over.left), f.x + f.width);
    return { ...f, x: left, width: f.x + f.width - left };
  }

  /** The view moved to the area as it is now. */
  function refit() {
    if (opened) send(() => pdfkit.setFrame(frame()));
  }

  // the bar slid in or out
  $effect(() => {
    void over.left;
    refit();
  });

  // a card shown or gone after the PDF opened
  let opening = true;
  $effect(() => {
    const now = hidden;
    if (opening) opening = false;
    else send(() => pdfkit.cover(id, now));
  });

  onMount(() => {
    const shown = path;
    // the area changes with the window and the bar above it
    const resize = new ResizeObserver(refit);
    resize.observe(area);
    const unlisten = pdfkit.onPage((p) => onpage(p));
    const unlistenHistory = pdfkit.onHistory((h) => onhistory?.(h));
    const unlistenClick = pdfkit.onClick(clicked);
    // the clicks are the page's: ⌘-click to the source, the menu of pdfMenu
    send(() => pdfkit.open(id, shown, page, frame(), hidden, true))
      .then(({ count, outline }) => {
        opened = true;
        onopen(count);
        tellOutline(outline);
      })
      .catch((e) => (error = String(e)));
    return () => {
      resize.disconnect();
      unlisten.then((f) => f());
      unlistenHistory.then((f) => f());
      unlistenClick.then((f) => f());
      send(() => pdfkit.close(id));
    };
  });
</script>

<div class="pdf" bind:this={area}>
  {#if error}<p class="error">{error}</p>{/if}
</div>

<style>
  .pdf {
    position: absolute;
    inset: 0;
  }
  .error {
    color: var(--error);
    text-align: center;
    margin-top: 40px;
  }
</style>
