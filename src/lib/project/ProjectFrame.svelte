<script lang="ts">
  // A tab's page as all the tabs of a project show it, shared by Papers Write and Papers Review: the project bar at
  // the left, the project's sidebar (shown or not, its width dragged at its right edge: ProjectSidebar, the same in all
  // the project's tabs), and the tab's own content under the strip of tabs (`lead` at its left, `top` at its right).
  // Whether a tab shows a text or a PDF, nothing around its content moves as the tabs change. A thin bar in a colour at
  // the top when asked (`colour`; a right-click on it is the app's, `oncolourmenu`).

  import type { Snippet } from "svelte";
  import type { ProjectSidebar } from "./sidebar.svelte";
  import type { TabItem } from "./TabStrip.svelte";
  import Tabs from "./Tabs.svelte";

  let {
    side,
    colour = null,
    colourTitle,
    oncolourmenu,
    offset = 0,
    least = 2,
    extra,
    bar,
    sidebar,
    lead,
    top,
    children,
  }: {
    side: ProjectSidebar;
    /** the project's colour for the bar at the top (none: no bar) */
    colour?: string | null;
    colourTitle?: string;
    oncolourmenu?: (e: MouseEvent) => void;
    /** the height (points) of what the page draws above the tabs, fixed at the top of the window */
    offset?: number;
    /** the fewest tabs that show a strip (Tabs) */
    least?: number;
    /** a control of the app's on a tab (TabStrip) */
    extra?: Snippet<[TabItem]>;
    /** the project bar */
    bar?: Snippet;
    /** what the sidebar shows, while it is shown */
    sidebar?: Snippet;
    /** at the left of the strip of tabs */
    lead?: Snippet;
    /** at the right of the strip of tabs */
    top?: Snippet;
    children: Snippet;
  } = $props();

  /** The narrowest and widest the sidebar is dragged. */
  const WIDTH = { min: 160, max: 520 };

  /** The sidebar's right edge dragged: its width, kept when let go. Over a PDF (native, above the page) the drag
   *  stays with the edge: AppKit sends the moves to the view the press went to. */
  function resize(e: PointerEvent) {
    const edge = e.currentTarget as HTMLElement;
    edge.setPointerCapture(e.pointerId);
    const [start, width] = [e.clientX, side.width];
    const move = (m: PointerEvent) =>
      side.set({ width: Math.round(Math.min(WIDTH.max, Math.max(WIDTH.min, width + m.clientX - start))) }, false);
    const stop = () => {
      edge.removeEventListener("pointermove", move);
      edge.removeEventListener("pointerup", stop);
      side.keep();
    };
    edge.addEventListener("pointermove", move);
    edge.addEventListener("pointerup", stop);
  }
</script>

<!-- the project's colour, at the top, to tell its windows apart among others -->
{#if colour}<div class="colour" style:background={colour} title={colourTitle} oncontextmenu={oncolourmenu} role="presentation"></div>{/if}
<div class="frame" class:coloured={colour}>
  {@render bar?.()}
  {#if side.shown && sidebar}
    <div class="side" style:width="{side.width}px">
      {@render sidebar()}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="edge" onpointerdown={resize}></div>
    </div>
  {/if}
  <main>
    <div class="top" style:margin-top="{offset}px">
      {@render lead?.()}
      <div class="tabs"><Tabs {least} {extra} /></div>
      {@render top?.()}
    </div>
    <!-- not positioned: what the page places absolutely is placed in `main` -->
    <div class="content">{@render children()}</div>
  </main>
</div>

<style>
  /* absolute, not fixed: WebKit tints the title bar with what is fixed at the top of the page */
  .colour {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: 50;
    height: 6px;
  }
  .frame {
    display: flex;
    height: 100vh;
    box-sizing: border-box;
  }
  .frame.coloured {
    padding-top: 6px;
  }
  .side {
    position: relative;
    flex: none;
    height: 100%;
  }
  .edge {
    position: absolute;
    top: 0;
    right: -3px;
    width: 6px;
    height: 100%;
    cursor: col-resize;
    z-index: 3;
  }
  main {
    flex: 1;
    min-width: 0;
    height: 100%;
    position: relative;
    display: flex;
    flex-direction: column;
  }
  .top {
    flex: none;
    display: flex;
    gap: 10px;
    align-items: center;
    height: 32px;
    box-sizing: border-box;
    padding: 0 10px 0 8px;
  }
  .tabs {
    flex: 1;
    min-width: 0;
  }
  .content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
</style>
