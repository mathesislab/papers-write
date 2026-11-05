<script lang="ts">
  // The suggestions and comments of the review, open ones in the order of the text: who and when, what is changed
  // (the old text struck through, the new beside it; a comment's quote and body), how it sits on the text now, its
  // thread (replies and counter-suggestions), and what the user can do: accept or reject, reply, resolve, suggest
  // instead, withdraw their own. A click brings the item into view in the text. Everything goes through the
  // ReviewSession, which writes the review files; the panel only asks.

  import type { ReviewSession } from "$lib/review.svelte";
  import type { ReviewItem } from "papers-write/editor";
  import type { ReviewRecord } from "$lib/editor/review/types";
  import { ago, colorOf, whoIs } from "$lib/reviewFormat";

  let { session, offer = "", oncommit, ondismiss }: { session: ReviewSession; offer?: string; oncommit?: () => void; ondismiss?: () => void } = $props();

  /** the person shown ("" for everyone, else a slug) and whether the decided ones show too */
  let who = $state("");
  let decided = $state(false);
  /** the form open: a reply to, or a counter-suggestion to, an item id; and its text */
  let form = $state<{ kind: "reply" | "counter"; on: string; text: string } | null>(null);

  const slugOf = (id: string) => id.replace(/-\d+$/, "");
  const mine = (id: string) => slugOf(id) === session.me?.slug;
  const name = (id: string) => whoIs(id, session.people);
  const byId = $derived(new Map(session.items.map((i) => [i.id, i])));
  const people = $derived([...new Set(session.items.map((i) => slugOf(i.id)))].sort());
  /** the items shown: a counter-suggestion is in the thread of the suggestion it answers, when that is there */
  const shown = $derived(
    session.items.filter((i) => !(i.on && byId.has(i.on)) && (decided || i.state === "open") && (!who || slugOf(i.id) === who)),
  );
  const hidden = $derived(session.items.length - session.items.filter((i) => i.state === "open").length);

  const old = (i: ReviewItem | ReviewRecord) => ("record" in i ? i.record.exact : i.exact) ?? "";
  const insertion = (i: ReviewItem) => i.replacement ?? i.record.insert ?? "";
  const label = (i: ReviewItem) =>
    i.state !== "open" ? i.state : i.status.type === "exact" ? "" : i.status.type;

  function open(kind: "reply" | "counter", item: ReviewItem) {
    form = { kind, on: item.id, text: kind === "counter" ? insertion(item) : "" };
  }

  async function send() {
    const f = form;
    const item = f && byId.get(f.on);
    if (!f || !item || (f.kind === "reply" && !f.text.trim())) return;
    form = null;
    await (f.kind === "reply" ? session.reply(f.on, f.text) : session.counter(item, f.text)).catch(() => {});
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") form = null;
    else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) void send();
    else return;
    e.preventDefault();
  }

  const grab = (node: HTMLTextAreaElement) => node.focus();
  const run = (e: Event, go: () => Promise<void>) => {
    e.stopPropagation();
    go().catch(() => {});
  };

  /** the item the text pointed at, scrolled into the panel */
  let list = $state<HTMLElement>();
  $effect(() => {
    const id = session.selected;
    if (id) list?.querySelector(`[data-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "nearest" });
  });
</script>

{#snippet change(item: ReviewItem)}
  {#if item.kind === "suggestion"}
    <p class="change">
      {#if old(item)}<del>{old(item)}</del>{/if}
      {#if old(item)}<span class="arrow">→</span>{/if}
      {#if insertion(item)}<ins>{insertion(item)}</ins>{:else}<em>deleted</em>{/if}
    </p>
    {#if item.record.text}<p class="text">{item.record.text}</p>{/if}
    {#if item.status.type === "outdated"}<p class="now">now: {item.status.now || "(nothing)"}</p>{/if}
  {:else}
    {#if old(item)}<blockquote>{old(item)}</blockquote>{/if}
    <p class="text">{item.record.text}</p>
  {/if}
{/snippet}

{#snippet actions(item: ReviewItem)}
  {#if item.state === "open"}
    <div class="actions">
      {#if item.kind === "suggestion"}
        <button class="primary" onclick={(e) => run(e, () => session.accept(item))}>Accept</button>
        <button onclick={(e) => run(e, () => session.reject(item))}>Reject</button>
        <button onclick={(e) => (e.stopPropagation(), open("counter", item))}>Suggest Instead</button>
      {:else}
        <button onclick={(e) => run(e, () => session.decide(item.id, "resolved"))}>Resolve</button>
      {/if}
      <button onclick={(e) => (e.stopPropagation(), open("reply", item))}>Reply</button>
      {#if mine(item.id)}<button onclick={(e) => run(e, () => session.decide(item.id, "withdrawn"))}>Withdraw</button>{/if}
    </div>
  {:else if item.state === "resolved" || item.state === "rejected"}
    <div class="actions"><button onclick={(e) => run(e, () => session.decide(item.id, "reopened"))}>Reopen</button></div>
  {/if}
  {#if form && form.on === item.id}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="form" onclick={(e) => e.stopPropagation()}>
      <textarea bind:value={form.text} use:grab onkeydown={key} rows="2" placeholder={form.kind === "reply" ? "Reply" : "Instead"}></textarea>
      <div class="actions">
        <span>⌘↵</span>
        <button onclick={() => (form = null)}>Cancel</button>
        <button class="primary" onclick={send}>{form.kind === "reply" ? "Reply" : "Suggest"}</button>
      </div>
    </div>
  {/if}
{/snippet}

<aside aria-label="Review">
  <header>
    <b>Review</b>
    <select bind:value={who} aria-label="Show">
      <option value="">Everyone</option>
      {#each people as slug (slug)}<option value={slug}>{whoIs(slug + "-0", session.people)}</option>{/each}
    </select>
    <label title="Show accepted, rejected and resolved items too"><input type="checkbox" bind:checked={decided} /> Resolved{hidden ? ` (${hidden})` : ""}</label>
  </header>
  <div class="list" bind:this={list}>
    {#if !session.me}
      <p class="empty">Save the document first, to review it.</p>
    {:else if !shown.length}
      <p class="empty">No {decided ? "" : "open "}suggestions or comments{who ? " by this person" : ""}.</p>
    {/if}
    {#each shown as item (item.id)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
      <article data-id={item.id} class:selected={session.selected === item.id} class:done={item.state !== "open"} onclick={() => session.show(item.id)}>
        <div class="head">
          <i style:background={colorOf(slugOf(item.id))}></i>
          <b>{item.author || name(item.id)}</b>
          <time>{ago(item.record.time)}</time>
          {#if label(item)}<span class="chip" class:warn={item.status.type === "outdated" || item.status.type === "orphaned"}>{label(item)}</span>{/if}
        </div>
        {@render change(item)}
        {#each item.thread as r (r.id)}
          {@const counter = byId.get(r.id)}
          <div class="reply" data-id={r.id} class:selected={session.selected === r.id}>
            <div class="head">
              <i style:background={colorOf(slugOf(r.id))}></i>
              <b>{name(r.id)}</b>
              <time>{ago(r.time)}</time>
              {#if counter && counter.state !== "open"}<span class="chip">{counter.state}</span>{/if}
            </div>
            {#if r.kind === "suggestion"}
              <p class="change">
                {#if counter}{#if old(counter)}<del>{old(counter)}</del> <span class="arrow">→</span>{/if}{/if}
                {#if counter ? insertion(counter) : r.insert}<ins>{counter ? insertion(counter) : r.insert}</ins>{:else}<em>deleted</em>{/if}
              </p>
              {#if r.text}<p class="text">{r.text}</p>{/if}
              {#if counter}{@render actions(counter)}{/if}
            {:else}
              <p class="text">{r.text}</p>
            {/if}
          </div>
        {/each}
        {@render actions(item)}
      </article>
    {/each}
  </div>
  {#if offer}
    <div class="offer">
      <span>Commit the decisions?</span>
      <button class="primary" onclick={oncommit}>Commit…</button>
      <button onclick={ondismiss} aria-label="Not now" title="Not now">×</button>
    </div>
  {/if}
  {#if session.errors.length}
    <footer>
      {#each session.errors as e (e)}<p>{e}</p>{/each}
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
  header select {
    flex: 1;
    min-width: 0;
    font: inherit;
    color: var(--fg);
    background: var(--menu-bg);
    border: 1px solid var(--border);
    border-radius: 5px;
  }
  header label {
    white-space: nowrap;
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
  article.selected {
    border-color: var(--accent);
  }
  article.done {
    opacity: 0.65;
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
  time {
    color: var(--muted);
    font-size: 11.5px;
  }
  .chip {
    margin-left: auto;
    padding: 0 6px;
    font-size: 11px;
    color: var(--muted);
    background: var(--chip);
    border-radius: 8px;
  }
  .chip.warn {
    color: var(--error);
  }
  p {
    margin: 4px 0 0;
    overflow-wrap: anywhere;
  }
  .change del {
    color: var(--error);
  }
  .change ins {
    text-decoration: none;
    background: var(--selection);
    border-radius: 2px;
  }
  .arrow,
  .now,
  em {
    color: var(--muted);
  }
  blockquote {
    margin: 4px 0 0;
    padding-left: 8px;
    border-left: 2px solid var(--border);
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .text {
    white-space: pre-wrap;
  }
  .reply {
    margin: 8px 0 0 6px;
    padding-left: 8px;
    border-left: 2px solid var(--line);
  }
  .reply.selected {
    border-left-color: var(--accent);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
    margin-top: 6px;
  }
  .actions span {
    flex: 1;
    color: var(--muted);
    font-size: 11.5px;
  }
  button {
    font: inherit;
    color: var(--fg);
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 0 7px;
  }
  button:hover {
    background: var(--chip);
  }
  button.primary {
    color: var(--on-accent);
    background: var(--accent);
    border-color: var(--accent);
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    margin-top: 6px;
    resize: vertical;
    font: inherit;
    color: var(--fg);
    background: var(--menu-bg);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 4px 6px;
    outline: none;
  }
  textarea:focus {
    border-color: var(--accent);
  }
  .offer {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-top: 1px solid var(--line);
  }
  .offer span {
    flex: 1;
    color: var(--muted);
  }
  footer {
    padding: 6px 10px;
    border-top: 1px solid var(--line);
    color: var(--error);
    font-size: 11.5px;
  }
  footer p {
    margin: 0 0 2px;
    overflow-wrap: anywhere;
  }
</style>
