<script lang="ts">
  // The symbols of the review, listed over its editor (⌘/): the sequences typed and the LaTeX names, each with the
  // symbol it gives. Typing filters them; Enter or a click puts the one marked in at the cursor; Esc closes the list.

  import { onMount } from "svelte";
  import { find, type Entry } from "$lib/symbols";

  let {
    entries,
    oninsert,
    onclose,
  }: {
    entries: readonly Entry[];
    /** the symbol chosen */
    oninsert: (symbol: string) => void;
    onclose: () => void;
  } = $props();

  let query = $state("");
  /** the entry marked, by its place in what is found */
  let marked = $state(0);
  let input: HTMLInputElement;
  let list: HTMLElement;
  const found = $derived(find(entries, query));

  function key(e: KeyboardEvent) {
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (step) {
      marked = Math.max(0, Math.min(found.length - 1, marked + step));
      list.children[marked]?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      if (found[marked]) oninsert(found[marked].insert);
    } else if (e.key === "Escape") {
      onclose();
    } else {
      return;
    }
    e.preventDefault();
    e.stopPropagation();
  }

  /** Focus leaving the list closes it, but not for a click in it. */
  function left(e: FocusEvent) {
    if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node | null)) onclose();
  }

  onMount(() => input.focus());
</script>

<div class="symbols" onfocusout={left} role="dialog" aria-label="Symbols" tabindex="-1">
  <input
    bind:this={input}
    bind:value={query}
    oninput={() => (marked = 0)}
    onkeydown={key}
    placeholder="Find a symbol"
    spellcheck="false"
    autocomplete="off"
  />
  <ul bind:this={list}>
    {#each found as entry, i (entry.keys)}
      <!-- mousedown kept from the input: the click follows -->
      <li class:on={i === marked} onmousedown={(e) => e.preventDefault()} onclick={() => oninsert(entry.insert)} onmousemove={() => (marked = i)} role="presentation">
        <span class="symbol">{entry.insert}</span>
        <code>{entry.keys}</code>
      </li>
    {:else}
      <li class="none">Nothing found</li>
    {/each}
  </ul>
  <p>In the text, Tab completes a \name.</p>
</div>

<style>
  .symbols {
    position: absolute;
    z-index: 5;
    top: 8px;
    right: 14px;
    width: 260px;
    max-height: calc(100% - 16px);
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 4px 18px rgba(0, 0, 0, 0.2);
    overflow: hidden;
  }
  input {
    flex: none;
    font: inherit;
    color: var(--fg);
    background: var(--chip);
    border: none;
    border-bottom: 1px solid var(--line);
    padding: 6px 10px;
    outline: none;
    -webkit-user-select: text;
    user-select: text;
  }
  ul {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 3px;
    list-style: none;
    overflow-y: auto;
  }
  li {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 2px 8px;
    border-radius: 5px;
  }
  li.on {
    background: var(--selection);
  }
  .symbol {
    flex: none;
    width: 2.2em;
    text-align: center;
    font-size: 15px;
  }
  code {
    color: var(--muted);
    font: 12px ui-monospace, Menlo, monospace;
  }
  .none {
    color: var(--muted);
  }
  p {
    flex: none;
    margin: 0;
    padding: 4px 10px;
    color: var(--muted);
    font-size: 11.5px;
    border-top: 1px solid var(--line);
  }
</style>
