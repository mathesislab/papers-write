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

  import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
  import type { Kind, Node } from "./types";

  let {
    root,
    files,
    linked = [],
    front = null,
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
    /** a file or folder of the project to move to the Trash */
    ontrash?: (node: Node) => void;
  } = $props();

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
    await (await Menu.new({ items })).popup();
  }

  /** the name of the project folder */
  const folder = $derived(root.slice(root.lastIndexOf("/") + 1));

  /** A right-click on a file or folder; `outside`: of the linked ones, `top`: at the top of its list. */
  async function menu(e: MouseEvent, node: Node, outside: boolean, top: boolean) {
    e.preventDefault();
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
    await (await Menu.new({ items: await Promise.all(items) })).popup();
  }

  /** the folder a Markdown file is being named in, and the name typed so far */
  let naming = $state<{ folder: string; name: string } | null>(null);

  /** Name a new file in a folder: the folder opens, a row with the name, its name but not its ending selected. */
  function start(folder: string) {
    const next = new Set(closed);
    next.delete(folder);
    closed = next;
    naming = { folder, name: "untitled.md" };
  }

  /** The name is made: Enter; the file is of Markdown, so `.md` is added when it is not there. */
  function commit() {
    if (!naming) return;
    const { folder, name } = naming;
    naming = null;
    const file = name.trim();
    if (file) onnew?.(folder, /\.md$/i.test(file) ? file : `${file}.md`);
  }

  function focusName(input: HTMLInputElement) {
    input.focus();
    input.setSelectionRange(0, input.value.lastIndexOf("."));
  }

  /** the folders closed (all open at first) */
  let closed = $state(new Set<string>());

  function toggle(folder: Node) {
    const next = new Set(closed);
    if (next.has(folder.path)) next.delete(folder.path);
    else next.add(folder.path);
    closed = next;
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
      {@const open = (!empty && !closed.has(node.path)) || naming?.folder === node.path}
      <button
        class="item"
        class:empty
        onclick={() => !empty && toggle(node)}
        oncontextmenu={(e) => menu(e, node, outside, depth === 0)}
        title={node.path}
      >
        <span class="disclosure">{#if !empty}{@render chevron(open)}{/if}</span>
        {@render folderIcon()}
        {#if from}{@render linkIcon()}{/if}
        <span class="name">{node.name}</span>
        {#if from}<span class="from">{from}</span>{/if}
      </button>
      {#if open}
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
        onclick={() => !node.missing && onopen(node)}
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

<div class="tree">
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
