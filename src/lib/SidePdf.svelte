<script lang="ts">
  // The PDF beside a text (View ▸ Show PDF): the one chosen for this source file, else the one found (`side_pdf`:
  // `<build>/<name>.pdf`), in a pane of the text's page, read again at the same page when it changes on disk
  // (`pdf-changed`: a build, a watch process). Its header says which PDF and changes it (the project's PDFs, the most
  // recent first; Other…; the one found). A ⌃-click on its text opens the source there (`onsource`); `find` looks for
  // words in it (Show in PDF).

  import { invoke } from "@tauri-apps/api/core";
  import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  import { onMount } from "svelte";
  import PdfView from "$lib/PdfView.svelte";
  import PdfjsView from "$lib/PdfjsView.svelte";
  import PdfTools from "$lib/PdfTools.svelte";
  import { listen } from "$lib/events";

  type At = { page: number; x: number; y: number };
  type Around = { before: string[]; after: string[] };
  let { onsource, onsay }: { onsource: (pdf: string, text: string, at: At | null, word: string, around: Around) => void; onsay: (what: string) => void } = $props();

  type Offered = { path: string; name: string; modified: number };
  type Side = { pdf: string | null; offered: Offered[] };

  /** PDFKit is macOS's */
  const mac = navigator.userAgent.includes("Mac");
  const Viewer = mac ? PdfView : PdfjsView;

  let side = $state<Side | null>(null);
  let error = $state("");
  let view = $state<PdfView | PdfjsView>();
  /** the page in view, kept as the PDF is read again */
  let page = $state(1);
  /** how many times the PDF was read again: the view is made anew */
  let reads = $state(0);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const nameOf = (path: string) => path.split("/").pop() ?? path;

  async function ask(step: Promise<Side | null>) {
    try {
      const got = await step;
      if (got) [side, error] = [got, ""];
    } catch (e) {
      error = String(e);
    }
  }

  const choose = (pdf: string | null) => ask(invoke<Side>("set_side_pdf", { pdf }));
  const other = () => ask(invoke<Side | null>("choose_side_pdf"));

  /** The PDF's name in the header, a click on it: the project's PDFs, Other…, and back to the one found. */
  async function changeMenu(e: MouseEvent) {
    const anchor = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const offered = await Promise.all(
      (side?.offered ?? []).slice(0, 15).map((o) => MenuItem.new({ text: o.name, action: () => void choose(o.path) })),
    );
    const items = [
      ...offered,
      ...(offered.length ? [await PredefinedMenuItem.new({ item: "Separator" })] : []),
      await MenuItem.new({ text: "Other…", action: () => void other() }),
      await MenuItem.new({ text: "The One Built from This File", action: () => void choose(null) }),
    ];
    await (await Menu.new({ items })).popup(new LogicalPosition(anchor.left, anchor.bottom + 2));
  }

  /** Words looked for in the PDF and shown there; false when they are not in it. */
  export async function find(text: string): Promise<boolean> {
    return (await view?.find(text)) ?? false;
  }

  /** The place `line` (1-based) of the source `source` is at in the PDF, from the map its build left beside it
   *  (texmark's, or SyncTeX's for LaTeX), shown and marked; false without a map, or when it says nothing there. */
  export async function showSource(source: string, line: number, word = "", along = 0, before: string[] = [], after: string[] = []): Promise<number | null> {
    if (!side?.pdf || !view || !("showBox" in view)) return null;
    status = "Looking it up in the build's map…";
    const boxes = await invoke<Box[] | null>("pdf_boxes", { pdf: side.pdf, source, line }).catch((e) => ((status = String(e)), null));
    if (!boxes?.length) {
      status = "No map from the build beside the PDF: by the words";
      return null;
    }
    // the lines of the PDF the source line made, in reading order; the one as far through them as the cursor is
    // through the source line (a paragraph is often one source line)
    const lines = [...boxes].sort((a, b) => a.page - b.page || a.y - b.y || a.x - b.x);
    const box = lines[Math.min(lines.length - 1, Math.max(0, Math.round(along * (lines.length - 1))))];
    // the paragraph's band on each page, from its first line (one more above: the map can miss it) to its last
    const regions = [...new Set(lines.map((l) => l.page))].map((page) => {
      const own = lines.filter((l) => l.page === page);
      const height = Math.max(...own.map((l) => l.height));
      return { page, top: Math.min(...own.map((l) => l.y)) - height * 2.6, bottom: Math.max(...own.map((l) => l.y)) + height * 0.4 };
    });
    const how = (view as PdfjsView).showBox(box, word, regions, before, after);
    status = `→ page ${box.page}: ${how}`;
    return box.page;
  }

  /** What Show in PDF did last, said in the header a moment. */
  let status = $state("");
  let statusTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    if (!status) return;
    clearTimeout(statusTimer);
    statusTimer = setTimeout(() => (status = ""), 8000);
  });
  export const tell = (what: string) => (status = what);
  type Box = { page: number; x: number; y: number; width: number; height: number };

  /** the PDF's outline (its bookmarks, pdf.js), each with its depth */
  let outline = $state.raw<{ level: number; title: string; page: number | null }[]>([]);
  /** the tools over the PDF (PdfTools, as a PDF's tab has them): Find is theirs */
  let tools = $state<PdfTools>();
  /** the pages in view, their count, and whether there is a page to go back to or forward */
  let count = $state(0);
  let history = $state({ back: false, forward: false });

  /** Find… with the PDF in front: its field, with the PDF's selection. */
  export const focusFind = () => tools?.focusFind();
  /** Find Next and Find Previous with the PDF in front. */
  export const findAgain = (backwards: boolean) => tools?.findAgain(backwards);
  /** Use Selection for Find with the PDF in front. */
  export const findSelection = () => tools?.findSelection();

  /** Whether a PDF is shown. */
  export const showing = () => !!side?.pdf;

  onMount(() => {
    ask(invoke<Side>("side_pdf"));
    const unlisten = listen("pdf-changed", () => {
      // a build writes it in several passes: read again a moment after the last
      clearTimeout(timer);
      timer = setTimeout(() => reads++, 500);
    });
    return () => {
      clearTimeout(timer);
      unlisten.then((f) => f());
      invoke("hide_side_pdf").catch(() => {});
    };
  });
</script>

<aside class="side-pdf">
  <!-- the tools of a PDF as its tab has them, then what was done and which PDF it is, last before the page's × that
       closes the pane -->
  <PdfTools bind:this={tools} view={side?.pdf ? (view as never) : undefined} current={page} {count} {outline} room={30}>
    {#snippet trail()}
      {#if status}<span class="status" title={status}>{status}</span>{/if}
      <button class="name" onclick={changeMenu} disabled={!side} title="{side?.pdf ?? ''} (a click changes it)">{side?.pdf ? nameOf(side.pdf) : "PDF"} ▾</button>
    {/snippet}
  </PdfTools>
  <div class="body">
    {#if error}
      <p class="note error">{error}</p>
    {:else if side && side.pdf}
      {#key `${side.pdf}:${reads}`}
        <Viewer
          bind:this={view}
          path={side.pdf}
          {page}
          onpage={(p) => (page = p)}
          onopen={(c: number) => ((count = c), (page = Math.min(page, c)))}
          onhistory={(h: { back: boolean; forward: boolean }) => (history = h)}
          onoutline={(items: { level: number; title: string; page: number | null }[]) => (outline = items)}
          onsource={(text: string, at: At | null, word: string, around: Around) => side?.pdf && onsource(side.pdf, text, at, word, around)}
        />
      {/key}
    {:else if side}
      <div class="choose">
        <p class="note">Which PDF goes with this file? It is kept for it.</p>
        {#each side.offered as o (o.path)}
          <button class="pick" onclick={() => choose(o.path)} title={o.path}>{o.name}</button>
        {/each}
        <button class="pick" onclick={other}>Other…</button>
      </div>
    {/if}
  </div>
</aside>

<style>
  aside {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    background: var(--viewer, var(--sidebar));
  }
  .name {
    margin-left: auto;
    flex: 0 1 auto;
    min-width: 3em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
    font-size: 11.5px;
  }
  /* inside the field, at its right: clears it */
  .name,
  .pick {
    font: inherit;
    font-size: 12px;
    padding: 1px 8px;
    color: var(--fg);
    background: var(--chip);
    border: 1px solid var(--border);
    border-radius: 5px;
  }
  .body {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .choose {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 12px;
    overflow: auto;
    max-height: 100%;
    box-sizing: border-box;
  }
  .note {
    margin: 0 0 6px;
    color: var(--muted);
    font-size: 12.5px;
  }
  .error {
    padding: 12px;
    color: var(--error);
  }
</style>
