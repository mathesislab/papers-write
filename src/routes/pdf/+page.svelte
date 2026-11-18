<script lang="ts">
  // The page of a tab that shows a PDF (PDFKit on macOS, pdf.js elsewhere: PdfjsView), in the project bar and sidebar of every tab (Frame): its tools are
  // Back and Forward (the page a link or a jump left: ⌘[ ⌘]), the page in view (and the field to go to one), the zoom
  // (⌘= ⌘- ⌘0, the PDF menu) and Find (⌘F, ⌘G; the menu says them as `pdf`). The Go menu comes as `go`: Back, Forward,
  // the previous, next, first and last page, Go to Page… (⌥⌘G). The PDF is read again, at the same page, when it
  // changes on disk (`file-changed`: a build writes it, in several passes: a moment after the last). A ⌘-click on its
  // text (Ctrl elsewhere) opens the source there (`open_source_at`). Either view (pdf.ts) does all of this.

  import { showSourceIn } from "$lib/pdfPlace";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, tick } from "svelte";
  import PdfView from "$lib/PdfView.svelte";
  import PdfjsView from "$lib/PdfjsView.svelte";
  import PdfTools from "$lib/PdfTools.svelte";
  import { sectionAt, type OutlineItem, type PdfViewer, type Spot } from "$lib/pdf";
  import { listen } from "$lib/events";
  import { part, ready } from "papers-write/project";
  import { mac } from "papers-write/editor";
  import Frame from "$lib/Frame.svelte";
  import "$lib/theme.css";
  import { printPdf } from "$lib/print";

  let path = $state<string | null | undefined>();
  let view = $state<PdfViewer>();
  /** PDFKit is macOS's */
  const Viewer = mac ? PdfView : PdfjsView;
  let count = $state(0);
  let current = $state(1);
  /** the tools over the PDF (PdfTools): Find and Go to Page are theirs */
  let tools = $state<PdfTools>();
  /** how many times the PDF was read again: the view is made anew */
  let reads = $state(0);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const name = $derived(path?.split("/").pop() ?? "");
  /** the tab is in front: the PDF (drawn by PDFKit above the page) hides with its tab */
  let front = $state(true);

  /** the PDF opened the first time, for the tab to be shown (`ready`) */
  const opened = part();
  let frame = $state<Frame>();
  /** the PDF's outline (its bookmarks), shown as the sidebar's Outline shows a text's headings */
  let outline = $state.raw<OutlineItem[]>([]);
  const headings = $derived(outline.map((o, i) => ({ level: o.level, title: o.title, from: i })));
  /** what was just done or went wrong, for a moment */
  let note = $state("");
  let noteTimer: ReturnType<typeof setTimeout> | undefined;

  /** A place of a text to show here (Show in PDF with the panes in tabs: panetabs.rs), once the PDF is read: from the
   *  build's map, else by the words of its line. */
  async function showAsked() {
    await opened.promise;
    const asked = await invoke<{ source: string; line: number; along: number; word: string; before: string[]; after: string[]; phrases: string[] } | null>(
      "take_show_source",
    ).catch(() => null);
    if (!asked || !view || !path) return;
    const shown = await showSourceIn(view, path, asked.source, asked.line, asked.word, asked.along, asked.before, asked.after);
    if (shown.page) return say(shown.status);
    for (const phrase of asked.phrases) if (await view.find(phrase)) return say(`Found by its words: “${phrase}”`);
    say("Not found in the PDF");
  }

  /** ⌘-click on the PDF's text: its source opened there, found by its words in the project's Markdown and LaTeX. */
  async function toSource({ line, at, word, before, after }: Spot) {
    if (!path) return;
    try {
      const found = await invoke<boolean>("open_source_at", { pdf: path, text: line, at, word, around: { before, after } });
      if (!found) say("Not found in the project's sources");
    } catch (e) {
      say(String(e));
    }
  }

  function say(what: string) {
    note = what;
    clearTimeout(noteTimer);
    noteTimer = setTimeout(() => (note = ""), 4000);
  }

  async function ask() {
    path = (await invoke<{ path: string | null }>("document")).path;
  }

  function changed() {
    clearTimeout(timer);
    timer = setTimeout(() => {
      count = 0;
      reads++;
    }, 500);
  }

  function command(what: string) {
    switch (what) {
      case "zoom-in":
        return view?.zoom(1);
      case "zoom-out":
        return view?.zoom(-1);
      case "zoom-fit":
        return view?.zoom(0);
      case "find":
      case "find-replace":
        return tools?.focusFind();
      case "find-next":
      case "find-previous":
        return tools?.findAgain(what === "find-previous");
      case "find-selection":
        return tools?.findSelection();
    }
  }

  /** A choice of the Go menu. */
  function go(what: string) {
    switch (what) {
      case "back":
        return view?.back();
      case "forward":
        return view?.forward();
      case "previous-page":
        return view?.turn("previous");
      case "next-page":
        return view?.turn("next");
      case "first-page":
        return view?.turn("first");
      case "last-page":
        return view?.turn("last");
      case "go-to-page":
        if (count) tools?.askPage();
    }
  }

  onMount(() => {
    ready(opened.promise, frame?.loaded() ?? Promise.resolve());
    ask();
    void showAsked();
    const unlisten = [
      listen("show-source", () => void showAsked()),
      // View ▸ Show PDF with this PDF in front (shown from a text, panes in tabs): back to the text, as a toggle
      listen("pdf-beside", () => void invoke("pdf_tab_back").catch(() => false)),
      // File ▸ Print…: the PDF's pages
      listen("print", () => void (path ? printPdf(path).catch((e) => say(`Print: ${e}`)) : undefined)),
      listen("opened", ask), listen("file-changed", changed), listen<string>("pdf", (e) => command(e.payload)), listen<boolean>("tab-shown", (e) => (front = e.payload)), listen<string>("find", (e) => command(e.payload)), listen<string>("go", (e) => go(e.payload))];
    return () => {
      clearTimeout(timer);
      clearTimeout(noteTimer);
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });
</script>

<Frame
  bind:this={frame}
  {path}
  {headings}
  here={sectionAt(outline, current)}
  outlined={outline.length > 0}
  onheading={(h) => view?.goToOutline(h.from)}
  onsay={say}
  onfocus={() => view?.focus()}
>
  <PdfTools bind:this={tools} {view} {current} {count} {outline}>
    {#snippet lead()}<span class="name" class:said={note} title={note || path || ""}>{note || name}</span>{/snippet}
  </PdfTools>
  <div class="viewer">
    {#if path}
      {#key `${path}:${reads}`}
        <Viewer
          bind:this={view}
          {path}
          page={current}
          covered={!front}
          onpage={(p) => (current = p)}
          onoutline={(items) => (outline = items)}
          onsource={toSource}
          onopen={(c) => {
            count = c;
            current = Math.min(current, c);
            opened.done();
          }}
        />
      {/key}
    {/if}
  </div>
</Frame>

<style>
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
  }
  .name.said {
    color: var(--fg);
  }
  .viewer {
    background: var(--viewer);
    flex: 1;
    position: relative;
    min-height: 0;
  }
</style>
