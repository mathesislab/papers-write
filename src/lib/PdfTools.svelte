<script lang="ts">
  // The tools of a PDF, the same in a PDF's tab and in the pane beside a text: the page in view (a click asks for
  // another), the zoom (−, Fit, +), the section in view (the outline's, when the PDF has one; a click lists them all)
  // and Find (Enter the next match, ⇧Enter the one before, ✕ or Esc clears it). Back and Forward are the Go menu's.
  // What is at either end is the page's (`lead`, `trail`): the tab's name, the pane's PDF and what it did.

  import { tick, type Snippet } from "svelte";
  import { keys } from "papers-write/editor";
  import { CheckMenuItem, Menu } from "@tauri-apps/api/menu";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  import { sectionAt, type OutlineItem, type PdfViewer } from "$lib/pdf";

  let {
    view,
    current,
    count,
    outline = [],
    lead,
    trail,
    room = 0,
  }: {
    view: PdfViewer | undefined;
    current: number;
    count: number;
    /** the outline (bookmarks), each with the page it goes to (none: not known) */
    outline?: OutlineItem[];
    lead?: Snippet;
    trail?: Snippet;
    /** room kept at the right (px): the pane's ×, which the page lays over it */
    room?: number;
  } = $props();

  let text = $state("");
  let lastText = "";
  let notFound = $state(false);
  let field = $state<HTMLInputElement>();
  /** the page typed in the field, while it is */
  let typing = $state<string | null>(null);

  /** Find… (⌘F): the field, with the PDF's selection when there is one. */
  export async function focusFind() {
    const selected = window.getSelection()?.toString().trim();
    if (selected && !selected.includes("\n")) text = selected;
    await tick();
    field?.focus();
    field?.select();
  }

  /** Find Next and Find Previous; the field asked first when it is empty. */
  export async function findAgain(backwards: boolean) {
    if (!text.trim()) return focusFind();
    const again = text === lastText;
    lastText = text;
    notFound = !(await view?.find(text, again, backwards));
  }

  /** Use Selection for Find. */
  export function findSelection() {
    const selected = window.getSelection()?.toString().trim();
    if (selected) [text, lastText] = [selected, ""];
    void findAgain(false);
  }

  /** Go to Page… (⌥⌘G): the page field. */
  export function askPage() {
    typing = String(current);
  }

  function clear() {
    [text, lastText, notFound] = ["", "", false];
    void view?.find("");
    view?.focus();
  }

  function answerPage(go: boolean) {
    const page = Number.parseInt(typing ?? "", 10);
    typing = null;
    if (go && Number.isFinite(page)) view?.pageTo(Math.min(Math.max(page, 1), count));
    view?.focus();
  }

  /** The section in view: the last of the outline that begins on the page in view or before it. */
  const here = $derived(sectionAt(outline, current));

  async function outlineMenu(e: MouseEvent) {
    const anchor = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items = await Promise.all(
      outline
        .slice(0, 200)
        .map((o, i) => CheckMenuItem.new({ text: `${"    ".repeat(Math.max(0, o.level - 1))}${o.title}`, checked: i === here, action: () => view?.goToOutline?.(i) })),
    );
    await (await Menu.new({ items })).popup(new LogicalPosition(anchor.left, anchor.bottom + 2));
  }
</script>

<div class="tools" style:padding-right={room ? `${room + 8}px` : null}>
  {@render lead?.()}
  {#if typing !== null}
    <!-- svelte-ignore a11y_autofocus -->
    <input
      class="page-field"
      bind:value={typing}
      autofocus
      inputmode="numeric"
      autocomplete="off"
      aria-label="Go to Page"
      onkeydown={(e) => {
        if (e.key === "Enter") answerPage(true);
        if (e.key === "Escape") answerPage(false);
      }}
      onblur={() => (typing = null)}
    />
    <span class="page">of {count}</span>
  {:else if count}
    <button class="page" onclick={askPage} title={keys("Go to Page… (⌥⌘G)")}>Page {current} of {count}</button>
  {/if}
  <span class="group">
    <button onclick={() => view?.zoom(-1)} title={keys("Zoom Out (⌘-)")} aria-label="Zoom Out">−</button>
    <button onclick={() => view?.zoom(0)} title={keys("Fit Width (⌘0)")}>Fit</button>
    <button onclick={() => view?.zoom(1)} title={keys("Zoom In (⌘=)")} aria-label="Zoom In">+</button>
  </span>
  {#if outline.length}
    <button class="section" onclick={outlineMenu} title={here >= 0 ? `${outline[here].title} (a click lists the sections)` : "The sections"}>
      <span>{here >= 0 ? outline[here].title : "Outline"}</span> ▾
    </button>
  {/if}
  <span class="find" class:missing={notFound}>
    <input
      bind:this={field}
      bind:value={text}
      placeholder="Find"
      autocomplete="off"
      spellcheck="false"
      aria-label="Find in the PDF"
      oninput={() => {
        notFound = false;
        if (!text) void view?.find("");
      }}
      onkeydown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          void findAgain(e.shiftKey);
        }
        if (e.key === "Escape") clear();
      }}
    />
    {#if text}<button class="clear" onclick={clear} title="Clear (esc)" aria-label="Clear">✕</button>{/if}
  </span>
  {@render trail?.()}
</div>

<style>
  /* on a narrow pane, onto a second line rather than past its right edge */
  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    min-width: 0;
    padding: 4px 8px;
    background: var(--bg);
    border-bottom: 1px solid var(--line);
    font-size: 12px;
  }
  button {
    flex: none;
    font: inherit;
    font-size: 12px;
    padding: 1px 8px;
    color: var(--fg);
    background: var(--chip);
    border: 1px solid var(--border);
    border-radius: 5px;
  }
  button:disabled {
    opacity: 0.4;
  }
  /* the section in view: a fixed width, a long title fading out at its end */
  .section {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    width: 16em;
    max-width: 30%;
    flex: 0 1 auto;
    text-align: left;
  }
  .section span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    -webkit-mask-image: linear-gradient(to right, #000 82%, transparent);
    mask-image: linear-gradient(to right, #000 82%, transparent);
  }
  .page {
    background: none;
    border-color: transparent;
    color: var(--muted);
    white-space: nowrap;
  }
  .page-field {
    width: 3.5em;
    font: inherit;
    font-size: 12px;
    padding: 1px 4px;
    color: var(--fg);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 5px;
  }
  .group {
    display: inline-flex;
    gap: 2px;
  }
  .find {
    position: relative;
    display: inline-flex;
    align-items: center;
    flex: 0 1 12em;
    min-width: 6em;
  }
  .find input {
    width: 100%;
    font: inherit;
    font-size: 12px;
    padding: 1px 20px 1px 6px;
    color: var(--fg);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 5px;
  }
  .find.missing input {
    border-color: var(--error);
  }
  .find .clear {
    position: absolute;
    right: 3px;
    width: 15px;
    height: 15px;
    padding: 0;
    font-size: 9px;
    line-height: 1;
    color: var(--muted);
    background: var(--chip);
    border: none;
    border-radius: 50%;
  }
  .find .clear:hover {
    color: var(--fg);
  }
</style>
