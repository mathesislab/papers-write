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
  import type { Heading } from "papers-write/editor";
  import OutlineTree from "$lib/OutlineTree.svelte";
  import { listen } from "$lib/events";
  import Search from "./Search.svelte";

  let {
    root,
    front,
    tab = $bindable("files"),
    folders = $bindable([]),
    headings,
    here,
    outlined,
    onheading,
    onsay,
    onclose,
  }: {
    /** the folder shown: the project's, or the document's */
    root: string;
    /** the document of the window */
    front: string | null;
    tab?: Tab;
    /** the folders open in the file tree */
    folders?: string[];
    headings: readonly Heading[];
    here: number;
    /** the document has headings to outline (Markdown, LaTeX) */
    outlined: boolean;
    onheading: (heading: Heading) => void;
    /** what was done or went wrong, for the foot of the window */
    onsay: (what: string) => void;
    /** the sidebar hidden (× of the search) */
    onclose: () => void;
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

  /** The outline of the whole document the text is part of (a thesis or book: its main document and chapters), as
   *  the app last read it (preview.rs `document_outline`); none: the text's own headings alone. */
  type Whole = { files: string[]; headings: { file: number; level: number; number: string; title: string }[] };
  let whole = $state.raw<Whole | null>(null);
  async function loadWhole() {
    whole = await invoke<Whole | null>("document_outline").catch(() => null);
  }
  $effect(() => {
    void front;
    void loadWhole();
  });
  onMount(() => {
    // the other chapters read anew (their table made again aside)
    const unlisten = listen("preview-labels", loadWhole);
    return () => void unlisten.then((f) => f());
  });

  /** The outline's lines: the whole document's headings, the text's own as it is typed (with the whole's numbers);
   *  else the text's headings. Each says where it goes: one of the text's, or the `nth` of its title in a file. */
  type Line = { level: number; title: string; number?: string; own?: number; file?: string; nth?: number };
  const lines = $derived.by<Line[]>(() => {
    const here = whole?.files.indexOf(front ?? "") ?? -1;
    if (!whole || here < 0) return headings.map((h, i) => ({ level: h.level, title: h.title, number: h.number, own: i }));
    const out: Line[] = [];
    whole.files.forEach((file, f) => {
      const theirs = whole!.headings.filter((h) => h.file === f);
      if (f === here) {
        headings.forEach((h, i) => out.push({ level: h.level, title: h.title, number: theirs[i]?.number || h.number, own: i }));
        return;
      }
      const seen = new Map<string, number>();
      for (const h of theirs) {
        const nth = seen.get(h.title) ?? 0;
        seen.set(h.title, nth + 1);
        out.push({ level: h.level, title: h.title, number: h.number || undefined, file, nth });
      }
    });
    return out;
  });
  const currentLine = $derived(here < 0 ? -1 : lines.findIndex((l) => l.own === here));

  /** A line chosen: the text's heading gone to, or the other file opened at the heading. */
  async function chosen(i: number) {
    const line = lines[i];
    if (line.own !== undefined) return onheading(headings[line.own]);
    if (!line.file) return;
    const at = await invoke<number | null>("heading_place", { path: line.file, title: line.title, nth: line.nth ?? 0 }).catch(() => null);
    await invoke("open_at", { path: line.file, at: at ?? 0, end: at ?? 0 }).catch((e) => onsay(String(e)));
  }

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
      bind:open={folders}
      onopen={open}
      onnew={make}
      onreveal={(path) => revealItemInDir(path).catch((e) => onsay(String(e)))}
      ondefault={(path) => openPath(path).catch((e) => onsay(String(e)))}
      ontrash={(node) => (trashing = node)}
    />
  </div>
  <div class="body" class:hidden={tab !== "outline"}>
    {#if outlined}
      <OutlineTree entries={lines} current={currentLine} onselect={chosen} />
    {:else}
      <p class="none">No outline for this file</p>
    {/if}
  </div>
  <div class="body search" class:hidden={tab !== "search"}>
    <Search bind:this={search} {root} onerror={onsay} {onclose} />
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
