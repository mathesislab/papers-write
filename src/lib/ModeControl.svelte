<script lang="ts">
  // The review's mode at the top right of the text, always shown: Editing, or Suggesting in the user's colour, with the
  // number of open suggestions and comments. A click shows or hides the review panel; the ▾ is a menu of the modes,
  // the views (All Markup, Accepted, Original) and the panel. An untitled document has no review: it edits.

  import { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  import type { ReviewMode, ReviewView } from "papers-write/editor";

  let {
    mode,
    view,
    open,
    colour,
    panel,
    reviewing,
    onpanel,
    onmode,
    onview,
  }: {
    mode: ReviewMode;
    view: ReviewView;
    /** the open suggestions and comments */
    open: number;
    /** the user's colour (CSS) */
    colour: string;
    /** the review panel is shown */
    panel: boolean;
    /** the document has a review (it is a file) */
    reviewing: boolean;
    onpanel: () => void;
    onmode: (mode: ReviewMode) => void;
    onview: (view: ReviewView) => void;
  } = $props();

  const VIEWS: [ReviewView, string, string][] = [
    ["markup", "All Markup", "Alt+CmdOrCtrl+1"],
    ["accepted", "Accepted", "Alt+CmdOrCtrl+2"],
    ["original", "Original", "Alt+CmdOrCtrl+3"],
  ];

  async function menu(e: MouseEvent) {
    const anchor = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items = await Promise.all([
      CheckMenuItem.new({ text: "Editing", checked: mode === "edit", action: () => onmode("edit") }),
      CheckMenuItem.new({ text: "Suggesting", checked: mode === "suggest", enabled: reviewing, accelerator: "Shift+CmdOrCtrl+E", action: () => onmode("suggest") }),
      PredefinedMenuItem.new({ item: "Separator" }),
      ...VIEWS.map(([v, text, accelerator]) =>
        CheckMenuItem.new({ text, checked: view === v, enabled: reviewing && mode === "edit", accelerator, action: () => onview(v) }),
      ),
      PredefinedMenuItem.new({ item: "Separator" }),
      MenuItem.new({ text: panel ? "Hide Review Panel" : "Show Review Panel", enabled: reviewing, accelerator: "Alt+CmdOrCtrl+R", action: onpanel }),
    ]);
    await (await Menu.new({ items })).popup(new LogicalPosition(anchor.left, anchor.top));
  }

  const viewName = $derived(VIEWS.find(([v]) => v === view)?.[1] ?? "");
</script>

<div class="mode" class:suggesting={mode === "suggest"} style:--user={colour}>
  <button
    class="main"
    class:on={panel}
    onclick={onpanel}
    disabled={!reviewing}
    title={reviewing ? `${panel ? "Hide" : "Show"} the review panel (⌥⌘R)` : "Save the document to review it"}
  >
    <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round">
      <path d="M10.75 2.25l3 3-8 8H2.75v-3z" /><path d="M9 4l3 3" />
    </svg>
    <span>{mode === "suggest" ? "Suggesting" : "Editing"}</span>
    {#if mode === "edit" && view !== "markup"}<span class="view">· {viewName}</span>{/if}
    {#if open}<span class="count" title="{open} open">{open}</span>{/if}
  </button>
  <button class="more" onclick={menu} title="Mode and view" aria-label="Mode and view">
    <svg viewBox="0 0 10 10" width="8" height="8" aria-hidden="true"><path d="M2 3.5l3 3 3-3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
  </button>
</div>

<style>
  .mode {
    display: inline-flex;
    flex: none;
    align-items: stretch;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
    font-size: 11.5px;
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font: inherit;
    color: var(--muted);
    background: none;
    border: none;
    padding: 1px 7px;
    white-space: nowrap;
  }
  button:hover:not(:disabled) {
    color: var(--fg);
    background: var(--chip);
  }
  .main.on {
    color: var(--fg);
    background: var(--chip);
  }
  .more {
    padding: 1px 5px;
    border-left: 1px solid var(--line);
  }
  .suggesting {
    border-color: var(--user);
  }
  .suggesting button,
  .suggesting button:hover:not(:disabled) {
    color: #fff;
    background: var(--user);
  }
  .suggesting .more {
    border-left-color: color-mix(in srgb, #fff 35%, var(--user));
  }
  .suggesting button:hover:not(:disabled) {
    background: color-mix(in srgb, #000 12%, var(--user));
  }
  .view {
    opacity: 0.85;
  }
  .count {
    min-width: 14px;
    padding: 0 4px;
    border-radius: 7px;
    font-size: 10px;
    line-height: 14px;
    text-align: center;
    color: var(--fg);
    background: color-mix(in srgb, var(--fg) 12%, transparent);
  }
  .suggesting .count {
    color: var(--user);
    background: #fff;
  }
</style>
