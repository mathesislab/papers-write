<script lang="ts">
  // The page of a window that shows a PDF (PDFKit): its tools are the page in view (and the field to go to one), the
  // zoom (⌘= ⌘- ⌘0, the PDF menu) and Find (⌘F, ⌘G; the menu says them as `pdf`). The PDF is read again, at the same
  // page, when it changes on disk (`file-changed`: a build writes it, in several passes: a moment after the last).

  import { invoke } from "@tauri-apps/api/core";
  import { onMount, tick } from "svelte";
  import PdfView from "$lib/PdfView.svelte";
  import { listen } from "$lib/events";
  import { Tabs, part, ready } from "papers-write/project";
  import "$lib/theme.css";

  let path = $state<string | null | undefined>();
  let view = $state<PdfView>();
  let count = $state(0);
  let current = $state(1);
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

  onMount(() => {
    ready(opened.promise);
    ask();
    const unlisten = [listen("opened", ask), listen("file-changed", changed), listen<string>("pdf", (e) => command(e.payload)), listen<boolean>("tab-shown", (e) => (front = e.payload)), listen<string>("find", (e) => command(e.payload))];
    return () => {
      clearTimeout(timer);
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });
</script>

<main>
  <!-- the tabs of the window (nothing for a tab alone) -->
  <div class="tabrow"><Tabs /></div>
  <div class="tools">
    <span class="name" title={path ?? ""}>{name}</span>
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
      <button class="page" onclick={() => (typing = String(current))} title="Go to Page">Page {current} of {count}</button>
    {/if}
    <button onclick={() => view?.zoom(-1)} title="Zoom Out (⌘-)">−</button>
    <button onclick={() => view?.zoom(0)} title="Fit Width (⌘0)">Fit</button>
    <button onclick={() => view?.zoom(1)} title="Zoom In (⌘=)">+</button>
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
      <button onclick={startFind} title="Find (⌘F)">Find</button>
    {/if}
  </div>
  <div class="viewer">
    {#if path}
      {#key `${path}:${reads}`}
        <PdfView
          bind:this={view}
          {path}
          page={current}
          covered={!front}
          onpage={(p) => (current = p)}
          onopen={(c) => {
            count = c;
            current = Math.min(current, c);
            opened.done();
          }}
        />
      {/key}
    {/if}
  </div>
</main>

<style>
  main {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--viewer);
  }
  .tabrow {
    padding: 5px 8px;
    background: var(--bg);
    border-bottom: 1px solid var(--line);
  }
  .tabrow:empty {
    display: none;
  }
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
  button:hover {
    background: var(--chip);
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
    flex: 1;
    position: relative;
    min-height: 0;
  }
</style>
