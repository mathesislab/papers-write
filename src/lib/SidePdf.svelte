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
  import { listen } from "$lib/events";

  type At = { page: number; x: number; y: number };
  let { onsource, onsay }: { onsource: (pdf: string, text: string, at: At | null, word: string) => void; onsay: (what: string) => void } = $props();

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
  export async function showSource(source: string, line: number, word = ""): Promise<boolean> {
    if (!side?.pdf || !view || !("showBox" in view)) return false;
    const boxes = await invoke<Box[] | null>("pdf_boxes", { pdf: side.pdf, source, line }).catch(() => null);
    if (!boxes?.length) return false;
    (view as PdfjsView).showBox(boxes[0], word);
    return true;
  }
  type Box = { page: number; x: number; y: number; width: number; height: number };

  /** the PDF's outline (its bookmarks, pdf.js), each with its depth */
  let outline = $state.raw<{ level: number; title: string }[]>([]);
  /** what is looked for in the PDF (the header's field), and whether it was found */
  let query = $state("");
  let lastQuery = "";
  let notFound = $state(false);

  /** The header's field: Enter the next match, ⇧Enter the one before, Esc clears it. */
  async function search(e: KeyboardEvent) {
    if (e.key === "Escape") return void ((query = ""), (notFound = false));
    if (e.key !== "Enter" || !query.trim()) return;
    e.preventDefault();
    const again = query === lastQuery;
    lastQuery = query;
    notFound = !(await view?.find(query, again, e.shiftKey));
  }

  /** The outline as a menu: a click goes to its place. */
  async function outlineMenu(e: MouseEvent) {
    const anchor = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items = await Promise.all(
      outline.slice(0, 200).map((o, i) =>
        MenuItem.new({ text: `${"    ".repeat(Math.max(0, o.level - 1))}${o.title}`, action: () => (view as PdfjsView | undefined)?.goToOutline?.(i) }),
      ),
    );
    await (await Menu.new({ items })).popup(new LogicalPosition(anchor.left, anchor.bottom + 2));
  }

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

<aside>
  <header>
    <button class="name" onclick={changeMenu} disabled={!side} title="{side?.pdf ?? ''} (a click changes it)">{side?.pdf ? nameOf(side.pdf) : "PDF"} ▾</button>
    {#if side?.pdf}
      <span class="tools">
        {#if outline.length}<button onclick={outlineMenu} title="Outline: the PDF's sections">Outline ▾</button>{/if}
        <button onclick={() => view?.zoom(-1)} title="Zoom Out" aria-label="Zoom Out">−</button>
        <button onclick={() => view?.zoom(0)} title="Fit Width">Fit</button>
        <button onclick={() => view?.zoom(1)} title="Zoom In" aria-label="Zoom In">+</button>
        <input
          class="find"
          class:missing={notFound}
          bind:value={query}
          oninput={() => (notFound = false)}
          onkeydown={search}
          placeholder="Find"
          aria-label="Find in the PDF"
          spellcheck="false"
        />
      </span>
    {/if}
  </header>
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
          onopen={(count) => (page = Math.min(page, count))}
          onoutline={(items: { level: number; title: string }[]) => (outline = items)}
          onsource={(text: string, at: At | null, word: string) => side?.pdf && onsource(side.pdf, text, at, word)}
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
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    /* room at the right for the page's × of the pane */
    padding: 4px 34px 4px 8px;
    border-bottom: 1px solid var(--line);
    background: var(--bg);
    font-size: 12px;
  }
  .name {
    flex: 0 1 auto;
    min-width: 3em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tools {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 3px;
    flex: none;
  }
  .find {
    width: 9em;
    font: inherit;
    font-size: 12px;
    padding: 1px 6px;
    color: var(--fg);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 5px;
  }
  .find.missing {
    border-color: var(--error);
  }
  header button,
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
