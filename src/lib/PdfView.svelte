<script lang="ts">
  // A PDF in PDFKit, as Preview shows it (tauri-plugin-pdfkit, as papers-review has it): a native view over the area
  // of this element, kept in place as the area changes; the props and the commands of every view of a PDF (`PdfViewer`,
  // pdf.ts). The commands go to the view in order: a search typed letter by letter must not land after the next one.

  import { onMount } from "svelte";
  import * as pdfkit from "tauri-plugin-pdfkit-api";
  import type { Turn } from "tauri-plugin-pdfkit-api";
  import type { PdfViewProps } from "$lib/pdf";

  // `covered`: hidden while the page shows a card over it, as the view lies above all of the page
  let { path, page = 1, covered = false, onpage, onhistory, onopen }: PdfViewProps = $props();

  let area: HTMLDivElement;
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

  /** The keys to the PDF, unless a card covers it. */
  export function focus() {
    if (!covered) send(() => pdfkit.cover(id, false));
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

  /** To the first, previous, next or last page. */
  export function turn(to: Turn) {
    send(() => pdfkit.turn(to));
  }

  /** Find `text`: the first place, or `again` the next (the one before, `backwards`); an empty text clears. */
  export function find(text: string, again = false, backwards = false): Promise<boolean> {
    return send(() => pdfkit.find(text, again, backwards));
  }

  // a card shown or gone after the PDF opened
  let opening = true;
  $effect(() => {
    const hidden = covered;
    if (opening) opening = false;
    else send(() => pdfkit.cover(id, hidden));
  });

  onMount(() => {
    const shown = path;
    let opened = false;
    // the area changes with the window and the bar above it
    const resize = new ResizeObserver(() => {
      if (opened) send(() => pdfkit.setFrame(pdfkit.frameOf(area)));
    });
    resize.observe(area);
    const unlisten = pdfkit.onPage((p) => onpage(p));
    const unlistenHistory = pdfkit.onHistory((h) => onhistory?.(h));
    send(() => pdfkit.open(id, shown, page, pdfkit.frameOf(area), covered))
      .then(({ count }) => {
        opened = true;
        onopen(count);
      })
      .catch((e) => (error = String(e)));
    return () => {
      resize.disconnect();
      unlisten.then((f) => f());
      unlistenHistory.then((f) => f());
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
