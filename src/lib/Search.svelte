<script lang="ts">
  // The search of the project, a tab of the sidebar (⇧⌘F): the text to find (case, whole word, regular expression),
  // the matches in the project's text files (`search_project`) asked 250 ms after the typing stops, grouped by file;
  // a click opens the file at the match (`open_at`). Replace All asks first how many, in how many files, then replaces
  // (`replace_in_project`): in a file open in a window its editor does it (undo, suggestions), others are written.

  import { invoke } from "@tauri-apps/api/core";

  type Query = { text: string; case: boolean; word: boolean; regex: boolean };
  type Hit = { line: number; from: number; to: number; text: string; at: number; end: number };
  type FileHits = { path: string; hits: Hit[] };

  let { root, onerror }: { root: string; onerror: (what: string) => void } = $props();

  let text = $state("");
  let replacement = $state("");
  let options = $state({ case: false, word: false, regex: false });
  let results = $state.raw<FileHits[]>([]);
  let problem = $state("");
  let busy = $state(false);
  /** the files closed in the list */
  let closed = $state(new Set<string>());
  /** Replace All waiting for its yes */
  let confirming = $state(false);
  let field = $state<HTMLInputElement>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let seq = 0;

  const query = (): Query => ({ text, ...options });
  const count = $derived(results.reduce((n, f) => n + f.hits.length, 0));

  /** The field to type in, its text selected. */
  export function focus() {
    field?.focus();
    field?.select();
  }

  /** The search again, as it is (the files may have changed). */
  export function again() {
    later(0);
  }

  function later(ms = 250) {
    clearTimeout(timer);
    confirming = false;
    timer = setTimeout(run, ms);
  }

  async function run() {
    const mine = ++seq;
    if (!text) {
      [results, problem] = [[], ""];
      return;
    }
    busy = true;
    try {
      const found = await invoke<FileHits[]>("search_project", { root, query: query() });
      if (mine === seq) [results, problem] = [found, ""];
    } catch (e) {
      if (mine === seq) [results, problem] = [[], String(e)];
    } finally {
      if (mine === seq) busy = false;
    }
  }

  $effect(() => {
    root;
    later(0);
  });

  function toggle(key: keyof typeof options) {
    options[key] = !options[key];
    later(0);
  }

  function fold(path: string) {
    const next = new Set(closed);
    if (!next.delete(path)) next.add(path);
    closed = next;
  }

  async function open(file: FileHits, hit: Hit) {
    try {
      await invoke("open_at", { path: file.path, at: hit.at, end: hit.end });
    } catch (e) {
      onerror(String(e));
    }
  }

  async function replaceAll() {
    confirming = false;
    try {
      const done = await invoke<number>("replace_in_project", { root, query: query(), with: replacement, paths: results.map((f) => f.path) });
      onerror(`${done} replaced`);
    } catch (e) {
      onerror(String(e));
    }
    later(400);
  }

  /** The path below the project's folder, without the file's name. */
  const folderOf = (path: string) => {
    const rel = path.startsWith(root + "/") ? path.slice(root.length + 1) : path;
    return rel.includes("/") ? rel.slice(0, rel.lastIndexOf("/")) : "";
  };
  const nameOf = (path: string) => path.slice(path.lastIndexOf("/") + 1);
</script>

<div class="search">
  <div class="fields">
    <div class="row">
      <input
        bind:this={field}
        bind:value={text}
        class:bad={!!problem}
        oninput={() => later()}
        onkeydown={(e) => e.key === "Enter" && later(0)}
        placeholder="Find in project"
        aria-label="Find in project"
        spellcheck="false"
        autocomplete="off"
      />
    </div>
    <div class="row toggles">
      <button class:on={options.case} onclick={() => toggle("case")} title="Match case">Aa</button>
      <button class:on={options.word} onclick={() => toggle("word")} title="Whole words">W</button>
      <button class:on={options.regex} onclick={() => toggle("regex")} title="Regular expression">.*</button>
      <span class="count">{busy ? "…" : text ? (problem ? "" : `${count} in ${results.length} file${results.length === 1 ? "" : "s"}`) : ""}</span>
    </div>
    <div class="row">
      <input bind:value={replacement} oninput={() => (confirming = false)} placeholder="Replace" aria-label="Replace" spellcheck="false" autocomplete="off" />
    </div>
    <div class="row">
      {#if confirming}
        <span class="ask">Replace {count} in {results.length} file{results.length === 1 ? "" : "s"}?</span>
        <button class="push" onclick={replaceAll}>Replace</button>
        <button onclick={() => (confirming = false)}>Cancel</button>
      {:else}
        <button class="push" disabled={!count} onclick={() => (confirming = true)}>Replace All…</button>
      {/if}
    </div>
    {#if problem}<p class="problem">{problem}</p>{/if}
  </div>
  <div class="results">
    {#each results as file (file.path)}
      {@const open_ = !closed.has(file.path)}
      <button class="file" onclick={() => fold(file.path)} title={file.path}>
        <svg class="chevron" class:open={open_} viewBox="0 0 12 12" width="10" height="10" aria-hidden="true">
          <path d="M4 2.25 7.75 6 4 9.75" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
        <span class="name">{nameOf(file.path)}</span>
        <span class="dir">{folderOf(file.path)}</span>
        <span class="n">{file.hits.length}</span>
      </button>
      {#if open_}
        {#each file.hits as hit, i (i)}
          <!-- a match far along its line: the text from a little before it -->
          {@const start = hit.from > 24 ? hit.from - 16 : 0}
          <button class="hit" onclick={() => open(file, hit)} title="Line {hit.line}">
            <span class="line">{hit.line}</span>
            <span class="text">{start ? "…" : ""}{hit.text.slice(start, hit.from)}<mark>{hit.text.slice(hit.from, hit.to)}</mark>{hit.text.slice(hit.to)}</span>
          </button>
        {/each}
      {/if}
    {/each}
  </div>
</div>

<style>
  .search {
    display: flex;
    flex-direction: column;
    height: 100%;
    font-size: 12px;
  }
  .fields {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 8px 8px 8px;
    border-bottom: 1px solid var(--line);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  input {
    flex: 1;
    min-width: 0;
    font: inherit;
    color: var(--fg);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 3px 7px;
    outline: none;
    -webkit-user-select: text;
    user-select: text;
  }
  input:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--selection);
  }
  input.bad {
    border-color: var(--error);
  }
  button {
    font: inherit;
    color: var(--muted);
    background: none;
    border: 1px solid transparent;
    border-radius: 5px;
    padding: 1px 6px;
  }
  button:hover:not(:disabled) {
    color: var(--fg);
    background: var(--chip);
  }
  .toggles button {
    font-family: ui-monospace, Menlo, monospace;
    font-size: 11px;
    min-width: 24px;
  }
  .toggles button.on {
    color: var(--accent);
    background: var(--selection);
  }
  .count {
    margin-left: auto;
    color: var(--muted);
    font-size: 11px;
    white-space: nowrap;
  }
  .push {
    color: var(--fg);
    background: var(--bg);
    border-color: var(--border);
  }
  .push:disabled {
    opacity: 0.5;
  }
  .ask {
    flex: 1;
    min-width: 0;
    color: var(--fg);
  }
  .problem {
    margin: 0;
    color: var(--error);
    font-size: 11px;
  }
  .results {
    flex: 1;
    overflow: auto;
    padding: 4px 0 20px;
  }
  .file,
  .hit {
    display: flex;
    align-items: center;
    gap: 5px;
    width: 100%;
    border-radius: 0;
    border: none;
    text-align: left;
    white-space: nowrap;
  }
  .file {
    padding: 3px 8px;
    color: var(--fg);
    font-weight: 600;
  }
  .chevron {
    flex: none;
    color: var(--muted);
    transition: transform 0.12s ease;
  }
  .chevron.open {
    transform: rotate(90deg);
  }
  .name {
    flex: none;
    max-width: 60%;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dir {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--muted);
    font-weight: 400;
    font-size: 11px;
  }
  .n {
    flex: none;
    color: var(--muted);
    font-weight: 400;
    font-size: 10.5px;
  }
  .hit {
    padding: 2px 8px 2px 22px;
    color: var(--fg);
  }
  .line {
    flex: none;
    min-width: 22px;
    color: var(--muted);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  .text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  mark {
    color: inherit;
    background: rgba(255, 196, 0, 0.4);
    border-radius: 2px;
  }
</style>
