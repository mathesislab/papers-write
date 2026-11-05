<script lang="ts">
  // The Git menu: Commit… (⌥⌘K, or a click on the line at the foot of the window, or the offer of the review panel),
  // Pull and Push. The menu's items come to `run`; the document is written first (`flush`).
  //
  // Commit…: a sheet lists the files of the repository that changed. The document, its bibliography files and its
  // review folder are ticked; the rest is not. The message is prefilled when the app knows why (`message`: after
  // accepting suggestions); else, when only the review changed, as a review of the document. Commit runs `git add` of
  // the ticked files and `git commit` of them (never `-a`, never an amend); a failure is shown as git says it.
  //
  // Compare with Commit…: a tag or a commit that touched the document (or any revision typed), and a tab of its own
  // opens, read only: the text of that commit with the changes since drawn as tracked changes (the route `history`).
  //
  // Pull is `git pull --ff-only`: if the branch cannot go forward, nothing is done and a sheet says so; the rest is for
  // a terminal. Push is `git push`, never forced, and needs an upstream.

  import { invoke } from "@tauri-apps/api/core";
  import type { Changed } from "$lib/git";

  type Revision = { rev: string; label: string; kind: "tag" | "commit"; detail: string };
  type Candidate = Changed & { kind: "document" | "bibliography" | "review" | "other"; ticked: boolean };

  let {
    flush,
    text,
    docName,
    me,
    say,
    onchange,
    onfocus,
    oncommitted,
  }: {
    flush: () => unknown;
    /** the document as typed (it names its bibliography) */
    text: () => string;
    docName: () => string;
    /** the user's name, for the message of a review */
    me: () => string | undefined;
    /** told for a moment */
    say: (what: string) => void;
    /** git changed the repository: its state is read again */
    onchange: () => void;
    onfocus?: () => void;
    /** a commit made */
    oncommitted?: () => void;
  } = $props();

  type Sheet = { files: Candidate[]; ticks: boolean[]; message: string; busy: boolean; error: string };
  let committing = $state<Sheet | null>(null);
  /** the sheet of Compare with Commit…: the revisions to choose from, the one chosen or typed */
  let comparing = $state<{ revisions: Revision[]; chosen: string; typed: string; busy: boolean; error: string } | null>(null);
  /** what a pull or a push could not do, as git says it */
  let failure = $state<{ what: string; error: string } | null>(null);

  /** The menu's items: "git-commit", "git-pull", "git-push"; `message` prefills the commit's. */
  export async function run(what: string, message = "") {
    failure = null;
    if (!["git-commit", "git-pull", "git-push", "git-compare"].includes(what)) return;
    await flush();
    if (what === "git-commit") return open(message);
    if (what === "git-compare") return compareSheet();
    const [name, go, done] =
      what === "git-pull" ? (["Pull", "git_pull", "Pulled"] as const) : (["Push", "git_push", "Pushed"] as const);
    say(`${name}ing…`);
    try {
      say((await invoke<string>(go)) || done);
    } catch (e) {
      failure = { what: name, error: String(e) };
    }
    onchange();
  }

  async function open(message: string) {
    try {
      const files = await invoke<Candidate[]>("git_changes", { text: text() });
      if (!files.length) return say("Nothing to commit: no file has changed");
      const review = files.some((f) => f.kind === "review") && !files.some((f) => f.kind === "document");
      const given = message || (review && me() ? `Review of ${docName()} by ${me()}` : "");
      committing = { files, ticks: files.map((f) => f.ticked), message: given, busy: false, error: "" };
    } catch (e) {
      failure = { what: "Commit", error: String(e) };
    }
  }

  async function compareSheet() {
    try {
      comparing = { revisions: await invoke<Revision[]>("revisions"), chosen: "", typed: "", busy: false, error: "" };
    } catch (e) {
      failure = { what: "Compare with Commit", error: String(e) };
    }
  }

  const revision = $derived(comparing ? comparing.typed.trim() || comparing.chosen : "");

  async function compare() {
    const c = comparing;
    if (!c || !revision || c.busy) return;
    [c.busy, c.error] = [true, ""];
    try {
      await invoke("compare_with_commit", { rev: revision });
      comparing = null;
    } catch (e) {
      c.error = String(e);
      c.busy = false;
    }
  }

  const chosen = $derived(committing ? committing.files.filter((_, i) => committing!.ticks[i]) : []);
  const ready = $derived(!!committing && !committing.busy && chosen.length > 0 && !!committing.message.trim());

  async function commit() {
    const c = committing;
    if (!c || !ready) return;
    [c.busy, c.error] = [true, ""];
    try {
      say(await invoke<string>("git_commit", { paths: chosen.map((f) => f.path), message: c.message }));
      committing = null;
      oncommitted?.();
      onchange();
      onfocus?.();
    } catch (e) {
      c.error = String(e);
      c.busy = false;
      onchange();
    }
  }

  function close() {
    committing = null;
    comparing = null;
    failure = null;
    onfocus?.();
  }

  const key = (e: KeyboardEvent) => {
    if (e.key === "Escape") close();
    else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) void commit();
    else return;
    e.preventDefault();
  };
  /** The keys to a sheet as it opens, so Esc reaches it. */
  const grab = (node: HTMLElement) => node.focus();
</script>

{#if committing}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="sheet" role="dialog" aria-label="Commit" tabindex="-1" onkeydown={key}>
    <header>
      <b>Commit</b>
      <button onclick={close}>Cancel</button>
    </header>
    <ul>
      {#each committing.files as f, i (f.path)}
        <li class:other={f.kind === "other"}>
          <label title={f.path}>
            <input type="checkbox" bind:checked={committing.ticks[i]} />
            <code class="status">{f.status}</code>
            <span class="path">{f.from ? `${f.from} → ${f.path}` : f.path}</span>
          </label>
          {#if f.kind !== "other"}<small>{f.kind}</small>{/if}
        </li>
      {/each}
    </ul>
    <!-- the keys to the message as the sheet opens: typing writes it, ⌘↵ commits, Esc cancels -->
    <textarea rows="3" placeholder="Message (⌘↵ commits)" bind:value={committing.message} use:grab></textarea>
    {#if committing.error}<p class="error">{committing.error}</p>{/if}
    <div class="buttons">
      <button class="primary" disabled={!ready} onclick={commit}>Commit {chosen.length} file{chosen.length === 1 ? "" : "s"}</button>
    </div>
  </div>
{/if}
{#if comparing}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="sheet" role="dialog" aria-label="Compare with Commit" tabindex="-1" onkeydown={(e) => e.key === "Escape" && close()} use:grab>
    <header>
      <b>Compare with Commit</b>
      <button onclick={close}>Cancel</button>
    </header>
    <p class="hint">Opens the document as it was then, with the changes since as tracked changes, read only.</p>
    <label>
      Tag or commit
      <select bind:value={comparing.chosen} onchange={() => comparing && (comparing.typed = "")}>
        <option value="">{comparing.revisions.length ? "Choose…" : "none (not in a repository)"}</option>
        {#each comparing.revisions as v (v.kind + v.rev)}
          <option value={v.rev}>{v.label}: {v.detail}</option>
        {/each}
      </select>
    </label>
    <label>
      or any revision (a branch, v1.2, HEAD~3)
      <input bind:value={comparing.typed} onkeydown={(e) => e.key === "Enter" && compare()} />
    </label>
    {#if comparing.error}<p class="error">{comparing.error}</p>{/if}
    <div class="buttons">
      <button class="primary" disabled={!revision || comparing.busy} onclick={compare}>Compare</button>
    </div>
  </div>
{/if}
{#if failure}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="sheet" role="dialog" aria-label={failure.what} tabindex="-1" onkeydown={(e) => e.key === "Escape" && close()} use:grab>
    <header>
      <b>{failure.what}</b>
      <button onclick={close}>Close</button>
    </header>
    <p class="error">{failure.error}</p>
  </div>
{/if}

<style>
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
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  ul {
    margin: 8px 0;
    padding: 0;
    list-style: none;
    max-height: 220px;
    overflow: auto;
  }
  li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 1px 0;
  }
  li label {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  li.other .path {
    color: var(--muted);
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status {
    flex: none;
    width: 2.2em;
    color: var(--muted);
  }
  small {
    flex: none;
    color: var(--muted);
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
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
  .error {
    margin: 6px 0 0;
    color: var(--error);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .hint {
    margin: 6px 0 0;
    color: var(--muted);
  }
  label:not(li label) {
    display: block;
    margin-top: 8px;
  }
  select,
  input:not([type="checkbox"]) {
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
  .buttons {
    display: flex;
    justify-content: flex-end;
    margin-top: 8px;
  }
  button {
    font: inherit;
    color: var(--fg);
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 1px 8px;
  }
  button:hover:not(:disabled) {
    background: var(--chip);
  }
  button:disabled {
    opacity: 0.5;
  }
  button.primary {
    color: var(--on-accent);
    background: var(--accent);
    border-color: var(--accent);
  }
</style>
