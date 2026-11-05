<script lang="ts">
  // The references of the project, listed over the review pane (⇧⌘R): the papers of a .bib or a papers library as
  // Papers Desktop finds them. Typing searches, the words matching from their start, a moment after the typing stops;
  // opened with a reference selected in the PDF, it lists the papers closest to that text until something is typed.
  // The key on the one marked: ↵ puts a citation in at the cursor, ⇧↵ the full reference, ⌘. [@key], ⌘L \cite{key};
  // ⌘↵ opens its PDF in a tab of the project. Esc closes the list. Where the references come from is the app's:
  // `search`, `closest` and `sources`.

  import { onMount } from "svelte";
  import { citation, fullReference, latex, pandoc } from "./refs";
  import type { Found, Item, Span } from "./types";

  let {
    text = "",
    enter = "citation",
    search: find,
    closest: findClosest,
    sources: findSources,
    oninsert,
    onopen,
    onclose,
  }: {
    /** a reference selected in the PDF: the papers closest to it are listed first */
    text?: string;
    /** what ↵ puts in: a citation as read, "Schoof (2007)" (a review), or `[@key]` (a paper in Markdown); ⇧↵ the
     *  full reference either way */
    enter?: "citation" | "pandoc";
    /** the papers matching `query`, at most `limit` of them */
    search: (query: string, limit: number) => Promise<Found>;
    /** the papers closest to `text`, at most `limit` of them */
    closest: (text: string, limit: number) => Promise<Found>;
    /** the .bib files of the references */
    sources: () => Promise<string[]>;
    /** the text to put in at the cursor */
    oninsert: (text: string) => void;
    /** the PDF of a paper, to open in a tab of the project */
    onopen: (path: string) => void;
    onclose: () => void;
  } = $props();

  /** the most papers listed */
  const LIMIT = 50;
  let query = $state("");
  /** the papers closest to `text` are listed, not what `query` finds */
  let nearest = $state(false);
  let found = $state<Found | null>(null);
  let error = $state("");
  /** the paper marked, by its place in the list */
  let marked = $state(0);
  /** the .bib files of the references */
  let bibs = $state<string[]>([]);
  let input: HTMLInputElement;
  let list: HTMLElement;
  let timer: ReturnType<typeof setTimeout> | undefined;
  /** the search asked last: an answer to an earlier one is dropped */
  let asked = 0;
  const hits = $derived(found?.hits ?? []);
  const hit = $derived(hits[marked]);

  async function search() {
    const now = ++asked;
    try {
      const result = nearest ? await findClosest(text, LIMIT) : await find(query, LIMIT);
      if (now !== asked) return;
      [found, error, marked] = [result, "", 0];
      list?.scrollTo({ top: 0 });
    } catch (e) {
      if (now === asked) [found, error] = [null, String(e)];
    }
  }

  function typed() {
    nearest = false;
    clearTimeout(timer);
    timer = setTimeout(search, 120);
  }

  /** The PDF of the paper marked, if it has one. */
  function pdf(item: Item | undefined) {
    if (item?.files[0]) onopen(item.files[0]);
  }

  function key(e: KeyboardEvent) {
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (step) {
      marked = Math.max(0, Math.min(hits.length - 1, marked + step));
      list.children[marked]?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter" && e.metaKey) {
      pdf(hit?.item);
    } else if (e.key === "Enter") {
      cite(e.shiftKey ? "reference" : enter);
    } else if (e.metaKey && e.key === ".") {
      cite("pandoc");
    } else if (e.metaKey && e.key.toLowerCase() === "l") {
      cite("latex");
    } else if (e.key === "Escape") {
      onclose();
    } else {
      return;
    }
    e.preventDefault();
    e.stopPropagation();
  }

  /** Focus leaving for another place of the page closes the list, but not for a click in it, nor for the window
   *  losing the keys. */
  function left(e: FocusEvent) {
    const to = e.relatedTarget as Node | null;
    if (to && !(e.currentTarget as HTMLElement).contains(to)) onclose();
  }

  /** The keys to the search field. */
  export const focus = () => input?.focus();

  /** The paper marked put in at the cursor: as a citation ("Schoof (2007)"), the full reference, `[@key]` or
   *  `\cite{key}`. The keys call it, and so can the app's menu: macOS takes ⌘. for Cancel before the page sees it, so
   *  ⌘. only reaches the list as a menu item of the app. */
  export function cite(form: "citation" | "reference" | "pandoc" | "latex") {
    if (!hit) return;
    const as = { citation, reference: fullReference, pandoc, latex }[form];
    oninsert(as(hit.item));
  }

  /** A file's name, from its path. */
  const nameOf = (path: string) => path.split("/").pop() ?? path;

  onMount(() => {
    input.focus();
    nearest = text.trim() !== "";
    search();
    findSources()
      .then((s) => (bibs = s))
      .catch(() => {});
    return () => clearTimeout(timer);
  });
</script>

{#snippet marks(spans: Span[])}{#each spans as span}{#if span.mark}<b>{span.text}</b>{:else}{span.text}{/if}{/each}{/snippet}

<div class="refs" onfocusout={left} role="dialog" aria-label="References" tabindex="-1">
  <input
    bind:this={input}
    bind:value={query}
    oninput={typed}
    onkeydown={key}
    placeholder="Search the references: words, author, year"
    spellcheck="false"
    autocomplete="off"
    autocorrect="off"
  />
  {#if nearest}
    <p class="closest">Closest to the selected reference: <span>{text.trim()}</span></p>
  {/if}
  <ul bind:this={list}>
    {#each hits as h, i (h.item.id)}
      <!-- mousedown kept from the input: the click follows -->
      <li class:on={i === marked} onmousedown={(e) => e.preventDefault()} onclick={() => oninsert(citation(h.item))} onmousemove={() => (marked = i)} role="presentation">
        <span class="lines">
          <span class="title">{@render marks(h.title)}</span>
          <span class="subtitle">{@render marks(h.subtitle)}</span>
        </span>
        {#if h.item.files.length}
          <button class="pdf" onmousedown={(e) => e.preventDefault()} onclick={(e) => (e.stopPropagation(), pdf(h.item))} title="Open the PDF in a tab (⌘↵)">PDF</button>
        {/if}
      </li>
    {:else}
      <li class="none" class:error>{error || (found ? "Nothing found" : "")}</li>
    {/each}
  </ul>
  <footer>
    <span class="keys">{enter === "pandoc" ? "↵ [@key]" : "↵ cite · ⌘. [@key]"} · ⇧↵ reference · ⌘L \cite · {hit?.item.files.length ? "⌘↵ PDF · " : ""}esc</span>
    <span class="sources" title={bibs.join("\n")}>
      {#if found && found.total > hits.length}{hits.length} of {found.total} ·{/if}
      {bibs.map(nameOf).join(", ")}
    </span>
  </footer>
</div>

<style>
  .refs {
    position: absolute;
    z-index: 5;
    inset: 8px 14px;
    max-width: 780px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 4px 18px rgba(0, 0, 0, 0.2);
    overflow: hidden;
  }
  input {
    flex: none;
    font: inherit;
    color: var(--fg);
    background: var(--chip);
    border: none;
    border-bottom: 1px solid var(--line);
    padding: 6px 10px;
    outline: none;
    -webkit-user-select: text;
    user-select: text;
  }
  .closest {
    flex: none;
    margin: 0;
    padding: 4px 10px;
    color: var(--muted);
    font-size: 11.5px;
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .closest span {
    font-style: italic;
  }
  ul {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 3px;
    list-style: none;
    overflow-y: auto;
  }
  li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 8px;
    border-radius: 5px;
  }
  li.on {
    background: var(--selection);
  }
  .lines {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .lines > span {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    line-height: 16px;
  }
  .subtitle {
    color: var(--muted);
    font-size: 11px;
    line-height: 14px;
  }
  b {
    font-weight: 700;
  }
  .pdf {
    flex: none;
    font: inherit;
    font-size: 10.5px;
    color: var(--muted);
    background: var(--chip);
    border: none;
    border-radius: 4px;
    padding: 0 5px;
  }
  .pdf:hover {
    color: var(--fg);
  }
  .none {
    color: var(--muted);
  }
  .none.error {
    color: var(--error);
  }
  footer {
    flex: none;
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 10px;
    color: var(--muted);
    font-size: 11.5px;
    border-top: 1px solid var(--line);
    white-space: nowrap;
  }
  .sources {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
