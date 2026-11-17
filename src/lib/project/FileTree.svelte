<script lang="ts" module>
  /** A pill on a file's row (papers-review: the review file). */
  export type Badge = { label: string; title: string };
  /** An item of the app's own in a file's menu, above the usual ones. */
  export type Extra = { text: string; action: () => void };
</script>

<script lang="ts">
  // The files of a project as a tree (folders first; write_core::files), then the files linked from elsewhere (each
  // with the folder it is in), shared by papers-write's sidebar and papers-review's Files. A click opens a file
  // (`onopen`); folders open and close. Right-clicked, the project's folder and its folders make a Markdown file (named
  // in a row of their own) or take files in, when the app does those; a file or folder can be shown in the Finder,
  // opened in the default app, removed from the project (a link) or moved to the Trash. The app adds its own: a pill
  // on a row (`badge`), items atop a file's menu (`extra`).
  //
  // The keys, with the tree in front (a click on a row leaves it there): Up and Down go from row to row, Right and Left
  // open and close a folder, Enter opens the file (or folder), Delete moves it to the Trash (asked first; ⇧Delete
  // without asking: it is in the Trash still). A file
  // opened from the tree shows in a tab of its own, another page: that page's tree takes the keys back, on the file
  // (`FOCUS`, which the pages of an app share).

  import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount } from "svelte";
  import type { Kind, Node } from "./types";

  let {
    root,
    files,
    linked = [],
    front = null,
    open = $bindable([]),
    badge,
    extra,
    onopen,
    onnew,
    onadd,
    onreveal,
    ondefault,
    onunlink,
    ontrash,
  }: {
    /** the path of the project folder */
    root: string;
    files: Node[];
    linked?: Node[];
    /** the file in front */
    front?: string | null;
    /** the folders open (none at first) */
    open?: string[];
    badge?: (node: Node) => Badge | null;
    extra?: (node: Node) => Extra[];
    onopen: (node: Node) => void;
    /** a Markdown file of this name to make in a folder of the project */
    onnew?: (folder: string, name: string) => void;
    /** files, or folders, to add to a folder of the project */
    onadd?: (folder: string, directory: boolean) => void;
    /** a path to show in the Finder */
    onreveal: (path: string) => void;
    /** a file to open in the default app */
    ondefault: (path: string) => void;
    /** a linked file or folder to take out of the project (the file stays where it is) */
    onunlink?: (node: Node) => void;
    /** a file or folder of the project to move to the Trash; `now`: without asking first (⇧Delete) */
    ontrash?: (node: Node, now?: boolean) => void;
  } = $props();

  /** The file opened from the tree and when, for the page that shows it to give its tree the keys (localStorage:
   *  the pages share it). */
  const FOCUS = "file-tree-focus";

  /** the row the keys are on (a path), and the tree's element */
  let selected = $state<string | null>(null);
  let element = $state<HTMLElement>();

  /** Every node, by its path. */
  const byPath = $derived.by(() => {
    const map = new Map<string, Node>();
    const add = (nodes: Node[]) => {
      for (const n of nodes) {
        map.set(n.path, n);
        if (n.children) add(n.children);
      }
    };
    add(files);
    add(linked);
    return map;
  });

  /** The rows shown, in order (the closed folders' files are not). */
  const rows = () => [...(element?.querySelectorAll<HTMLElement>("button.item[data-path]") ?? [])];

  /** The tree given the keys on the row of `path` (the file in front if none). */
  export function focusTree(path = front) {
    selected = path ?? selected;
    const row = rows().find((r) => r.dataset.path === selected) ?? rows()[0];
    if (row) {
      selected = row.dataset.path ?? null;
      row.focus({ preventScroll: true });
      row.scrollIntoView({ block: "nearest" });
    } else element?.focus();
  }

  /** A file opened from the tree: noted for the page that will show it, then opened. */
  function openFromTree(node: Node) {
    selected = node.path;
    try {
      localStorage.setItem(FOCUS, JSON.stringify({ path: node.path, at: Date.now() }));
    } catch {}
    onopen(node);
  }

  /** The keys of the tree. */
  function keys(e: KeyboardEvent) {
    if (naming || e.altKey || e.ctrlKey || e.metaKey) return;
    const list = rows();
    const at = list.findIndex((r) => r.dataset.path === selected);
    const node = selected ? byPath.get(selected) : undefined;
    const go = (i: number) => {
      const row = list[Math.max(0, Math.min(list.length - 1, i))];
      if (!row) return;
      selected = row.dataset.path ?? null;
      row.focus({ preventScroll: true });
      row.scrollIntoView({ block: "nearest" });
    };
    if (e.key === "ArrowDown") go(at + 1);
    else if (e.key === "ArrowUp") go(at < 0 ? 0 : at - 1);
    else if (e.key === "Home") go(0);
    else if (e.key === "End") go(list.length - 1);
    else if (e.key === "ArrowRight" && node?.kind === "folder" && !opened.has(node.path) && node.children?.length) toggle(node);
    else if (e.key === "ArrowLeft" && node?.kind === "folder" && opened.has(node.path)) toggle(node);
    else if (e.key === "Enter" && node) node.kind === "folder" ? node.children?.length && toggle(node) : !node.missing && openFromTree(node);
    else if ((e.key === "Delete" || e.key === "Backspace") && node && ontrash && !isLinked(node.path)) {
      // the next row takes the keys, the one moved to the Trash going
      const next = list[at + 1] ?? list[at - 1];
      ontrash(node, e.shiftKey);
      if (next) selected = next.dataset.path ?? null;
    }
    else return;
    e.preventDefault();
  }

  /** Whether a path is of the linked files (not the project's: not moved to the Trash from here). */
  const isLinked = (path: string) => linked.some((n) => path === n.path || path.startsWith(n.path + "/"));

  // the page shows a file opened from the tree (here, or in another page's tree): its tree takes the keys back,
  // after the editor, which takes them as its text is read
  onMount(() => {
    const claim = () => {
      try {
        const asked = JSON.parse(localStorage.getItem(FOCUS) ?? "null") as { path: string; at: number } | null;
        if (!asked || asked.path !== front || Date.now() - asked.at > 4000) return;
        for (const wait of [0, 150, 500]) setTimeout(() => focusTree(asked.path), wait);
      } catch {}
    };
    claim();
    const unlisten = getCurrentWebview().listen<boolean>("tab-shown", (e) => e.payload && claim());
    window.addEventListener("focus", claim);
    return () => {
      unlisten.then((f) => f());
      window.removeEventListener("focus", claim);
    };
  });

  const item = (text: string, action: () => void) => MenuItem.new({ text, action });
  const separator = () => PredefinedMenuItem.new({ item: "Separator" });

  /** The tools of a folder of the project: a Markdown file made in it, files and folders added to it. */
  const folderItems = (folder: string) => [
    ...(onnew ? [item("New Markdown File…", () => start(folder))] : []),
    ...(onadd ? [item("Add Files…", () => onadd(folder, false)), item("Add Folder…", () => onadd(folder, true))] : []),
  ];

  /** A right-click on the project's folder. */
  async function folderMenu(e: MouseEvent) {
    e.preventDefault();
    const tools = folderItems(root);
    const items = await Promise.all([...tools, ...(tools.length ? [separator()] : []), item("Reveal in Finder", () => onreveal(root))]);
    await (await Menu.new({ items })).popup(new LogicalPosition(e.clientX, e.clientY));
  }

  /** the name of the project folder */
  const folder = $derived(root.slice(root.lastIndexOf("/") + 1));

  /** A right-click on a file or folder; `outside`: of the linked ones, `top`: at the top of its list. */
  async function menu(e: MouseEvent, node: Node, outside: boolean, top: boolean) {
    e.preventDefault();
    selected = node.path;
    const items = [];
    if (node.kind === "folder" && !outside) {
      const tools = folderItems(node.path);
      if (tools.length) items.push(...tools, separator());
    }
    const own = node.kind === "folder" ? [] : (extra?.(node) ?? []);
    if (own.length) items.push(...own.map((x) => item(x.text, x.action)));
    if (node.kind !== "folder" && !node.missing) items.push(item("Open in Default App", () => ondefault(node.path)));
    if (!node.missing) items.push(item("Reveal in Finder", () => onreveal(node.path)));
    if (outside && top && onunlink) items.push(separator(), item("Remove Link", () => onunlink(node)));
    if (!outside && ontrash) items.push(separator(), item("Move to Trash…", () => ontrash(node)));
    // at the pointer (without a place, it opens elsewhere in the window)
    await (await Menu.new({ items: await Promise.all(items) })).popup(new LogicalPosition(e.clientX, e.clientY));
  }

  /** the folder a Markdown file is being named in, and the name typed so far */
  let naming = $state<{ folder: string; name: string } | null>(null);

  /** Name a new file in a folder: the folder opens, a row with the name, its name but not its ending selected. */
  function start(folder: string) {
    if (!opened.has(folder)) open = [...open, folder];
    naming = { folder, name: "untitled.md" };
  }

  /** The name is made: Enter; the file is of Markdown, so `.md` is added when it has no Markdown extension
   *  (`.qmd` is Quarto's). */
  function commit() {
    if (!naming) return;
    const { folder, name } = naming;
    naming = null;
    const file = name.trim();
    if (file) onnew?.(folder, /\.(md|markdown|qmd)$/i.test(file) ? file : `${file}.md`);
  }

  function focusName(input: HTMLInputElement) {
    input.focus();
    input.setSelectionRange(0, input.value.lastIndexOf("."));
  }

  const opened = $derived(new Set(open));

  function toggle(folder: Node) {
    open = opened.has(folder.path) ? open.filter((p) => p !== folder.path) : [...open, folder.path];
  }

  /** The folder holding a path, with the home folder as ~ (empty for a path at the root). */
  function parent(path: string): string {
    const dir = path.slice(0, path.lastIndexOf("/"));
    return dir.replace(/^\/Users\/[^/]+/, "~");
  }

  const LABELS: Record<Kind, string> = {
    folder: "",
    pdf: "PDF",
    markdown: "MD",
    latex: "TEX",
    text: "TXT",
    bib: "BIB",
    image: "IMG",
    other: "",
  };
</script>

{#snippet chevron(open: boolean)}
  <svg class="chevron" class:open viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
    <path d="M4 2.25 7.75 6 4 9.75" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
  </svg>
{/snippet}

{#snippet folderIcon()}
  <svg class="glyph" viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
    <path
      d="M1.75 4.5A1.25 1.25 0 0 1 3 3.25h2.9c.33 0 .64.14.86.38l.78.87h4.46A1.25 1.25 0 0 1 13.25 5.75v5.5A1.25 1.25 0 0 1 12 12.5H3a1.25 1.25 0 0 1-1.25-1.25z"
      fill="currentColor"
      opacity="0.85"
    />
  </svg>
{/snippet}

{#snippet linkIcon()}
  <svg class="glyph link" viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
    <path
      d="M6.75 9.25a2.5 2.5 0 0 0 3.54 0l2.2-2.2a2.5 2.5 0 0 0-3.54-3.54l-.6.6M9.25 6.75a2.5 2.5 0 0 0-3.54 0l-2.2 2.2a2.5 2.5 0 0 0 3.54 3.54l.6-.6"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
    />
  </svg>
{/snippet}

<!-- The row where a new file is named, in the folder it is made in: Enter makes it, Esc (or leaving the field) does not. -->
{#snippet nameRow()}
  {#if naming}
    <div class="item naming">
      <span class="disclosure"></span>
      <span class="kind markdown">MD</span>
      <input
        bind:value={naming.name}
        use:focusName
        autocomplete="off"
        autocorrect="off"
        spellcheck="false"
        onkeydown={(e) => {
          if (e.key === "Enter") commit();
          if (e.key === "Escape") naming = null;
        }}
        onblur={() => (naming = null)}
      />
    </div>
  {/if}
{/snippet}

<!-- The nodes at one level; a level in a folder sits in a box that draws the guide line under its folder. A linked
     node at the top is marked with a link glyph and the folder it comes from. -->
{#snippet tree(nodes: Node[], depth: number, outside: boolean)}
  {#each nodes as node (node.path)}
    {@const from = outside && depth === 0 ? parent(node.path) : ""}
    {#if node.kind === "folder"}
      {@const empty = !node.children?.length}
      {@const unfolded = (!empty && opened.has(node.path)) || naming?.folder === node.path}
      <button
        class="item"
        class:empty
        class:selected={selected === node.path}
        data-path={node.path}
        onclick={() => ((selected = node.path), !empty && toggle(node))}
        oncontextmenu={(e) => menu(e, node, outside, depth === 0)}
        title={node.path}
      >
        <span class="disclosure">{#if !empty}{@render chevron(unfolded)}{/if}</span>
        {@render folderIcon()}
        {#if from}{@render linkIcon()}{/if}
        <span class="name">{node.name}</span>
        {#if from}<span class="from">{from}</span>{/if}
      </button>
      {#if unfolded}
        <div class="children" style:--depth={depth + 1}>
          {#if naming?.folder === node.path}{@render nameRow()}{/if}
          {#if node.children}{@render tree(node.children, depth + 1, outside)}{/if}
        </div>
      {/if}
    {:else}
      <button
        class="item"
        class:front={node.path === front}
        class:missing={node.missing}
        class:selected={selected === node.path}
        data-path={node.path}
        onclick={() => !node.missing && openFromTree(node)}
        oncontextmenu={(e) => menu(e, node, outside, depth === 0)}
        aria-disabled={node.missing}
        title={node.missing ? `${node.path}: not found` : node.path}
      >
        <span class="disclosure"></span>
        <span class="kind {node.kind}">{LABELS[node.kind]}</span>
        {#if from}{@render linkIcon()}{/if}
        <span class="name">{node.name}</span>
        {#if from}<span class="from">{from}</span>{/if}
        {#if badge?.(node)}{@const mark = badge(node)!}<span class="badge" title={mark.title}>{mark.label}</span>{/if}
      </button>
    {/if}
  {/each}
{/snippet}

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="tree" bind:this={element} tabindex="-1" onkeydown={keys} role="tree">
  <h4>Folder</h4>
  <div class="root" title={root} oncontextmenu={folderMenu} role="presentation">
    {@render folderIcon()}
    <span class="name">{folder}</span>
  </div>
  <div class="level" style:--depth={0}>
    {#if naming?.folder === root}{@render nameRow()}{/if}
    {@render tree(files, 0, false)}
  </div>
  {#if linked.length}
    <h4>Linked</h4>
    <div class="level" style:--depth={0}>
      {@render tree(linked, 0, true)}
    </div>
  {/if}
</div>

<style>
  .tree {
    padding: 0 6px 10px;
  }
  /* the project's folder, above its tree */
  .root {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 3px;
    padding: 3px 8px;
    font-weight: 600;
    white-space: nowrap;
  }
  .root .glyph {
    color: var(--accent);
  }
  h4 {
    margin: 12px 8px 3px;
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    font: inherit;
    color: inherit;
    text-align: left;
    background: none;
    border: none;
    border-radius: 5px;
    padding: 3px 8px 3px calc(8px + var(--depth) * 14px);
    white-space: nowrap;
  }
  .item:hover:not(.missing, .naming) {
    background: var(--chip);
  }
  /* the row the keys are on */
  .item.selected {
    outline: 1px solid color-mix(in srgb, var(--accent) 60%, transparent);
    outline-offset: -1px;
  }
  .tree:focus {
    outline: none;
  }
  .item.front {
    background: var(--selection);
  }
  .item.missing {
    opacity: 0.5;
  }
  .naming input {
    flex: 1;
    min-width: 0;
    padding: 1px 5px;
    border: 1px solid var(--accent);
    border-radius: 4px;
    background: var(--bg);
    color: var(--fg);
    font: inherit;
    outline: none;
    -webkit-user-select: text;
    user-select: text;
  }
  /* a folder's contents: a guide line under the folder's chevron spans them */
  .children {
    position: relative;
  }
  .children::before {
    content: "";
    position: absolute;
    top: 0;
    bottom: 2px;
    left: calc(8px + 9px + (var(--depth) - 1) * 14px - 0.5px);
    border-left: 1px solid var(--line);
    pointer-events: none;
  }
  .disclosure {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    flex: none;
    border-radius: 4px;
    color: var(--muted);
  }
  .item:hover:not(.missing) .disclosure:not(:empty) {
    background: color-mix(in srgb, var(--fg) 12%, transparent);
    color: var(--fg);
  }
  .chevron {
    transition: transform 0.12s ease;
  }
  .chevron.open {
    transform: rotate(90deg);
  }
  .glyph {
    flex: none;
    color: var(--muted);
  }
  .item.empty .name {
    color: var(--muted);
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* the folder a linked node is in, dimmed and cut short first */
  .from {
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--muted);
    opacity: 0.75;
    font-size: 11px;
  }
  /* the app's pill on a file's row (papers-review: the review) */
  .badge {
    flex: none;
    margin-left: auto;
    padding: 1px 6px;
    border-radius: 8px;
    background: var(--selection);
    color: var(--accent);
    font-size: 10px;
    font-weight: 600;
    line-height: 1.4;
  }
  .kind {
    flex: none;
    width: 26px;
    font: 600 8.5px/1 -apple-system, sans-serif;
    padding: 3px 0;
    border-radius: 3px;
    text-align: center;
    color: #fff;
    background: #9a9aa2;
  }
  .kind:empty {
    background: none;
  }
  .kind.pdf {
    background: #d04545;
  }
  .kind.markdown {
    background: #4a76d6;
  }
  .kind.bib {
    background: #2f9e6e;
  }
  .kind.latex {
    background: #3d8c8c;
  }
  .kind.image {
    background: #a0659e;
  }
</style>
