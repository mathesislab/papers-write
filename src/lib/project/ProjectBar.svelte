<script lang="ts" module>
  import type { Colour } from "./types";

  /** How near something is (papers-review: a due date), as the bar down a tile's left side: far, soon, near, due. */
  export type Level = "ok" | "soon" | "near" | "due";

  /** A project of the bar. */
  export type Tile = {
    /** what the callbacks are given (the project's folder) */
    id: string;
    name: string;
    /** the few letters the tile says, else the initials of the name */
    short: string | null;
    colour: Colour;
    /** the project of the window: marked */
    current: boolean;
    /** a line under the label (a date, a count of words); a newline makes two */
    detail?: string;
    /** a bar down the left side in the colour of the level; the tiles keep room for it when any has one */
    level?: Level | null;
    /** its folder is gone: the tile is faded */
    missing?: boolean;
    /** the tooltip; else the name and the detail */
    tooltip?: string;
  };
</script>

<script lang="ts">
  // The project bar: a column of tiles, one per project in its colour with its short name or initials (bar.ts), the
  // current one marked, an optional detail line under the label and an optional level bar down its left side; a `+`
  // tile at the end. A click asks to switch (`onswitch`), a right-click for the tile's menu (`onmenu`), `+` for
  // another project (`onadd`); with `onmove`, a tile is dragged up or down to another place (the others making way),
  // told when let go. What a tile says and does is the app's.
  //
  // The variables of the theme it uses: --sidebar (its background), --line (its right edge), --accent (the mark of
  // the current tile), --border, --muted and --chip (the + tile), --tile-none (a project of no colour), and
  // --level-ok, --level-soon, --level-near, --level-due (the level bar; else papers-review's --due-far, --due-near,
  // --due-close, --due-due). Its own: --project-bar-width (92px) and --tile-detail-size (the detail's font, 13px).

  import { label } from "./bar";
  import { cssOf } from "./colour";

  let {
    items,
    onswitch,
    onmenu,
    onadd,
    onmove,
    addTitle = "Open Project…",
  }: {
    items: Tile[];
    onswitch: (id: string) => void;
    onmenu: (id: string, event: MouseEvent) => void;
    onadd: () => void;
    /** a tile dragged to place `to` (none: tiles are not dragged) */
    onmove?: (id: string, to: number) => void;
    /** the tooltip of the + tile */
    addTitle?: string;
  } = $props();

  /** room for the level bar in every tile, so their labels line up */
  const levelled = $derived(items.some((item) => item.level));

  const tooltip = (item: Tile) => item.tooltip ?? [item.name, item.missing ? "not found" : (item.detail ?? "").replaceAll("\n", " · ")].filter(Boolean).join("\n");

  /** How far the pointer goes before a press on a tile is a drag rather than a click. */
  const SLOP = 4;
  let column = $state<HTMLElement>();
  /** the tile being dragged: where it started, how far it went, the tiles' places when it started, where it would go */
  let drag = $state<{ id: string; from: number; y: number; dy: number; moved: boolean; spans: { top: number; height: number }[]; to: number } | null>(null);
  /** a drag just ended: the click that follows it is not a switch */
  let dragged = false;

  function press(e: PointerEvent, id: string) {
    if (!onmove || e.button !== 0) return;
    const tiles = [...(column?.querySelectorAll<HTMLElement>(".tile[data-id]") ?? [])];
    const from = tiles.findIndex((t) => t.dataset.id === id);
    if (from < 0) return;
    drag = { id, from, y: e.clientY, dy: 0, moved: false, spans: tiles.map((t) => ({ top: t.offsetTop, height: t.offsetHeight })), to: from };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const dy = e.clientY - drag.y;
    const moved = drag.moved || Math.abs(dy) > SLOP;
    const own = drag.spans[drag.from];
    const middle = own.top + own.height / 2 + dy;
    let to = 0;
    drag.spans.forEach((span, i) => {
      if (i !== drag!.from && span.top + span.height / 2 < middle) to++;
    });
    drag = { ...drag, dy, moved, to };
  }

  function release() {
    if (!drag) return;
    const { id, from, to, moved } = drag;
    drag = null;
    if (!moved) return;
    dragged = true;
    setTimeout(() => (dragged = false), 0);
    if (to !== from) onmove?.(id, to);
  }

  /** How far a tile is drawn from its place while another is dragged: the one dragged follows the pointer, those
   *  between where it was and where it would go make way. */
  function offset(i: number): number {
    if (!drag?.moved) return 0;
    const { from, to, dy, spans } = drag;
    if (i === from) return dy;
    const gap = spans.length > 1 ? Math.abs(spans[1].top - spans[0].top - spans[0].height) : 0;
    const room = spans[from].height + gap;
    if (from < to && i > from && i <= to) return -room;
    if (to < from && i >= to && i < from) return room;
    return 0;
  }

  function menu(e: MouseEvent, item: Tile) {
    e.preventDefault();
    onmenu(item.id, e);
  }
</script>

<aside class="projects" aria-label="Projects" bind:this={column}>
  {#each items as item, i (item.id)}
    {@const name = label(item.name, item.short)}
    <button
      class="tile"
      class:current={item.current}
      class:missing={item.missing}
      title={tooltip(item)}
      aria-label={item.name}
      aria-current={item.current ? "true" : undefined}
      data-id={item.id}
      class:dragging={drag?.moved && drag.id === item.id}
      class:sliding={drag?.moved && drag.id !== item.id}
      style:transform={offset(i) ? `translateY(${offset(i)}px)` : undefined}
      onpointerdown={(e) => press(e, item.id)}
      onpointermove={move}
      onpointerup={release}
      onpointercancel={() => (drag = null)}
      onclick={() => !dragged && onswitch(item.id)}
      oncontextmenu={(e) => menu(e, item)}
    >
      <span
        class="chip"
        class:levelled
        class:level={item.level}
        style:background={cssOf(item.colour) ?? "var(--tile-none)"}
        style:--level={item.level ? `var(--level-${item.level}, var(--due-${{ ok: "far", soon: "near", near: "close", due: "due" }[item.level]}))` : null}
      >
        <span class="name" style:font-size="{name.size}px">{#each name.lines as line}<span>{line}</span>{/each}</span>
        {#if item.detail}<span class="detail">{#each item.detail.split("\n") as line}<span>{line}</span>{/each}</span>{/if}
      </span>
    </button>
  {/each}
  <button class="tile add" title={addTitle} aria-label={addTitle.replace(/…$/, "")} onclick={onadd}>
    <span class="chip">+</span>
  </button>
</aside>

<style>
  .tile.sliding {
    transition: transform 120ms ease;
  }
  .tile.dragging {
    position: relative;
    z-index: 1;
    opacity: 0.85;
  }
  .projects {
    box-sizing: border-box;
    width: var(--project-bar-width, 92px);
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 8px;
    min-height: 0;
    padding: 10px 0;
    overflow-y: auto;
    scrollbar-width: none;
    background: var(--sidebar);
    border-right: 1px solid var(--line);
  }
  .tile {
    position: relative;
    flex: none;
    display: flex;
    justify-content: center;
    padding: 0 8px;
    background: none;
    border: none;
    font: inherit;
  }
  /* the tile of the project in view: a mark at the bar's edge, a ring round the tile */
  .tile.current::before {
    content: "";
    position: absolute;
    left: 0;
    top: 6px;
    bottom: 6px;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: var(--accent);
  }
  .tile.current .chip {
    box-shadow:
      0 0 0 2px var(--sidebar),
      0 0 0 4px var(--accent);
  }
  .tile.missing {
    opacity: 0.45;
  }
  .chip {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    width: 76px;
    min-height: 52px;
    box-sizing: border-box;
    position: relative;
    padding: 4px;
    overflow: hidden;
    border-radius: 10px;
    color: rgba(0, 0, 0, 0.78);
  }
  .chip.levelled {
    padding-left: 13px;
  }
  /* a bar down its left side in the colour of the level, with a line between it and the tile's colour */
  .chip.level::before {
    content: "";
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 8px;
    background: var(--level);
    border-right: 1px solid rgba(0, 0, 0, 0.35);
  }
  /* the short name, on one line or two */
  .name {
    font-weight: 600;
    line-height: 1.1;
    text-align: center;
    white-space: nowrap;
  }
  .name span,
  .detail span {
    display: block;
  }
  /* the detail: dark, small as it is, on the light colours of the tiles */
  .detail {
    color: rgba(0, 0, 0, 0.68);
    font-size: var(--tile-detail-size, 13px);
    font-weight: 600;
    line-height: 1.1;
    text-align: center;
    white-space: nowrap;
  }
  .tile:not(.current):hover .chip {
    filter: brightness(0.94);
  }
  .tile.add .chip {
    border: 1px dashed var(--border);
    color: var(--muted);
    font-size: 18px;
    font-weight: 400;
  }
  .tile.add:hover .chip {
    background: var(--chip);
  }
</style>
