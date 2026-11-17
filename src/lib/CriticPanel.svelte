<script lang="ts">
  // The review panel of a project whose suggestions are in the text (CriticMarkup): the marks in the order of the
  // text, each with what it changes (the old text struck through, the new beside it; a comment's text and note), who
  // made it when its note says (`@MP`), and Accept and Reject (Resolve for a comment); at the foot Accept All and
  // Reject All. A click brings the mark into view in the text. Accepting and rejecting are edits of the text, made
  // by the page's editor; the panel only asks.

  import { reviewColour, type Mark } from "papers-write/editor";

  let {
    marks,
    text,
    ondecide,
    onall,
    onshow,
  }: {
    marks: Mark[];
    /** the editor's text, as the marks were read from it */
    text: () => string;
    /** a mark accepted (true) or rejected; a comment resolved (true) */
    ondecide: (mark: Mark, accept: boolean) => void;
    /** every mark accepted or rejected */
    onall: (accept: boolean) => void;
    onshow: (mark: Mark) => void;
  } = $props();

  const KIND: Record<Mark["kind"], string> = {
    insert: "Insertion",
    delete: "Deletion",
    replace: "Replacement",
    highlight: "Highlight",
    comment: "Comment",
  };
  const part = (span: { from: number; to: number } | undefined) => (span ? text().slice(span.from, span.to) : "");
  const clip = (s: string, n = 160) => (s.length > n ? s.slice(0, n - 1) + "…" : s);
  const suggestions = $derived(marks.filter((m) => m.kind !== "comment" && m.kind !== "highlight").length);

  /** A click on a button of a mark is not a click on the mark. */
  function run(e: MouseEvent, step: () => void) {
    e.stopPropagation();
    step();
  }
</script>

<aside>
  <header>
    <b>In the text</b>
    <span class="count">{marks.length} {marks.length === 1 ? "mark" : "marks"}</span>
  </header>
  <div class="list">
    {#if !marks.length}
      <p class="empty">No suggestions or comments in the text. In Suggesting mode, what is typed is written as CriticMarkup.</p>
    {/if}
    {#each marks as mark (`${mark.from}:${mark.kind}`)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
      <article onclick={() => onshow(mark)}>
        <div class="head">
          <i style:background={reviewColour(mark.author ?? "")}></i>
          <b>{KIND[mark.kind]}</b>
          {#if mark.author}<span class="who">{mark.author}</span>{/if}
        </div>
        <p class="what">
          {#if mark.old && mark.kind !== "comment" && mark.kind !== "highlight"}<del>{clip(part(mark.old))}</del>{/if}
          {#if mark.new}<ins>{clip(part(mark.new))}</ins>{/if}
          {#if mark.old && (mark.kind === "comment" || mark.kind === "highlight")}<q>{clip(part(mark.old))}</q>{/if}
        </p>
        {#if mark.note}<p class="note">{mark.note}</p>{/if}
        <div class="actions">
          {#if mark.kind === "comment" || mark.kind === "highlight"}
            <button class="primary" onclick={(e) => run(e, () => ondecide(mark, true))}>Resolve</button>
          {:else}
            <button class="primary" onclick={(e) => run(e, () => ondecide(mark, true))}>Accept</button>
            <button onclick={(e) => run(e, () => ondecide(mark, false))}>Reject</button>
          {/if}
        </div>
      </article>
    {/each}
  </div>
  {#if suggestions}
    <footer>
      <button class="primary" onclick={() => onall(true)}>Accept All</button>
      <button onclick={() => onall(false)}>Reject All</button>
    </footer>
  {/if}
</aside>

<style>
  aside {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--sidebar);
    border-left: 1px solid var(--line);
    font-size: 12.5px;
    color: var(--fg);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
  }
  .count,
  .who {
    color: var(--muted);
  }
  .list {
    flex: 1;
    overflow: auto;
    padding: 8px;
  }
  .empty {
    margin: 12px 4px;
    color: var(--muted);
  }
  article {
    margin-bottom: 8px;
    padding: 8px 10px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 8px;
    cursor: default;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .head i {
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: 50%;
  }
  .what,
  .note {
    margin: 6px 0 0;
    overflow-wrap: anywhere;
  }
  del {
    color: var(--muted);
  }
  ins {
    text-decoration: underline;
    text-underline-offset: 2px;
    margin-left: 4px;
  }
  q {
    color: var(--muted);
  }
  .note {
    font-style: italic;
  }
  .actions,
  footer {
    display: flex;
    gap: 6px;
    margin-top: 8px;
  }
  footer {
    margin: 0;
    padding: 8px 10px;
    border-top: 1px solid var(--line);
  }
  button {
    font: inherit;
    padding: 2px 10px;
    color: var(--fg);
    background: var(--chip);
    border: 1px solid var(--border);
    border-radius: 5px;
  }
  button.primary {
    color: var(--on-accent);
    background: var(--accent);
    border-color: var(--accent);
  }
</style>
