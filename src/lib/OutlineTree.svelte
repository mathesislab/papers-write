<script lang="ts">
  // The outline as a tree that opens around where one is: the top-level headings always there, the heading of what
  // shows (`current`: the text's view, as one reads or scrolls) with the path to it opened, then, while the list stays within a budget (what fits in the
  // sidebar, at most MOST lines), the headings around it opened too; the rest folded (▸). It follows the text: as the
  // cursor goes to another heading, the tree opens around it again. A click on ▸/▾ opens or folds by hand (till the
  // cursor goes elsewhere), on a title goes there. Keys, as the file tree's: Up and Down, Right opens (or goes in),
  // Left folds (or goes to the parent), Enter goes there.

  import { untrack } from "svelte";

  type Entry = { level: number; title: string; number?: string };

  let { entries, current = -1, onselect }: { entries: readonly Entry[]; current?: number; onselect: (i: number) => void } = $props();

  /** the most lines the tree shows by itself (more by hand) */
  const MOST = 15;
  const LINE = 22;

  let nav = $state<HTMLElement>();
  let height = $state(400);
  let open = $state(new Set<number>());
  let focused = $state(-1);

  /** each heading's parent (the nearest heading before it of a lesser level), and whether it has headings under it */
  const parents = $derived.by(() => {
    const stack: number[] = [];
    return entries.map((e, i) => {
      while (stack.length && entries[stack[stack.length - 1]].level >= e.level) stack.pop();
      const p = stack.length ? stack[stack.length - 1] : -1;
      stack.push(i);
      return p;
    });
  });
  const hasChildren = $derived(entries.map((_, i) => parents.indexOf(i) !== -1));
  const top = $derived(entries.reduce((least, h) => Math.min(least, h.level), 6));

  const ancestors = (i: number) => {
    const out: number[] = [];
    for (let p = parents[i]; p !== -1 && p !== undefined; p = parents[p]) out.push(p);
    return out;
  };
  const visibleWith = (set: Set<number>) => entries.map((_, i) => ancestors(i).every((a) => set.has(a)));
  const countWith = (set: Set<number>) => visibleWith(set).filter(Boolean).length;
  const shown = $derived(visibleWith(open));

  /** The tree opened around heading `c`: the path to it, it, then the headings nearest to it while the budget allows. */
  function around(c: number): Set<number> {
    const budget = Math.max(5, Math.min(MOST, Math.floor(height / LINE)));
    const set = new Set<number>(c >= 0 ? [...ancestors(c), c] : []);
    if (c < 0) return set;
    // nearest first: its siblings, by their distance from it; then its parent's siblings; and so on up
    const near: number[] = [];
    for (let at = c; at !== -1; at = parents[at]) {
      const sibs = entries.map((_, i) => i).filter((i) => parents[i] === parents[at] && i !== at && hasChildren[i]);
      near.push(...sibs.sort((a, b) => Math.abs(a - at) - Math.abs(b - at)));
    }
    for (const n of near) {
      set.add(n);
      if (countWith(set) > budget) {
        set.delete(n);
        break;
      }
    }
    return set;
  }

  // the text's cursor in another heading: the tree opened around it (typing in the same heading leaves it as it is,
  // folds opened by hand too)
  $effect(() => {
    const c = current;
    untrack(() => {
      open = around(c);
      focused = c;
    });
  });
  // the heading marked stays in view
  $effect(() => {
    if (current >= 0) nav?.querySelector<HTMLElement>(`[data-index="${current}"]`)?.scrollIntoView({ block: "nearest" });
  });

  function toggle(i: number) {
    const next = new Set(open);
    if (next.has(i)) next.delete(i);
    else next.add(i);
    open = next;
  }

  function keys(e: KeyboardEvent) {
    const rows = entries.map((_, i) => i).filter((i) => shown[i]);
    const at = rows.indexOf(focused);
    if (e.key === "ArrowDown") focused = rows[Math.min(rows.length - 1, at + 1)] ?? focused;
    else if (e.key === "ArrowUp") focused = rows[Math.max(0, at - 1)] ?? focused;
    else if (e.key === "ArrowRight") {
      if (hasChildren[focused] && !open.has(focused)) toggle(focused);
      else if (hasChildren[focused]) focused = focused + 1;
    } else if (e.key === "ArrowLeft") {
      if (open.has(focused) && hasChildren[focused]) toggle(focused);
      else if (parents[focused] >= 0) focused = parents[focused];
    } else if (e.key === "Enter" && focused >= 0) onselect(focused);
    else return;
    e.preventDefault();
    queueMicrotask(() => nav?.querySelector<HTMLElement>(`[data-index="${focused}"]`)?.scrollIntoView({ block: "nearest" }));
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<nav class="outline" aria-label="Outline" bind:this={nav} bind:clientHeight={height} tabindex="0" onkeydown={keys}>
  {#each entries as entry, i (i)}
    {#if shown[i]}
      <div class="row" class:current={i === current} class:focused={i === focused} data-index={i} style:padding-left="{4 + (entry.level - top) * 12}px">
        {#if hasChildren[i]}
          <button class="caret" tabindex="-1" onclick={() => toggle(i)} aria-label={open.has(i) ? "Fold" : "Open"}>{open.has(i) ? "▾" : "▸"}</button>
        {:else}
          <span class="caret"></span>
        {/if}
        <button class="title" tabindex="-1" title={entry.number ? `${entry.number} ${entry.title}` : entry.title} onclick={() => ((focused = i), onselect(i))}
          >{#if entry.number}<span class="number">{entry.number}</span>{/if}{entry.title}</button
        >
      </div>
    {/if}
  {:else}
    <p>No headings</p>
  {/each}
</nav>

<style>
  .outline {
    height: 100%;
    box-sizing: border-box;
    overflow-y: auto;
    padding: 8px 0 40px;
    font-family: var(--editor-font, -apple-system, system-ui, sans-serif);
    font-size: 12.5px;
    outline: none;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 2px;
    padding-right: 10px;
  }
  .row:hover {
    background: var(--chip);
  }
  .row.current {
    background: var(--selection);
  }
  .outline:focus .row.focused {
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .caret {
    flex: none;
    width: 14px;
    padding: 0;
    font-size: 9px;
    color: var(--muted);
    background: none;
    border: 0;
    text-align: center;
  }
  .title {
    flex: 1;
    min-width: 0;
    font: inherit;
    text-align: left;
    color: var(--muted);
    background: none;
    border: 0;
    padding-block: 3px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row.current .title {
    color: var(--fg);
    font-weight: 650;
  }
  .row:hover .title {
    color: var(--fg);
  }
  .number {
    opacity: 0.65;
    margin-right: 0.5em;
    font-variant-numeric: tabular-nums;
  }
  p {
    margin: 0;
    padding: 4px 10px;
    color: var(--muted);
  }
</style>
