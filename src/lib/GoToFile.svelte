<script lang="ts">
  // Go to File (⌘P), as Sublime's Goto Anything: a field over the page. What is typed finds a file of the project
  // (its letters in order, the file's name first: fuzzy.ts), the most recently changed first while nothing is typed;
  // `@` the headings of the text, `:` a line of it. Up and Down go through what is found, Enter goes there, Esc
  // closes.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { overPage } from "$lib/overlay.svelte";
  import { matchIn, pieces, rank } from "$lib/fuzzy";
  import type { Heading } from "papers-write/editor";

  type Listed = { path: string; relative: string; modified: number };

  let {
    headings,
    onheading,
    online,
    onclose,
    start = "",
  }: {
    /** the headings of the text in front (none: not a Markdown or LaTeX text) */
    headings: Heading[];
    onheading: (heading: Heading) => void;
    online: (line: number) => void;
    onclose: () => void;
    /** what the field starts with: `:` for Go to Line (Ctrl+G) */
    start?: string;
  } = $props();

  // svelte-ignore state_referenced_locally
  let query = $state(start);
  let files = $state.raw<Listed[]>([]);
  let loading = $state(true);
  let at = $state(0);
  let error = $state("");
  let field = $state<HTMLInputElement>();
  let list = $state<HTMLElement>();

  type Row = { kind: "file"; file: Listed; at: number[] } | { kind: "heading"; heading: Heading; at: number[] } | { kind: "line"; line: number };

  const rows = $derived.by<Row[]>(() => {
    if (query.startsWith(":")) {
      const line = Number.parseInt(query.slice(1), 10);
      return Number.isFinite(line) && line > 0 ? [{ kind: "line", line }] : [];
    }
    if (query.startsWith("@")) {
      const q = query.slice(1).trim().toLowerCase();
      return headings
        .map((heading) => ({ heading, ...matchIn(q, heading.title.toLowerCase()) }))
        .filter((m) => m.score > 0)
        .sort((a, b) => (q ? b.score - a.score : 0))
        .map((m) => ({ kind: "heading", heading: m.heading, at: m.at }));
    }
    return rank(files, query, 200).map((m) => ({ kind: "file", file: m.item, at: m.at }));
  });

  $effect(() => {
    void query;
    at = 0;
  });

  async function go(row: Row | undefined) {
    if (!row) return;
    if (row.kind === "line") return void (online(row.line), onclose());
    if (row.kind === "heading") return void (onheading(row.heading), onclose());
    try {
      await invoke("open_document", { path: row.file.path });
      onclose();
    } catch (e) {
      error = String(e);
    }
  }

  function keys(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
    else if (e.key === "Enter") void go(rows[at]);
    else if (e.key === "ArrowDown") at = Math.min(rows.length - 1, at + 1);
    else if (e.key === "ArrowUp") at = Math.max(0, at - 1);
    else return;
    e.preventDefault();
    queueMicrotask(() => list?.querySelector(".on")?.scrollIntoView({ block: "nearest" }));
  }

  /** `text` in pieces, those at `at` (from `from` on in the whole path) marked. */
  const split = (relative: string) => {
    const cut = Math.max(relative.lastIndexOf("/"), relative.lastIndexOf("\\")) + 1;
    return { folder: relative.slice(0, cut), name: relative.slice(cut), cut };
  };

  onMount(() => {
    field?.focus();
    field?.setSelectionRange(query.length, query.length);
    invoke<Listed[]>("project_file_list")
      .then((found) => (files = found))
      .catch((e) => (error = String(e)))
      .finally(() => (loading = false));
  });
  /** The focus gone to another place of the page (the text): closed, never shown with the cursor elsewhere (the
   *  window left for another app, it stays). */
  function leaving(e: FocusEvent) {
    const to = e.relatedTarget as Node | null;
    if (to && !(e.currentTarget as HTMLElement).contains(to)) onclose();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="veil" onclick={onclose} {@attach overPage}>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="sheet"
    role="dialog"
    aria-label="Go to File"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onfocusout={(e) => leaving(e)}>
    <input
      bind:this={field}
      bind:value={query}
      onkeydown={keys}
      placeholder="Go to a file of the project; @ a heading, : a line"
      spellcheck="false"
      autocomplete="off"
    />
    {#if error}<p class="error">{error}</p>{/if}
    <ul bind:this={list}>
      {#each rows as row, i (row.kind === "file" ? row.file.path : row.kind === "heading" ? `@${row.heading.from}` : ":")}
        <li class:on={i === at}>
          <button onclick={() => go(row)} onmousemove={() => (at = i)}>
            {#if row.kind === "file"}
              {@const p = split(row.file.relative)}
              <span class="name">{#each pieces(p.name, row.at, p.cut) as piece, k (k)}{#if piece.on}<b>{piece.text}</b>{:else}{piece.text}{/if}{/each}</span>
              {#if p.folder}<span class="folder">{#each pieces(p.folder, row.at) as piece, k (k)}{#if piece.on}<b>{piece.text}</b>{:else}{piece.text}{/if}{/each}</span>{/if}
            {:else if row.kind === "heading"}
              <span class="name" style:padding-left="{(row.heading.level - 1) * 1.1}em"
                >{#each pieces(row.heading.title, row.at) as piece, k (k)}{#if piece.on}<b>{piece.text}</b>{:else}{piece.text}{/if}{/each}</span
              >
            {:else}
              <span class="name">Go to line {row.line}</span>
            {/if}
          </button>
        </li>
      {:else}
        <li class="none">
          {#if query.startsWith("@")}{headings.length ? "No heading matches" : "This text has no headings"}
          {:else if query.startsWith(":")}Type a line number
          {:else if loading}Listing the project's files…
          {:else if files.length}No file matches
          {:else}No files: this text is in no project{/if}
        </li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .veil {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 8vh;
    background: color-mix(in srgb, #000 22%, transparent);
  }
  .sheet {
    width: min(640px, 92vw);
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.3);
    overflow: hidden;
  }
  input {
    margin: 10px;
    font: inherit;
    font-size: 15px;
    padding: 6px 10px;
    color: var(--fg);
    background: var(--sidebar);
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0 4px 4px;
    overflow: auto;
    border-top: 1px solid var(--line);
  }
  li button {
    display: flex;
    align-items: baseline;
    gap: 8px;
    width: 100%;
    padding: 5px 10px;
    text-align: left;
    font: inherit;
    color: var(--fg);
    background: none;
    border: none;
    border-radius: 6px;
    white-space: nowrap;
    overflow: hidden;
  }
  li.on button {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .name {
    flex: none;
  }
  .folder {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 12px;
    color: var(--muted);
  }
  b {
    font-weight: 700;
    color: var(--accent);
  }
  .none {
    padding: 12px;
    font-size: 12px;
    color: var(--muted);
  }
  .error {
    margin: 0 12px 8px;
    color: var(--error);
  }
</style>
