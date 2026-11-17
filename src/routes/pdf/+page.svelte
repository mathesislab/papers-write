<script lang="ts">
  // The page of a tab that shows a PDF (PDFKit on macOS, pdf.js elsewhere: PdfjsView), in the project bar and sidebar of every tab (Frame): its tools are
  // Back and Forward (the page a link or a jump left: ⌘[ ⌘]), the page in view (and the field to go to one), the zoom
  // (⌘= ⌘- ⌘0, the PDF menu) and Find (⌘F, ⌘G; the menu says them as `pdf`). The Go menu comes as `go`: Back, Forward,
  // the previous, next, first and last page, Go to Page… (⌥⌘G). The PDF is read again, at the same page, when it
  // changes on disk (`file-changed`: a build writes it, in several passes: a moment after the last). A ⌃-click on its
  // text opens the source there (pdf.js only, not PDFKit yet: `open_source_at`).

  import { invoke } from "@tauri-apps/api/core";
  import { onMount, tick } from "svelte";
  import PdfView from "$lib/PdfView.svelte";
  import PdfjsView from "$lib/PdfjsView.svelte";
  import PdfTools from "$lib/PdfTools.svelte";
  import { listen } from "$lib/events";
  import { part, ready } from "papers-write/project";
  import type { Heading } from "papers-write/editor";
  import Frame from "$lib/Frame.svelte";
  import "$lib/theme.css";

  let path = $state<string | null | undefined>();
  let view = $state<PdfView | PdfjsView>();
  /** PDFKit is macOS's */
  const mac = navigator.userAgent.includes("Mac");
  const Viewer = mac ? PdfView : PdfjsView;
  let count = $state(0);
  let current = $state(1);
  /** whether there is a page to go back to, or forward */
  let history = $state({ back: false, forward: false });
  /** the page typed in the field, while it is */
  /** the tools over the PDF (PdfTools): Find and Go to Page are theirs */
  let tools = $state<PdfTools>();
  let text = $state("");
  let notFound = $state(false);
  let field = $state<HTMLInputElement>();
  /** how many times the PDF was read again: the view is made anew */
  let reads = $state(0);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const name = $derived(path?.split("/").pop() ?? "");
  /** the tab is in front: the PDF (drawn by PDFKit above the page) hides with its tab */
  let front = $state(true);

  /** the PDF opened the first time, for the tab to be shown (`ready`) */
  const opened = part();
  let frame = $state<Frame>();
  /** the PDF's outline (its bookmarks: pdf.js only), the page of each, and the one the page in view is under */
  let outline = $state.raw<Heading[]>([]);
  let outlinePages: (number | null)[] = [];
  const hereInOutline = $derived.by(() => {
    let at = -1;
    outlinePages.forEach((p, i) => {
      if (p !== null && p <= current) at = i;
    });
    return outline.length ? at : -1;
  });
  /** a card of the frame over the page: the PDF makes way */
  let carded = $state(false);
  /** what was just done or went wrong, for a moment */
  let note = $state("");
  let noteTimer: ReturnType<typeof setTimeout> | undefined;

  /** ⌃-click on the PDF's text: its source opened there, found by its words in the project's Markdown and LaTeX. */
  async function toSource(text: string, at: { page: number; x: number; y: number } | null = null, word = "", around = { before: [] as string[], after: [] as string[] }) {
    if (!path) return;
    try {
      const found = await invoke<boolean>("open_source_at", { pdf: path, text, at, word, around });
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
      history = { back: false, forward: false };
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
    const unlisten = [listen("opened", ask), listen("file-changed", changed), listen<string>("pdf", (e) => command(e.payload)), listen<boolean>("tab-shown", (e) => (front = e.payload)), listen<string>("find", (e) => command(e.payload)), listen<string>("go", (e) => go(e.payload))];
    return () => {
      clearTimeout(timer);
      clearTimeout(noteTimer);
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });
</script>

<Frame
  bind:this={frame}
  bind:covering={carded}
  {path}
  headings={outline}
  here={hereInOutline}
  outlined={outline.length > 0}
  onheading={(h) => (view as PdfjsView | undefined)?.goToOutline?.(h.from)}
  onsay={say}
  onfocus={() => view?.focus()}
>
  <PdfTools bind:this={tools} view={view as never} {current} {count} outline={outline.map((h, i) => ({ level: h.level, title: h.title, page: outlinePages[i] ?? null }))}>
    {#snippet lead()}<span class="name" class:said={note} title={note || path || ""}>{note || name}</span>{/snippet}
  </PdfTools>
  <div class="viewer">
    {#if path}
      {#key `${path}:${reads}`}
        <Viewer
          bind:this={view}
          {path}
          page={current}
          covered={!front || carded}
          onpage={(p) => (current = p)}
          onoutline={(items: { level: number; title: string; page: number | null }[]) => {
            outline = items.map((it, i) => ({ level: it.level, title: it.title, from: i }));
            outlinePages = items.map((it) => it.page);
          }}
          onhistory={(h) => (history = h)}
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
