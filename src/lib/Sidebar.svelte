<script lang="ts" module>
  export type Tab = "files" | "outline" | "search";
</script>

<script lang="ts">
  // The sidebar at the left of a document window, a tab each: the files of the project, or of the document's folder
  // outside a project (FileTree of papers-write/project; a click opens a text file or PDF in a tab, anything else in
  // its default app); the outline of the document (the editor's headings); the search of the project (Search). Shown
  // and hidden by a click on the project's tile and View ▸ Show Sidebar (⌥⌘S); ⌥⌘O shows the outline, ⇧⌘F the search.
  // The files are read again when the window gets the focus, and after what the tree did.

  import { invoke } from "@tauri-apps/api/core";
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import { FileTree, type Node } from "papers-write/project";
  import { Outline, type Heading } from "papers-write/editor";
  import Search from "./Search.svelte";

  let {
    root,
    front,
    tab = $bindable("files"),
    headings,
    here,
    outlined,
    onheading,
    onsay,
  }: {
    /** the folder shown: the project's, or the document's */
    root: string;
    /** the document of the window */
    front: string | null;
    tab?: Tab;
    headings: readonly Heading[];
    here: number;
    /** the document has headings to outline (Markdown, LaTeX) */
    outlined: boolean;
    onheading: (heading: Heading) => void;
    /** what was done or went wrong, for the foot of the window */
    onsay: (what: string) => void;
  } = $props();

  let files = $state.raw<Node[]>([]);
  let search = $state<Search>();
  /** the file waiting for a yes to go to the Trash */
  let trashing = $state<Node | null>(null);

  async function refresh() {
    try {
      files = await invoke<Node[]>("project_files", { root });
    } catch (e) {
      onsay(String(e));
    }
  }

  $effect(() => {
    root;
    refresh();
  });

  /** The search's field, its tab shown. */
  export function find() {
    tab = "search";
    queueMicrotask(() => search?.focus());
  }

  const OPENED = new Set(["markdown", "latex", "text", "bib", "pdf"]);

  function open(node: Node) {
    const done = OPENED.has(node.kind) ? invoke("open_document", { path: node.path }) : openPath(node.path);
    done.catch((e) => onsay(String(e)));
  }

  async function make(folder: string, name: string) {
    try {
      await invoke("new_markdown_file", { folder, name });
    } catch (e) {
      onsay(String(e));
    }
    refresh();
  }

  async function trash(node: Node) {
    trashing = null;
    try {
      await invoke("trash_path", { path: node.path });
      onsay(`${node.name} moved to the Trash`);
    } catch (e) {
      onsay(String(e));
    }
    refresh();
  }

  onMount(() => {
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  });

  const TABS: [Tab, string][] = [
    ["files", "Files"],
    ["outline", "Outline"],
    ["search", "Search"],
  ];
</script>

<aside>
  <div class="tabs" role="tablist">
    {#each TABS as [id, name] (id)}
      <button role="tab" aria-selected={tab === id} class:on={tab === id} onclick={() => (id === "search" ? find() : (tab = id))}>{name}</button>
    {/each}
  </div>
  <div class="body" class:hidden={tab !== "files"}>
    {#if trashing}
      {@const node = trashing}
      <div class="confirm">
        <span>Move <b>{node.name}</b> to the Trash?</span>
        <span class="buttons">
          <button onclick={() => (trashing = null)}>Cancel</button>
          <button class="push" onclick={() => trash(node)}>Move to Trash</button>
        </span>
      </div>
    {/if}
    <FileTree
      {root}
      {files}
      {front}
      onopen={open}
      onnew={make}
      onreveal={(path) => revealItemInDir(path).catch((e) => onsay(String(e)))}
      ondefault={(path) => openPath(path).catch((e) => onsay(String(e)))}
      ontrash={(node) => (trashing = node)}
    />
  </div>
  <div class="body" class:hidden={tab !== "outline"}>
    {#if outlined}
      <Outline {headings} current={here} onselect={onheading} />
    {:else}
      <p class="none">No outline for this file</p>
    {/if}
  </div>
  <div class="body search" class:hidden={tab !== "search"}>
    <Search bind:this={search} {root} onerror={onsay} />
  </div>
</aside>

<style>
  aside {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    background: var(--sidebar);
    border-right: 1px solid var(--line);
    font-size: 12.5px;
  }
  .tabs {
    flex: none;
    display: flex;
    gap: 2px;
    padding: 6px 6px 5px;
    border-bottom: 1px solid var(--line);
  }
  .tabs button {
    flex: 1;
    font: inherit;
    font-size: 11.5px;
    color: var(--muted);
    background: none;
    border: none;
    border-radius: 5px;
    padding: 2px 0;
  }
  .tabs button:hover {
    color: var(--fg);
  }
  .tabs button.on {
    color: var(--fg);
    background: var(--chip);
    font-weight: 600;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .body.search {
    overflow: hidden;
  }
  .hidden {
    display: none;
  }
  .none {
    margin: 0;
    padding: 10px;
    color: var(--muted);
  }
  .confirm {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 8px 6px 0;
    padding: 8px 10px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
  .confirm button {
    font: inherit;
    color: var(--fg);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 1px 8px;
  }
  .confirm .push {
    color: #fff;
    background: var(--error);
    border-color: transparent;
  }
</style>
