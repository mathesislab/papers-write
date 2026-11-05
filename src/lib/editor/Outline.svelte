<script lang="ts">
  // The outline beside the text: the headings indented by their level, the one the cursor is under marked, a click
  // goes there. `headings` and `current` are the editor's (see outline.ts); what a click does is the owner's.

  import type { Heading } from "./outline";

  let { headings, current = -1, onselect }: { headings: readonly Heading[]; current?: number; onselect: (heading: Heading) => void } = $props();

  /** the levels count from the shallowest the document has */
  const top = $derived(headings.reduce((least, h) => Math.min(least, h.level), 6));
  let list: HTMLElement | undefined;

  // the heading marked stays in view as the cursor moves through a long list
  $effect(() => {
    if (current >= 0) list?.querySelector<HTMLElement>(`[data-index="${current}"]`)?.scrollIntoView({ block: "nearest" });
  });
</script>

<nav class="outline" aria-label="Outline" bind:this={list}>
  {#each headings as heading, i (i)}
    <button
      class:current={i === current}
      data-index={i}
      style:padding-left="{10 + (heading.level - top) * 12}px"
      title={heading.number ? `${heading.number} ${heading.title}` : heading.title}
      onclick={() => onselect(heading)}>{#if heading.number}<span class="number">{heading.number}</span>{/if}{heading.title}</button
    >
  {:else}
    <p>No headings</p>
  {/each}
</nav>

<style>
  .outline {
    flex: none;
    width: 210px;
    overflow-y: auto;
    padding: 8px 0 40px;
    border-right: 1px solid var(--line);
    font-family: var(--editor-font, -apple-system, system-ui, sans-serif);
    font-size: 12.5px;
  }
  button {
    display: block;
    width: 100%;
    font: inherit;
    text-align: left;
    color: var(--muted);
    background: none;
    border: 0;
    border-radius: 0;
    padding-block: 3px;
    padding-right: 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .number {
    opacity: 0.65;
    margin-right: 0.5em;
    font-variant-numeric: tabular-nums;
  }
  button:hover {
    background: var(--chip);
    color: var(--fg);
  }
  button.current {
    color: var(--fg);
    font-weight: 650;
    background: var(--selection);
  }
  p {
    margin: 0;
    padding: 4px 10px;
    color: var(--muted);
  }
</style>
