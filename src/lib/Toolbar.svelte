<script lang="ts">
  // The toolbar above a text, under the tabs: the formats (bold, italic, code, a link; a heading, bullets, numbers, a
  // quotation: Markdown, or LaTeX in a .tex file; none in plain text), a comment on the selection, Find and Find and
  // Replace (the one the bar shows is pressed, and a click on it closes the bar); in the middle how the text is shown:
  // its size (A− A+), light or dark (a click switches to the other, a right-click offers Match System), the width of its column (a slider and Full Width, in a popover); at the right the
  // review panel, shown or hidden, with the number of open suggestions and comments, and the
  // mode, Editing or Suggesting (ModeControl). Each button says its key, where it has one (the menus give them).

  import { TEXT_WIDTH, keys, type FindBar, type Format, type ReviewMode, type ReviewView } from "papers-write/editor";
  import ModeControl from "$lib/ModeControl.svelte";
  import { CheckMenuItem, Menu } from "@tauri-apps/api/menu";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  import type { Appearance } from "$lib/settings";

  let {
    kind,
    mode,
    view,
    open,
    colour,
    panel,
    reviewing,
    findbar,
    onformat,
    oncomment,
    onfind,
    onpanel,
    width,
    onwidth,
    appearance,
    onsize,
    onappearance,
    where = "",
    store,
    onstore,
    onmode,
    onview,
  }: {
    /** what the text is: the formats are Markdown's, LaTeX's, or none */
    kind: "markdown" | "latex" | "text";
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
    /** what the find bar shows */
    findbar: FindBar;
    onformat: (what: Format) => void;
    oncomment: () => void;
    /** the bar shown, Find or Find and Replace; or closed */
    onfind: (what: "find" | "replace" | "close") => void;
    onpanel: () => void;
    /** the width of the column of text (em); none: as wide as the window */
    width: number | null;
    onwidth: (width: number | null) => void;
    /** light, dark or as the system is */
    appearance: Appearance;
    /** the text a step larger (1) or smaller (-1) */
    onsize: (by: number) => void;
    onappearance: (appearance: Appearance) => void;
    /** where suggestions are kept, said by the mode control, and its change */
    where?: string;
    store?: "notes" | "in-file";
    onstore?: (store: "notes" | "in-file") => void;
    onmode: (mode: ReviewMode) => void;
    onview: (view: ReviewView) => void;
  } = $props();

  const tex = $derived(kind === "latex");
  /** the appearance a click on its button goes to, and what the button says */
  /** the system's own appearance, as it is now: what "Match System" shows */
  const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
  let systemDark = $state(darkQuery.matches);
  $effect(() => {
    const changed = (e: MediaQueryListEvent) => (systemDark = e.matches);
    darkQuery.addEventListener("change", changed);
    return () => darkQuery.removeEventListener("change", changed);
  });
  /** what is shown: light or dark, the system's when it is matched */
  const shown = $derived<"light" | "dark">(appearance === "system" ? (systemDark ? "dark" : "light") : appearance);
  const other = $derived(shown === "dark" ? "light" : "dark");
  const NAMES = { light: "Light", dark: "Dark" };

  /** A right-click on the appearance button: Match System, Light or Dark, the one set ticked. */
  async function appearanceMenu(e: MouseEvent) {
    e.preventDefault();
    const anchor = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const choices: [Appearance, string][] = [
      ["system", "Match System"],
      ["light", "Light"],
      ["dark", "Dark"],
    ];
    const items = await Promise.all(
      choices.map(([a, text]) => CheckMenuItem.new({ text, checked: appearance === a, action: () => onappearance(a) })),
    );
    await (await Menu.new({ items })).popup(new LogicalPosition(anchor.left, anchor.bottom + 2));
  }
  /** the popover of the column's width is open; the width last set, kept while the text is full width */
  let sizing = $state(false);
  let lastWidth = $state(46);
  let sizer = $state<HTMLElement>();
  // the popover closes on a press anywhere outside it (or on its button, which toggles it)
  $effect(() => {
    if (!sizing) return;
    const away = (e: PointerEvent) => {
      if (!sizer?.contains(e.target as Node)) sizing = false;
    };
    window.addEventListener("pointerdown", away, true);
    return () => window.removeEventListener("pointerdown", away, true);
  });
  $effect(() => {
    if (width !== null) lastWidth = width;
  });
  const INLINE: [Format, string][] = $derived([
    ["bold", keys(tex ? "Bold: \\textbf (⌘B)" : "Bold (⌘B)")],
    ["italic", keys(tex ? "Italic: \\emph (⌘I)" : "Italic (⌘I)")],
    ["code", tex ? "Code: \\texttt" : "Code"],
    ["link", keys(tex ? "Link: \\href (⌘K)" : "Link (⌘K)")],
  ]);
  const BLOCKS: [Format, string][] = $derived([
    ["heading", keys(tex ? "Section: \\section, \\subsection, \\subsubsection, none (⇧⌘H)" : "Heading: #, ##, ###, none (⇧⌘H)")],
    ["bullet", keys(tex ? "Bulleted List: itemize (⇧⌘8)" : "Bulleted List (⇧⌘8)")],
    ["numbered", keys(tex ? "Numbered List: enumerate (⇧⌘7)" : "Numbered List (⇧⌘7)")],
    ["quote", keys(tex ? "Quote: the quote environment (⇧⌘9)" : "Quote (⇧⌘9)")],
  ]);
  const FIND: ["find" | "replace", string, string][] = [
    ["find", "Find", keys("⌘F")],
    ["replace", "Find and Replace", keys("⌥⌘F")],
  ];
</script>

{#snippet icon(what: Format | "comment" | "find" | "replace" | "panel" | "width" | "light" | "dark")}
  <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round">
    {#if what === "bold"}
      <path d="M4.5 2.75h4.25a2.6 2.6 0 0 1 0 5.2H4.5zM4.5 7.95h5a2.65 2.65 0 0 1 0 5.3h-5z" stroke-width="1.9" />
    {:else if what === "italic"}
      <path d="M7 2.75h5M4 13.25h5M10 2.75l-4 10.5" stroke-width="1.5" />
    {:else if what === "code"}
      <path d="M5.25 4.5 1.75 8l3.5 3.5M10.75 4.5 14.25 8l-3.5 3.5M9.25 3l-2.5 10" />
    {:else if what === "link"}
      <path d="M6.75 9.25l2.5-2.5M7.5 4.75l1-1a2.65 2.65 0 0 1 3.75 3.75l-1 1M8.5 11.25l-1 1a2.65 2.65 0 0 1-3.75-3.75l1-1" />
    {:else if what === "heading"}
      <path d="M3.5 2.75v10.5M11 2.75v10.5M3.5 8H11" stroke-width="1.6" />
    {:else if what === "bullet"}
      <circle cx="3" cy="4" r="1.1" fill="currentColor" stroke="none" />
      <circle cx="3" cy="8" r="1.1" fill="currentColor" stroke="none" />
      <circle cx="3" cy="12" r="1.1" fill="currentColor" stroke="none" />
      <path d="M6.5 4h7M6.5 8h7M6.5 12h7" />
    {:else if what === "numbered"}
      <path d="M2 2.6h1v3M1.9 5.6h2.2" stroke-width="1" />
      <path d="M1.8 9.4c.25-.6 2.1-.7 2.1.35 0 .85-2.1 1.3-2.1 2.45h2.2" stroke-width="1" />
      <path d="M6.5 4h7M6.5 8h7M6.5 12h7" />
    {:else if what === "quote"}
      <path d="M2.75 3v10" stroke-width="2" />
      <path d="M6.5 4.5h7M6.5 8h7M6.5 11.5h5" />
    {:else if what === "comment"}
      <path d="M2.5 3.25h11v7.5H8l-3 2.5v-2.5H2.5z" />
      <path d="M8 5.25v3.5M6.25 7h3.5" />
    {:else if what === "find"}
      <circle cx="7" cy="7" r="4.25" />
      <path d="M10.25 10.25 13.75 13.75" />
    {:else if what === "replace"}
      <circle cx="6" cy="6" r="3.5" />
      <path d="M8.6 8.6 10.5 10.5" />
      <path d="M9.5 13.5h4.25l-1.5-1.5M13.75 13.5l-1.5 1.5" stroke-width="1.2" />
    {:else if what === "light"}
      <circle cx="8" cy="8" r="3" />
      <path d="M8 1.75v1.5M8 12.75v1.5M1.75 8h1.5M12.75 8h1.5M3.6 3.6l1.05 1.05M11.35 11.35l1.05 1.05M3.6 12.4l1.05-1.05M11.35 4.65l1.05-1.05" />
    {:else if what === "dark"}
      <path d="M13 9.5A5.5 5.5 0 0 1 6.5 3a5.5 5.5 0 1 0 6.5 6.5z" />
    {:else if what === "width"}
      <path d="M2.25 2.75v10.5M13.75 2.75v10.5M5 8h6M6.5 6.25 4.75 8l1.75 1.75M9.5 6.25 11.25 8 9.5 9.75" />
    {:else if what === "panel"}
      <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="2" />
      <path d="M10 2.75v10.5M11.6 5.5h1M11.6 7.75h1" />
    {/if}
  </svg>
{/snippet}

<!-- a press on a button leaves the keys and the selection with the text -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="toolbar"
  role="toolbar"
  aria-label="Text"
  tabindex="-1"
  onmousedown={(e) => !(e.target as HTMLElement).closest(".popover") && e.preventDefault()}
>
  {#if kind !== "text"}
    <div class="group">
      {#each INLINE as [what, title] (what)}
        <button onclick={() => onformat(what)} {title} aria-label={title}>{@render icon(what)}</button>
      {/each}
    </div>
    <span class="rule"></span>
    <div class="group">
      {#each BLOCKS as [what, title] (what)}
        <button onclick={() => onformat(what)} {title} aria-label={title}>{@render icon(what)}</button>
      {/each}
    </div>
    <span class="rule"></span>
  {/if}
  <div class="group">
    <button
      onclick={oncomment}
      disabled={!reviewing}
      title={reviewing ? keys("Add Comment on the selection (⌥⌘M)") : "Save the document to comment on it"}
      aria-label="Add Comment">{@render icon("comment")}</button
    >
  </div>
  <span class="rule"></span>
  <div class="group">
    {#each FIND as [what, name, key] (what)}
      <button
        class:on={findbar === what}
        onclick={() => onfind(findbar === what ? "close" : what)}
        title={findbar === what ? `Close ${name} (Esc)` : `${name} (${key})`}
        aria-label={name}
        aria-pressed={findbar === what}>{@render icon(what)}</button
      >
    {/each}
  </div>
  <!-- in the middle, how the text is shown -->
  <div class="middle">
    <button class="size" onclick={() => onsize(-1)} title={keys("Smaller Text (the whole window: ⌘−)")} aria-label="Smaller Text">A<sup>−</sup></button>
    <button class="size" onclick={() => onsize(1)} title={keys("Larger Text (the whole window: ⌘=)")} aria-label="Larger Text">A<sup>+</sup></button>
    <button
      onclick={() => onappearance(other)}
      oncontextmenu={appearanceMenu}
      title="{NAMES[shown]}{appearance === 'system' ? ' (matching the system)' : ''}. Click: {NAMES[other]}. Right-click: Match System, Light or Dark"
      aria-label="Appearance: {NAMES[shown]}; switch to {NAMES[other]}">{@render icon(shown)}</button
    >
    <div class="sizing" bind:this={sizer}>
      <button
        class:on={sizing}
        onclick={() => (sizing = !sizing)}
        title="Text Width"
        aria-label="Text Width"
        aria-expanded={sizing}>{@render icon("width")}</button
      >
      {#if sizing}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="popover" onkeydown={(e) => e.key === "Escape" && (sizing = false)}>
          <label>
            <span>Width</span>
            <input
              type="range"
              min={TEXT_WIDTH.least}
              max={TEXT_WIDTH.most}
              step="1"
              value={width ?? lastWidth}
              disabled={width === null}
              oninput={(e) => onwidth(Number(e.currentTarget.value))}
            />
            <span class="value">{width === null ? "—" : `${width} em`}</span>
          </label>
          <label>
            <input type="checkbox" checked={width === null} onchange={(e) => onwidth(e.currentTarget.checked ? null : lastWidth)} />
            <span>Full width</span>
          </label>
        </div>
      {/if}
    </div>
  </div>
  <!-- at the right, together on a line of their own in a narrow window -->
  <div class="end">
    <button
      class="panel"
      class:on={panel}
      onclick={onpanel}
      disabled={!reviewing}
      title={reviewing ? keys(`${panel ? "Hide" : "Show"} Review Panel (⌥⌘R)`) : "Save the document to review it"}
      aria-label="{panel ? 'Hide' : 'Show'} Review Panel"
      aria-pressed={panel}
    >
      {@render icon("panel")}
      {#if open}<span class="count" title="{open} open">{open}</span>{/if}
    </button>
    <ModeControl {mode} {view} {colour} {reviewing} {where} {store} {onstore} {onmode} {onview} />
  </div>
</div>

<style>
  .toolbar {
    flex: none;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px 4px;
    min-height: 32px;
    box-sizing: border-box;
    padding: 3px 10px;
    background: var(--bg);
    border-bottom: 1px solid var(--line);
    font-size: 12px;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 1px;
  }
  .rule {
    flex: none;
    width: 1px;
    height: 16px;
    margin: 0 4px;
    background: var(--line);
  }
  .middle {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 1px;
  }
  .size {
    font-weight: 600;
    font-size: 12px;
  }
  .size sup {
    font-size: 9px;
    margin-left: 1px;
  }
  .sizing {
    position: relative;
  }
  .popover {
    position: absolute;
    top: calc(100% + 4px);
    left: 50%;
    transform: translateX(-50%);
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
    white-space: nowrap;
  }
  .popover label {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--fg);
  }
  .popover input[type="range"] {
    width: 140px;
  }
  .popover .value {
    min-width: 44px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .end {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 4px;
  }
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    min-width: 26px;
    height: 24px;
    padding: 0 5px;
    box-sizing: border-box;
    font: inherit;
    color: var(--muted);
    background: none;
    border: 1px solid transparent;
    border-radius: 5px;
  }
  button:hover:not(:disabled) {
    color: var(--fg);
    background: var(--chip);
  }
  button:disabled {
    opacity: 0.4;
  }
  button.on {
    color: var(--fg);
    background: var(--chip);
    border-color: var(--line);
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
</style>
