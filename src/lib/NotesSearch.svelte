<script lang="ts">
  // The search of a folder of notes (a project of kind `notes`), over the page: a field, and the notes it finds, the
  // best first (by title, then file name, then text; notes.rs), each with its title, its name and the line the query
  // is found in. Without a query, all the notes, the most recently changed first, or by title (the toggle). Up and
  // Down go through them, Enter opens one, ⌘Enter (or New Note) makes a note of the query as its title, Esc closes.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  let { name, onclose }: { name: string; onclose: () => void } = $props();

  type Note = { path: string; name: string; title: string; modified: number; score: number; line: string | null };

  let query = $state("");
  let notes = $state.raw<Note[]>([]);
  let at = $state(0);
  let alphabetical = $state(false);
  let error = $state("");
  let field = $state<HTMLInputElement>();
  let list = $state<HTMLElement>();
  let timer: ReturnType<typeof setTimeout> | undefined;

  /** The notes as listed: by score with a query; else by date, or by title. */
  const shown = $derived(
    !query.trim() && alphabetical ? [...notes].sort((a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: "base" })) : notes,
  );

  async function search() {
    try {
      notes = await invoke<Note[]>("search_notes", { query });
      at = 0;
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  function later() {
    clearTimeout(timer);
    timer = setTimeout(search, 120);
  }

  async function open(note: Note) {
    try {
      await invoke("open_document", { path: note.path });
      onclose();
    } catch (e) {
      error = String(e);
    }
  }

  async function make() {
    try {
      await invoke("new_note", { title: query.trim() });
      onclose();
    } catch (e) {
      error = String(e);
    }
  }

  function keys(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
    else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) void make();
    else if (e.key === "Enter" && shown[at]) void open(shown[at]);
    else if (e.key === "ArrowDown") at = Math.min(shown.length - 1, at + 1);
    else if (e.key === "ArrowUp") at = Math.max(0, at - 1);
    else return;
    e.preventDefault();
    queueMicrotask(() => list?.querySelector(".on")?.scrollIntoView({ block: "nearest" }));
  }

  const ago = (seconds: number) => {
    const days = Math.floor((Date.now() / 1000 - seconds) / 86400);
    return days <= 0 ? "today" : days === 1 ? "yesterday" : days < 30 ? `${days} days ago` : new Date(seconds * 1000).toLocaleDateString();
  };

  onMount(() => {
    search();
    field?.focus();
    return () => clearTimeout(timer);
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="veil" onclick={onclose}>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="sheet" role="dialog" aria-label="Notes" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <header>
      <input bind:this={field} bind:value={query} oninput={later} onkeydown={keys} placeholder="Find a note in {name}, or name a new one" spellcheck="false" autocomplete="off" />
      <button onclick={make} title="A new note, the text typed as its title ({navigator.userAgent.includes('Mac') ? '⌘' : 'Ctrl+'}Enter)">New Note</button>
      <button class="order" onclick={() => (alphabetical = !alphabetical)} disabled={!!query.trim()} title="Without a query: the most recent first, or by title">
        {alphabetical ? "A–Z" : "Recent"}
      </button>
    </header>
    {#if error}<p class="error">{error}</p>{/if}
    <ul bind:this={list}>
      {#each shown as note, i (note.path)}
        <li class:on={i === at}>
          <button onclick={() => open(note)} onmouseenter={() => (at = i)}>
            <span class="title">{note.title}</span>
            <span class="meta">{note.name} · {ago(note.modified)}</span>
            {#if note.line && query.trim()}<span class="line">{note.line}</span>{/if}
          </button>
        </li>
      {:else}
        <li class="none">{query.trim() ? "No note matches: ⌘Enter makes one of that title" : "No notes yet: type a title and press ⌘Enter"}</li>
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
    width: min(720px, 92vw);
    max-height: 76vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.3);
    overflow: hidden;
  }
  header {
    display: flex;
    gap: 6px;
    padding: 10px;
    border-bottom: 1px solid var(--line);
  }
  input {
    flex: 1;
    font: inherit;
    font-size: 15px;
    padding: 6px 10px;
    color: var(--fg);
    background: var(--sidebar);
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  header button {
    font: inherit;
    padding: 0 10px;
    color: var(--fg);
    background: var(--chip);
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 4px;
    overflow: auto;
  }
  li button {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 6px 10px;
    text-align: left;
    font: inherit;
    color: var(--fg);
    background: none;
    border: none;
    border-radius: 6px;
  }
  li.on button {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .title {
    font-weight: 600;
  }
  .meta,
  .none,
  .line {
    font-size: 12px;
    color: var(--muted);
  }
  .line {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .none {
    padding: 12px;
  }
  .error {
    margin: 8px 12px;
    color: var(--error);
  }
</style>
