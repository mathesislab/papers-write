<script lang="ts">
  // File ▸ Exchange: a review that comes in or goes out without the app on the other side. The menu's items come to
  // `run`; the document is written first (`flush`), then a sheet asks what is needed and the backend does the rest.
  //
  // Compare as Suggestions…: a branch or a commit (typed), or an edited copy of the file. From git, what was done there
  // since it left this branch's history, each change under the author of its commit (Git ▸ Review Branch… lists the
  // branches); for a file, the changes from the text now, under the name asked for. What it added is logged (`onlog`)
  // for the message of the next commit.
  //
  // Export as Word with Changes…: where to save the .docx (a dialog), then a sheet that says how many tracked changes
  // and comments it has and lists what could not go to Word (suggestions that overlap, in the front matter).
  //
  // Import Word Review…: a .docx (a dialog). The tracked changes and comments come in under their authors' names. Changes
  // made with Track Changes off are looked for only when asked (the whole text is compared, with the document made
  // into a .docx as an export does; Word names no one for them, so the sheet asks whose they are). A sheet says how
  // many came in, and lists what could not be placed in the text.
  //
  // Send for Review…: the round (a name, "Draft of <date>" to begin with), then where to save the `.review` file (a
  // dialog); a sheet says what went into it. The document opened from a `.review` has a quiet bar at the top (the
  // package says whose draft it is): Send Back… makes the file of the user's own suggestions and comments to return
  // (also in File ▸ Exchange).

  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "$lib/events";
  import { said, type Added } from "$lib/git";

  export type Reviewing = { name: string; email: string; round: string; document: string };
  let {
    flush,
    onfocus,
    onlog,
    reviewing = $bindable(null),
    mode = "edit",
  }: {
    flush: () => unknown;
    onfocus?: () => void;
    /** what was done to the review, for the message of the next commit: "review of jane/edits as suggestions" */
    onlog?: (what: string) => void;
    reviewing?: Reviewing | null;
    mode?: string;
  } = $props();

  type Sent = { path: string; figures: number; bibliographies: number; skipped: string[] };
  type Returned = { path: string; suggestions: number; comments: number; to: string };
  /** the sheet of Send for Review: the round, and what went into the file */
  let sending = $state<{ round: string; busy: boolean; result: Sent | null } | null>(null);
  /** the sheet of Send Back: what went into the file */
  let returned = $state<Returned | null>(null);

  type Compare = { typed: string; file: string; name: string; email: string; busy: boolean; done: string };
  let comparing = $state<Compare | null>(null);
  type Exported = { path: string; suggestions: number; comments: number; skipped: string[]; notes: string[] };
  /** the sheet of an export: what it made */
  let exported = $state<Exported | null>(null);
  type Imported = { suggestions: number; comments: number; authors: string[]; unplaced: string[] };
  /** the sheet of an import: the file, the author of untracked changes, and what came of it */
  let importing = $state<{ file: string; untracked: boolean; author: string; busy: boolean; result: Imported | null } | null>(null);
  let problem = $state("");

  /** The menu's items: "send-review", "send-back", "compare", "word-export", "word-import". */
  export async function run(what: string) {
    if (!["send-review", "send-back", "compare", "word-export", "word-import"].includes(what)) return;
    problem = "";
    await flush();
    if (what === "send-review") {
      const day = new Date().toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
      sending = { round: `Draft of ${day}`, busy: false, result: null };
      return;
    }
    if (what === "send-back") return sendBack();
    if (what === "word-import") {
      importing = { file: "", untracked: false, author: "", busy: false, result: null };
      return chooseWord();
    }
    if (what === "word-export") {
      try {
        exported = (await invoke<Exported | null>("export_word", { path: null })) ?? exported;
      } catch (e) {
        problem = String(e);
      }
      return;
    }
    comparing = { typed: "", file: "", name: "", email: "", busy: false, done: "" };
  }

  async function sendReview() {
    const s = sending;
    if (!s || !s.round.trim() || s.busy) return;
    [s.busy, problem] = [true, ""];
    try {
      s.result = await invoke<Sent | null>("send_for_review", { round: s.round.trim() });
      if (!s.result) sending = null;
    } catch (e) {
      problem = String(e);
    }
    s.busy = false;
  }

  /** Send Back…: the file of the user's own review, where a dialog says. */
  async function sendBack() {
    problem = "";
    await flush();
    try {
      returned = (await invoke<Returned | null>("send_back")) ?? returned;
    } catch (e) {
      problem = String(e);
    }
  }

  /** What this window is a review of (a document opened from a `.review`), asked again when it shows another file. */
  async function whose() {
    try {
      reviewing = await invoke<Reviewing | null>("review_package");
    } catch {
      reviewing = null;
    }
  }

  onMount(() => {
    whose();
    const unlisten = listen("opened", whose);
    return () => {
      unlisten.then((f) => f());
    };
  });

  const nameOf = (file: string) => file.split("/").pop() ?? file;

  /** the branch or commit to compare with (a file is chosen instead) */
  const revision = $derived(comparing ? comparing.typed.trim() : "");
  const ready = $derived(!!comparing && !comparing.busy && (!!revision || (!!comparing.file && !!comparing.name.trim())));

  async function chooseFile() {
    try {
      const file = await invoke<string | null>("choose_edited_copy");
      if (file && comparing) [comparing.file, comparing.typed] = [file, ""];
    } catch (e) {
      problem = String(e);
    }
  }

  async function compare() {
    const c = comparing;
    if (!c || !ready) return;
    [c.busy, c.done, problem] = [true, "", ""];
    try {
      const source = revision ? { revision } : { file: c.file };
      const author = revision ? null : { name: c.name.trim(), email: c.email.trim() || null };
      const made = await invoke<Added>("compare_as_suggestions", { source, author });
      c.done = said(made);
      if (made.added) onlog?.(`review of ${revision || nameOf(c.file)} as suggestions`);
    } catch (e) {
      problem = String(e);
    }
    c.busy = false;
  }

  async function chooseWord() {
    try {
      const file = await invoke<string | null>("choose_word_file");
      if (file && importing) importing.file = file;
    } catch (e) {
      problem = String(e);
    }
  }

  async function importWord() {
    const i = importing;
    if (!i || !i.file || i.busy) return;
    [i.busy, problem] = [true, ""];
    try {
      i.result = await invoke<Imported>("import_word", { path: i.file, alsoUntracked: i.untracked, fallbackAuthor: i.author.trim() });
      if (i.result.suggestions + i.result.comments) onlog?.(`import of the Word review ${nameOf(i.file)}`);
    } catch (e) {
      problem = String(e);
    }
    i.busy = false;
  }

  function close() {
    sending = null;
    returned = null;
    comparing = null;
    importing = null;
    exported = null;
    problem = "";
    onfocus?.();
  }

  /** The keys to a sheet as it opens, so Esc reaches it. */
  const grab = (node: HTMLElement) => node.focus();
</script>

{#if reviewing}
  <div class="bar">
    <span>
      A review of {reviewing.name}'s draft ({reviewing.round}). Your suggestions stay beside the text.{#if mode !== "suggest"}
        Edits made in the text itself will not go back: only suggestions and comments do.{/if}
    </span>
    <button onclick={sendBack}>Send Back…</button>
  </div>
{/if}
{#if sending}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="sheet" role="dialog" aria-label="Send for Review" tabindex="-1" onkeydown={(e) => e.key === "Escape" && close()} use:grab>
    <header>
      <b>Send for Review</b>
      <button onclick={close}>{sending.result ? "Close" : "Cancel"}</button>
    </header>
    {#if sending.result}
      {@const r = sending.result}
      <p>
        <code title={r.path}>{nameOf(r.path)}</code>: the document, {r.figures} figure{r.figures === 1 ? "" : "s"}, {r.bibliographies}
        bibliograph{r.bibliographies === 1 ? "y" : "ies"} and its review.
      </p>
      {#if r.skipped.length}
        <p class="where">Not included ({r.skipped.length}):</p>
        <ul>{#each r.skipped as line (line)}<li>{line}</li>{/each}</ul>
      {/if}
      <div class="buttons"><button onclick={() => invoke("reveal", { path: r.path }).catch((e) => (problem = String(e)))}>Reveal in Finder</button></div>
    {:else}
      <p class="where">
        One file for someone who has Papers Write: this document as it is now, the figures and bibliography it shows, and the review so far. Their suggestions and
        comments come back in a file of their own.
      </p>
      <label>
        Round
        <input bind:value={sending.round} spellcheck="false" onkeydown={(e) => e.key === "Enter" && sendReview()} />
      </label>
      <div class="buttons"><button class="go" disabled={!sending.round.trim() || sending.busy} onclick={sendReview}>Save…</button></div>
    {/if}
  </div>
{/if}
{#if returned}
  {@const r = returned}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="sheet" role="dialog" aria-label="Send Back" tabindex="-1" onkeydown={(e) => e.key === "Escape" && close()} use:grab>
    <header>
      <b>Review to send back</b>
      <button onclick={close}>Close</button>
    </header>
    <p>
      <code title={r.path}>{nameOf(r.path)}</code>: {r.suggestions} suggestion{r.suggestions === 1 ? "" : "s"} and {r.comments} comment{r.comments === 1 ? "" : "s"}
      for {r.to}.
    </p>
    <div class="buttons"><button onclick={() => invoke("reveal", { path: r.path }).catch((e) => (problem = String(e)))}>Reveal in Finder</button></div>
  </div>
{/if}
{#if comparing}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="sheet" role="dialog" aria-label="Compare as Suggestions" tabindex="-1" onkeydown={(e) => e.key === "Escape" && close()} use:grab>
    <header>
      <b>Compare as Suggestions</b>
      <button onclick={close}>{comparing.done ? "Close" : "Cancel"}</button>
    </header>
    <p class="where">
      A branch or a commit: what was done there since it left this branch, each change under the author of its commit (your own edits since are kept). A file:
      the changes from this document to it.
    </p>
    <label>
      Branch or commit
      <input bind:value={comparing.typed} placeholder="jane/edits, origin/jane, a1b2c3d" oninput={() => (comparing!.file = "")} spellcheck="false" />
    </label>
    <div class="row">
      <span>or an edited copy</span>
      <button onclick={chooseFile}>Choose file…</button>
      {#if comparing.file}<span class="where" title={comparing.file}>{nameOf(comparing.file)}</span>{/if}
    </div>
    {#if !revision}
      <label>
        Edits made by
        <input bind:value={comparing.name} placeholder="Name" spellcheck="false" />
      </label>
      <label>
        Email (optional)
        <input bind:value={comparing.email} placeholder="name@example.org" spellcheck="false" />
      </label>
    {/if}
    <div class="buttons"><button class="go" disabled={!ready} onclick={compare}>Compare</button></div>
    {#if comparing.done}<p>{comparing.done}</p>{/if}
  </div>
{/if}
{#if exported}
  {@const path = exported.path}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="sheet" role="dialog" aria-label="Export as Word" tabindex="-1" onkeydown={(e) => e.key === "Escape" && close()} use:grab>
    <header>
      <b>Exported as Word</b>
      <button onclick={close}>Close</button>
    </header>
    <p>
      <code title={path}>{nameOf(path)}</code>: {exported.suggestions} tracked change{exported.suggestions === 1 ? "" : "s"} and
      {exported.comments} comment{exported.comments === 1 ? "" : "s"}.
    </p>
    {#each exported.notes as note (note)}<p class="where">{note}</p>{/each}
    {#if exported.skipped.length}
      <p class="where">Left out ({exported.skipped.length}):</p>
      <ul>{#each exported.skipped as line (line)}<li>{line}</li>{/each}</ul>
    {/if}
    <div class="buttons"><button onclick={() => invoke("reveal", { path }).catch((e) => (problem = String(e)))}>Reveal in Finder</button></div>
  </div>
{/if}
{#if importing}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="sheet" role="dialog" aria-label="Import Word Review" tabindex="-1" onkeydown={(e) => e.key === "Escape" && close()} use:grab>
    <header>
      <b>Import Word Review</b>
      <button onclick={close}>{importing.result ? "Close" : "Cancel"}</button>
    </header>
    <div class="row">
      <button onclick={chooseWord}>Choose .docx…</button>
      {#if importing.file}<span class="where" title={importing.file}>{nameOf(importing.file)}</span>{/if}
    </div>
    <label class="check">
      <input type="checkbox" bind:checked={importing.untracked} />
      Also changes made with Track Changes off (compares the whole text; may find differences that are only formatting)
    </label>
    {#if importing.untracked}
      <label>
        Those changes are by
        <input bind:value={importing.author} placeholder="Word" spellcheck="false" />
      </label>
    {/if}
    <p class="where">Tracked changes and comments come in under the names Word has for them; the text only, not Word's formatting.</p>
    <div class="buttons"><button class="go" disabled={!importing.file || importing.busy || !!importing.result} onclick={importWord}>Import</button></div>
    {#if importing.result}
      {@const r = importing.result}
      <p>
        {r.suggestions} suggestion{r.suggestions === 1 ? "" : "s"} and {r.comments} comment{r.comments === 1 ? "" : "s"}{r.authors.length
          ? ` by ${r.authors.join(", ")}`
          : ""}.
      </p>
      {#if r.unplaced.length}
        <p class="where">Not placed in the text ({r.unplaced.length}):</p>
        <ul>{#each r.unplaced as line (line)}<li>{line}</li>{/each}</ul>
      {/if}
    {/if}
  </div>
{/if}
{#if problem}<button class="mark error" onclick={() => (problem = "")} title="Dismiss">{problem}</button>{/if}

<style>
  .mark {
    position: absolute;
    z-index: 5;
    right: 14px;
    bottom: 10px;
    max-width: calc(100% - 28px);
    font: inherit;
    font-size: 12px;
    background: var(--chip);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 2px 10px;
    text-align: left;
  }
  .mark.error {
    color: var(--error);
  }
  /* at the top of the window, above the panes the page moves down by its height (--bar), whatever this component sits in */
  .bar {
    position: fixed;
    z-index: 4;
    top: 0;
    left: 0;
    right: 0;
    height: 28px;
    box-sizing: border-box;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    padding: 0 12px;
    background: var(--chip);
    border-bottom: 1px solid var(--line);
    color: var(--muted);
    font-size: 12px;
  }
  .bar span {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .bar button {
    flex: none;
  }
  .sheet {
    position: absolute;
    z-index: 6;
    top: 12px;
    right: 14px;
    width: min(460px, calc(100% - 28px));
    max-height: calc(100% - 60px);
    overflow: auto;
    box-sizing: border-box;
    padding: 10px 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.18);
    font-size: 12.5px;
    outline: none;
  }
  header,
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  .row {
    justify-content: flex-start;
    margin-top: 8px;
  }
  label {
    display: block;
    margin-top: 8px;
  }
  label.check {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  label.check input {
    display: inline;
    width: auto;
    flex: none;
  }
  input {
    display: block;
    width: 100%;
    box-sizing: border-box;
    margin-top: 2px;
    font: inherit;
    color: var(--fg);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 2px 6px;
  }
  button {
    font: inherit;
    color: var(--fg);
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 1px 6px;
  }
  button:hover:not(:disabled) {
    background: var(--chip);
  }
  button:disabled {
    opacity: 0.5;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    margin-top: 10px;
  }
  .where {
    color: var(--muted);
    font-size: 11.5px;
    overflow-wrap: anywhere;
    margin: 6px 0 0;
  }
  p {
    margin: 8px 0 0;
  }
  ul {
    margin: 4px 0 0;
    padding-left: 16px;
  }
</style>
