<script lang="ts">
  // The page of a tab that shows a PDF (PDFKit on macOS, pdf.js elsewhere: PdfjsView), in the project bar and sidebar of every tab (Frame): its tools are
  // Back and Forward (the page a link or a jump left: ⌘[ ⌘]), the page in view (and the field to go to one), the zoom
  // (⌘= ⌘- ⌘0, the PDF menu) and Find (⌘F, ⌘G; the menu says them as `pdf`). The Go menu comes as `go`: Back, Forward,
  // the previous, next, first and last page, Go to Page… (⌥⌘G). The PDF is read again, at the same page, when it
  // changes on disk (`file-changed`: a build writes it, in several passes: a moment after the last).

  import { invoke } from "@tauri-apps/api/core";
  import { keys } from "papers-write/editor";
  import { onMount, tick } from "svelte";
  import PdfView from "$lib/PdfView.svelte";
  import PdfjsView from "$lib/PdfjsView.svelte";
  import { listen } from "$lib/events";
  import { part, ready } from "papers-write/project";
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
  let typing = $state<string | null>(null);
  let finding = $state(false);
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
  /** a card of the frame over the page: the PDF makes way */
  let carded = $state(false);
  /** what was just done or went wrong, for a moment */
  let note = $state("");
  let noteTimer: ReturnType<typeof setTimeout> | undefined;

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

  async function search(again: boolean, backwards = false) {
    if (!view) return;
    notFound = text !== "" && !(await view.find(text, again, backwards));
  }

  async function startFind() {
    finding = true;
    await tick();
    field?.focus();
    field?.select();
  }

  function stopFind() {
    finding = false;
    notFound = false;
    view?.find("");
    view?.focus();
  }

  function answerPage(go: boolean) {
    const page = Number.parseInt(typing ?? "", 10);
    typing = null;
    if (go && Number.isFinite(page)) view?.pageTo(Math.min(Math.max(page, 1), count));
    view?.focus();
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
        return startFind();
      case "find-next":
      case "find-previous":
        if (text) return search(true, what === "find-previous");
        return startFind();
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
        if (count) typing = String(current);
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

<Frame bind:this={frame} bind:covering={carded} {path} onsay={say} onfocus={() => view?.focus()}>
  <div class="tools">
    <button class="icon" disabled={!history.back} onclick={() => view?.back()} title={keys("Back (⌘[)")} aria-label="Back">‹</button>
    <button class="icon" disabled={!history.forward} onclick={() => view?.forward()} title={keys("Forward (⌘])")} aria-label="Forward">›</button>
    <span class="name" class:said={note} title={note || path || ""}>{note || name}</span>
    {#if typing !== null}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="page-field"
        bind:value={typing}
        autofocus
        inputmode="numeric"
        autocomplete="off"
        aria-label="Go to Page"
        onkeydown={(e) => {
          if (e.key === "Enter") answerPage(true);
          if (e.key === "Escape") answerPage(false);
        }}
        onblur={() => (typing = null)}
      />
      <span class="page">of {count}</span>
    {:else if count}
      <button class="page" onclick={() => (typing = String(current))} title={keys("Go to Page… (⌥⌘G)")}>Page {current} of {count}</button>
    {/if}
    <button onclick={() => view?.zoom(-1)} title={keys("Zoom Out (⌘-)")}>−</button>
    <button onclick={() => view?.zoom(0)} title={keys("Fit Width (⌘0)")}>Fit</button>
    <button onclick={() => view?.zoom(1)} title={keys("Zoom In (⌘=)")}>+</button>
    {#if finding}
      <input
        bind:this={field}
        class:missing={notFound}
        bind:value={text}
        placeholder="Find"
        autocomplete="off"
        spellcheck="false"
        aria-label="Find"
        oninput={() => search(false)}
        onkeydown={(e) => {
          if (e.key === "Enter") search(true, e.shiftKey);
          if (e.key === "Escape") stopFind();
        }}
      />
      <button onclick={stopFind} title="Close Find (esc)">Done</button>
    {:else}
      <button onclick={startFind} title={keys("Find (⌘F)")}>Find</button>
    {/if}
  </div>
  <div class="viewer">
    {#if path}
      {#key `${path}:${reads}`}
        <Viewer
          bind:this={view}
          {path}
          page={current}
          covered={!front || carded}
          onpage={(p) => (current = p)}
          onhistory={(h) => (history = h)}
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
  .tools {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    background: var(--bg);
    border-bottom: 1px solid var(--line);
    font-size: 12px;
    color: var(--fg);
  }
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
  .page {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  button {
    font: inherit;
    color: var(--fg);
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 1px 7px;
  }
  button:hover:not(:disabled) {
    background: var(--chip);
  }
  button:disabled {
    opacity: 0.4;
  }
  button.icon {
    min-width: 24px;
    padding: 0 6px;
    font-size: 15px;
    line-height: 17px;
  }
  button.page {
    border-color: transparent;
  }
  input {
    font: inherit;
    color: var(--fg);
    background: var(--menu-bg);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 1px 6px;
    width: 130px;
    outline: none;
  }
  input.page-field {
    width: 44px;
    text-align: right;
  }
  input.missing {
    border-color: var(--error);
  }
  .viewer {
    background: var(--viewer);
    flex: 1;
    position: relative;
    min-height: 0;
  }
</style>
