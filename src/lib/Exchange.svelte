<script lang="ts">
  // File ▸ Exchange: a review that comes in or goes out without the app on the other side. The menu's items come to
  // `run`; the document is written first (`flush`), then a sheet asks what is needed and the backend does the rest.
  //
  // Compare as Suggestions…: the revisions of the document in git (or a branch, a tag, any revision typed) or an edited
  // copy of the file, and whose edits they are (the commit's author, filled in; for a file the name is asked). Each
  // change becomes a suggestion under that name.
  //
  // Export as Word with Changes…: where to save the .docx (a dialog), then a sheet that says how many tracked changes
  // and comments it has and lists what could not go to Word (suggestions that overlap, in the front matter).
  //
  // Import Word Review…: a .docx (a dialog). The tracked changes and comments come in under their authors' names. Changes
  // made with Track Changes off are looked for only when asked (the whole text is compared, with the document made
  // into a .docx as an export does; Word names no one for them, so the sheet asks whose they are). A sheet says how
  // many came in, and lists what could not be placed in the text.

  import { invoke } from "@tauri-apps/api/core";

  type Revision = { rev: string; label: string; kind: "tag" | "commit"; detail: string };

  let { flush, onfocus }: { flush: () => unknown; onfocus?: () => void } = $props();

  type Compare = { revisions: Revision[]; chosen: string; typed: string; file: string; name: string; email: string; busy: boolean; done: string };
  let comparing = $state<Compare | null>(null);
  type Exported = { path: string; suggestions: number; comments: number; skipped: string[]; notes: string[] };
  /** the sheet of an export: what it made */
  let exported = $state<Exported | null>(null);
  type Imported = { suggestions: number; comments: number; authors: string[]; unplaced: string[] };
  /** the sheet of an import: the file, the author of untracked changes, and what came of it */
  let importing = $state<{ file: string; untracked: boolean; author: string; busy: boolean; result: Imported | null } | null>(null);
  let problem = $state("");

  /** The menu's items: "compare", "word-export", "word-import". */
  export async function run(what: string) {
    if (what !== "compare" && what !== "word-export" && what !== "word-import") return;
    problem = "";
    await flush();
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
    try {
      const revisions = await invoke<Revision[]>("revisions");
      comparing = { revisions, chosen: "", typed: "", file: "", name: "", email: "", busy: false, done: "" };
    } catch (e) {
      problem = String(e);
    }
  }

  const nameOf = (file: string) => file.split("/").pop() ?? file;
  /** the revision to compare with: the one typed, else the one chosen in the list */
  const revision = $derived(comparing ? comparing.typed.trim() || comparing.chosen : "");
  const ready = $derived(!!comparing && !comparing.busy && !!comparing.name.trim() && (!!revision || !!comparing.file));

  /** The author of the commit named, unless a name was typed already. */
  async function author() {
    if (!comparing || !revision || comparing.name.trim()) return;
    try {
      const who = await invoke<{ name: string; email: string } | null>("revision_author", { revision });
      if (who && comparing && !comparing.name.trim()) [comparing.name, comparing.email] = [who.name, who.email];
    } catch {}
  }

  async function chooseFile() {
    try {
      const file = await invoke<string | null>("choose_edited_copy");
      if (file && comparing) [comparing.file, comparing.chosen, comparing.typed] = [file, "", ""];
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
      const n = await invoke<number>("compare_as_suggestions", { source, author: { name: c.name.trim(), email: c.email.trim() || null } });
      c.done = n ? `${n} suggestion${n === 1 ? "" : "s"} added under ${c.name.trim()}.` : "No difference: nothing to suggest.";
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
    } catch (e) {
      problem = String(e);
    }
    i.busy = false;
  }

  function close() {
    comparing = null;
    importing = null;
    exported = null;
    problem = "";
    onfocus?.();
  }

  /** The keys to a sheet as it opens, so Esc reaches it. */
  const grab = (node: HTMLElement) => node.focus();
</script>

{#if comparing}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="sheet" role="dialog" aria-label="Compare as Suggestions" tabindex="-1" onkeydown={(e) => e.key === "Escape" && close()} use:grab>
    <header>
      <b>Compare as Suggestions</b>
      <button onclick={close}>{comparing.done ? "Close" : "Cancel"}</button>
    </header>
    <p class="where">The changes from this document to the other version become suggestions under the name of whoever made it.</p>
    <label>
      Commit or tag
      <select bind:value={comparing.chosen} onchange={() => { comparing!.typed = ""; comparing!.file = ""; author(); }}>
        <option value="">{comparing.revisions.length ? "Choose…" : "none (not in a repository)"}</option>
        {#each comparing.revisions as v (v.kind + v.rev)}
          <option value={v.rev}>{v.label}: {v.detail}</option>
        {/each}
      </select>
    </label>
    <label>
      or a branch or any revision
      <input bind:value={comparing.typed} placeholder="feature/jane, HEAD~3" oninput={() => (comparing!.file = "")} onchange={author} spellcheck="false" />
    </label>
    <div class="row">
      <span>or an edited copy</span>
      <button onclick={chooseFile}>Choose file…</button>
      {#if comparing.file}<span class="where" title={comparing.file}>{nameOf(comparing.file)}</span>{/if}
    </div>
    <label>
      Edits made by
      <input bind:value={comparing.name} placeholder="Name" spellcheck="false" />
    </label>
    <label>
      Email (optional)
      <input bind:value={comparing.email} placeholder="name@example.org" spellcheck="false" />
    </label>
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
  select,
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
