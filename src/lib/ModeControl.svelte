<script lang="ts">
  // The review's mode, at the right of the toolbar: Editing or Suggesting, a click on either switches to it (⇧⌘E
  // switches too), Suggesting in the user's colour; the ▾ is a menu of the views of the suggestions in Editing (All
  // Markup, Accepted, Original), the one shown said after Editing when it is not All Markup. An untitled document has
  // no review: it edits. Suggesting says where suggestions go (`where`: in the text as CriticMarkup, or in the review's
  // notes). The review panel is a button of its own (Toolbar).

  import { CheckMenuItem, Menu, PredefinedMenuItem } from "@tauri-apps/api/menu";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  import type { ReviewMode, ReviewView } from "papers-write/editor";

  let {
    mode,
    view,
    colour,
    reviewing,
    where = "",
    store,
    onstore,
    onmode,
    onview,
  }: {
    mode: ReviewMode;
    view: ReviewView;
    /** the user's colour (CSS) */
    colour: string;
    /** the document has a review (it is a file) */
    reviewing: boolean;
    /** where suggestions go ("in the text", "in notes"), said after Suggesting while suggesting; none: not said */
    where?: string;
    /** where the project keeps its suggestions, and its change (none: not offered) */
    store?: "notes" | "in-file";
    onstore?: (store: "notes" | "in-file") => void;
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
      ...VIEWS.map(([v, text, accelerator]) =>
        CheckMenuItem.new({ text, checked: view === v, enabled: reviewing && mode === "edit", accelerator, action: () => onview(v) }),
      ),
      ...(store && onstore
        ? [
            PredefinedMenuItem.new({ item: "Separator" }),
            CheckMenuItem.new({ text: "Suggestions in Review Notes", checked: store === "notes", enabled: reviewing, action: () => onstore("notes") }),
            CheckMenuItem.new({ text: "Suggestions in the Text (CriticMarkup)", checked: store === "in-file", enabled: reviewing, action: () => onstore("in-file") }),
          ]
        : []),
    ]);
    await (await Menu.new({ items })).popup(new LogicalPosition(anchor.left, anchor.bottom + 2));
  }

  const viewName = $derived(VIEWS.find(([v]) => v === view)?.[1] ?? "");
</script>

<div class="mode" class:suggesting={mode === "suggest"} style:--user={colour} role="radiogroup" aria-label="Mode">
  <button
    class:on={mode === "edit"}
    onclick={() => onmode("edit")}
    role="radio"
    aria-checked={mode === "edit"}
    title={mode === "edit" ? "Editing: the text changes as typed" : "Edit the text (⇧⌘E)"}
  >
    Editing{#if mode === "edit" && view !== "markup"}<span class="view">· {viewName}</span>{/if}
  </button>
  <button
    class="suggest"
    class:on={mode === "suggest"}
    onclick={() => onmode("suggest")}
    disabled={!reviewing}
    role="radio"
    aria-checked={mode === "suggest"}
    title={!reviewing
      ? "Save the document to suggest changes"
      : mode === "suggest"
        ? `Suggesting: what is typed is a suggestion${where ? `, kept ${where}` : ""}`
        : `Suggest changes${where ? `, kept ${where}` : ""} (⇧⌘E)`}
  >
    Suggesting{#if mode === "suggest" && where}<span class="view">· {where}</span>{/if}
  </button>
  <button class="more" onclick={menu} disabled={!reviewing} title="View: All Markup, Accepted, Original; where suggestions are kept" aria-label="View">
    <svg viewBox="0 0 10 10" width="8" height="8" aria-hidden="true"><path d="M2 3.5l3 3 3-3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
  </button>
</div>

<style>
  .mode {
    display: inline-flex;
    flex: none;
    align-items: stretch;
    height: 24px;
    box-sizing: border-box;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
    font-size: 11.5px;
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font: inherit;
    color: var(--muted);
    background: none;
    border: none;
    padding: 0 8px;
    white-space: nowrap;
  }
  button + button {
    border-left: 1px solid var(--line);
  }
  button:hover:not(:disabled):not(.on) {
    color: var(--fg);
    background: var(--chip);
  }
  button:disabled {
    opacity: 0.45;
  }
  button.on {
    color: var(--fg);
    background: var(--chip);
    font-weight: 600;
  }
  .suggesting {
    border-color: var(--user);
  }
  .suggest.on {
    color: #fff;
    background: var(--user);
  }
  .more {
    padding: 0 5px;
  }
  .view {
    font-weight: 400;
    opacity: 0.85;
  }
</style>
