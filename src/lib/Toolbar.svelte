<script lang="ts">
  // The toolbar above a text, under the tabs: the formats (bold, italic, code, a link; a heading, bullets, numbers, a
  // quotation: Markdown, or LaTeX in a .tex file; none in plain text), a comment on the selection, Find and Find and
  // Replace (the one the bar shows is pressed, and a click on it closes the bar); at the right the review panel, shown or hidden, with the number of open suggestions and comments, and the
  // mode, Editing or Suggesting (ModeControl). Each button says its key, where it has one (the menus give them).

  import type { FindBar, Format, ReviewMode, ReviewView } from "papers-write/editor";
  import ModeControl from "$lib/ModeControl.svelte";
  import ReadAloudButton from "$lib/ReadAloudButton.svelte";

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
    onmode,
    onview,
    speak,
  }: {
    /** what Read Aloud reads (its button, when a command is set) */
    speak?: () => string;
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
    onmode: (mode: ReviewMode) => void;
    onview: (view: ReviewView) => void;
  } = $props();

  const tex = $derived(kind === "latex");
  const INLINE: [Format, string][] = $derived([
    ["bold", tex ? "Bold: \\textbf (⌘B)" : "Bold (⌘B)"],
    ["italic", tex ? "Italic: \\emph (⌘I)" : "Italic (⌘I)"],
    ["code", tex ? "Code: \\texttt" : "Code"],
    ["link", tex ? "Link: \\href (⌘K)" : "Link (⌘K)"],
  ]);
  const BLOCKS: [Format, string][] = $derived([
    ["heading", tex ? "Section: \\section, \\subsection, \\subsubsection, none (⇧⌘H)" : "Heading: #, ##, ###, none (⇧⌘H)"],
    ["bullet", tex ? "Bulleted List: itemize (⇧⌘8)" : "Bulleted List (⇧⌘8)"],
    ["numbered", tex ? "Numbered List: enumerate (⇧⌘7)" : "Numbered List (⇧⌘7)"],
    ["quote", tex ? "Quote: the quote environment (⇧⌘9)" : "Quote (⇧⌘9)"],
  ]);
  const FIND: ["find" | "replace", string, string][] = [
    ["find", "Find", "⌘F"],
    ["replace", "Find and Replace", "⌥⌘F"],
  ];
</script>

{#snippet icon(what: Format | "comment" | "find" | "replace" | "panel")}
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
    {:else if what === "panel"}
      <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="2" />
      <path d="M10 2.75v10.5M11.6 5.5h1M11.6 7.75h1" />
    {/if}
  </svg>
{/snippet}

<!-- a press on a button leaves the keys and the selection with the text -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="toolbar" role="toolbar" aria-label="Text" tabindex="-1" onmousedown={(e) => e.preventDefault()}>
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
      title={reviewing ? "Add Comment on the selection (⌥⌘M)" : "Save the document to comment on it"}
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
  {#if speak}
    <span class="rule"></span>
    <div class="group"><ReadAloudButton text={speak} /></div>
  {/if}
  <!-- at the right, together on a line of their own in a narrow window -->
  <div class="end">
    <button
      class="panel"
      class:on={panel}
      onclick={onpanel}
      disabled={!reviewing}
      title={reviewing ? `${panel ? "Hide" : "Show"} Review Panel (⌥⌘R)` : "Save the document to review it"}
      aria-label="{panel ? 'Hide' : 'Show'} Review Panel"
      aria-pressed={panel}
    >
      {@render icon("panel")}
      {#if open}<span class="count" title="{open} open">{open}</span>{/if}
    </button>
    <ModeControl {mode} {view} {colour} {reviewing} {onmode} {onview} />
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
