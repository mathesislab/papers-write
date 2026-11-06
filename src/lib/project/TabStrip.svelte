<script lang="ts" module>
  /** A tab: its id (the app's), its name, the tooltip (a path). */
  export type TabItem = { id: string; title: string; tooltip?: string | null };
</script>

<script lang="ts">
  import type { Snippet } from "svelte";

  // The tabs of a window as a strip, shared by papers-write and papers-review: each as wide as its name needs
  // (within bounds), the one in front marked, a close button at its right (shown on the tab in front and on hover).
  // A click chooses a tab (`onselect`), the close button or a middle click closes it (`onclose`). It only draws: what
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
    extra,
  }: {
    tabs: TabItem[];
    /** the id of the tab in front */
    front: string | null;
    /** the fewest tabs that show a strip */
    least?: number;
    onselect: (id: string) => void;
    onclose: (id: string) => void;
    /** a control of the app's on a tab, before its close button */
    extra?: Snippet<[TabItem]>;
  } = $props();
</script>

{#if tabs.length >= least}
  <div class="strip" role="tablist">
    {#each tabs as tab (tab.id)}
      <div
        class="tab"
        class:on={tab.id === front}
        role="tab"
        tabindex="-1"
        aria-selected={tab.id === front}
        title={tab.tooltip ?? tab.title}
        onclick={() => tab.id !== front && onselect(tab.id)}
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
