<script lang="ts" module>
  /** A tab: its id (the app's), its name, the tooltip (a path). */
  export type TabItem = { id: string; title: string; tooltip?: string | null };
</script>

<script lang="ts">
  import type { Snippet } from "svelte";

  // The tabs of a window as a strip, shared by papers-write and papers-review: each as wide as its name needs
  // (within bounds), the one in front marked, a close button at its right (shown on the tab in front and on hover).
  // A click chooses a tab (`onselect`), the close button or a middle click closes it (`onclose`). With `onmove`, a tab
  // is dragged along the strip to another place (the others making way), told when let go. It only draws: what
  // the tabs are and what choosing does are the app's. Nothing shows for fewer than `least` tabs. An app may put a
  // control of its own on a tab, before the close button (`extra`, shown like it).
  //
  // Its colours are the app's: --fg, --muted, --bg (the tab in front), --chip (hover), --border.

  let {
    tabs,
    front,
    least = 2,
    onselect,
    onclose,
    onmove,
    extra,
  }: {
    tabs: TabItem[];
    /** the id of the tab in front */
    front: string | null;
    /** the fewest tabs that show a strip */
    least?: number;
    onselect: (id: string) => void;
    onclose: (id: string) => void;
    /** a tab dragged to place `to` among the tabs (none: tabs are not dragged) */
    onmove?: (id: string, to: number) => void;
    /** a control of the app's on a tab, before its close button */
    extra?: Snippet<[TabItem]>;
  } = $props();

  /** How far the pointer goes before a press on a tab is a drag rather than a click. */
  const SLOP = 4;

  let strip = $state<HTMLElement>();
  /** the tab being dragged: where it started, how far it went, the tabs' places when it started, where it would go */
  let drag = $state<{ id: string; from: number; x: number; dx: number; moved: boolean; spans: { left: number; width: number }[]; to: number } | null>(null);
  /** a drag just ended: the click that follows it is not a choice */
  let dragged = false;

  function press(e: PointerEvent, id: string) {
    if (!onmove || e.button !== 0 || (e.target as HTMLElement).closest("button, .extra")) return;
    const tabs = [...(strip?.querySelectorAll<HTMLElement>(".tab") ?? [])];
    const spans = tabs.map((t) => ({ left: t.offsetLeft, width: t.offsetWidth }));
    const from = tabs.findIndex((t) => t.dataset.id === id);
    if (from < 0) return;
    drag = { id, from, x: e.clientX, dx: 0, moved: false, spans, to: from };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const dx = e.clientX - drag.x;
    const moved = drag.moved || Math.abs(dx) > SLOP;
    // the place whose middle the dragged tab's middle has passed
    const own = drag.spans[drag.from];
    const middle = own.left + own.width / 2 + dx;
    let to = 0;
    drag.spans.forEach((span, i) => {
      if (i !== drag!.from && span.left + span.width / 2 < middle) to++;
    });
    drag = { ...drag, dx, moved, to };
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

  /** How far a tab is drawn from its place while another is dragged: the one dragged follows the pointer, those
   *  between where it was and where it would go make way. */
  function offset(i: number): number {
    if (!drag?.moved) return 0;
    const { from, to, dx, spans } = drag;
    if (i === from) return dx;
    const gap = spans.length > 1 ? Math.abs(spans[1].left - spans[0].left - spans[0].width) : 0;
    const room = spans[from].width + gap;
    if (from < to && i > from && i <= to) return -room;
    if (to < from && i >= to && i < from) return room;
    return 0;
  }
</script>

{#if tabs.length >= least}
  <div class="strip" role="tablist" bind:this={strip}>
    {#each tabs as tab, i (tab.id)}
      <div
        class="tab"
        class:on={tab.id === front}
        class:dragging={drag?.moved && drag.id === tab.id}
        class:sliding={drag?.moved && drag.id !== tab.id}
        style:transform={offset(i) ? `translateX(${offset(i)}px)` : undefined}
        data-id={tab.id}
        role="tab"
        tabindex="-1"
        aria-selected={tab.id === front}
        title={tab.tooltip ?? tab.title}
        onclick={() => !dragged && tab.id !== front && onselect(tab.id)}
        onpointerdown={(e) => press(e, tab.id)}
        onpointermove={move}
        onpointerup={release}
        onpointercancel={() => (drag = null)}
        onauxclick={(e) => e.button === 1 && onclose(tab.id)}
        onkeydown={(e) => e.key === "Enter" && onselect(tab.id)}
      >
        <span class="name">{tab.title}</span>
        {#if extra}
          <!-- a click on the app's control is not a click on the tab -->
          <span class="extra" role="none" onclick={(e) => e.stopPropagation()}>{@render extra(tab)}</span>
        {/if}
        <button
          class="close"
          title="Close Tab"
          aria-label="Close {tab.title}"
          onclick={(e) => {
            e.stopPropagation();
            onclose(tab.id);
          }}
        >
          <svg viewBox="0 0 12 12" width="9" height="9" aria-hidden="true"><path d="M3 3l6 6M9 3l-6 6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .strip {
    display: flex;
    align-items: center;
    gap: 2px;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .strip::-webkit-scrollbar {
    display: none;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 0 1 auto;
    min-width: 72px;
    max-width: 200px;
    height: 24px;
    box-sizing: border-box;
    padding: 0 4px 0 10px;
    font-size: 12px;
    color: var(--muted);
    border: 1px solid transparent;
    border-radius: 6px;
    cursor: default;
    -webkit-user-select: none;
    user-select: none;
  }
  .tab.sliding {
    transition: transform 120ms ease;
  }
  .tab.dragging {
    position: relative;
    z-index: 1;
    color: var(--fg);
    background: var(--chip);
  }
  .tab:hover {
    color: var(--fg);
    background: var(--chip);
  }
  .tab.on {
    color: var(--fg);
    background: var(--bg);
    border-color: var(--border);
    font-weight: 500;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .close,
  .extra > :global(*) {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 16px;
    height: 16px;
    padding: 0;
    color: var(--muted);
    background: none;
    border: none;
    border-radius: 4px;
    visibility: hidden;
  }
  .extra {
    display: contents;
  }
  .tab.on .close,
  .tab:hover .close,
  .tab.on .extra > :global(*),
  .tab:hover .extra > :global(*) {
    visibility: visible;
  }
  .close:hover,
  .extra > :global(*:hover) {
    color: var(--fg);
    background: color-mix(in srgb, var(--fg) 12%, transparent);
  }
</style>
