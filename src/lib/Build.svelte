<script lang="ts">
  // The builds of a document, over its editor: Build (⌘B), Build and Show PDF (⇧⌘B) and Marked-up PDF… of the Build
  // menu come to `run`. The document is written first (`flush`), then the backend runs texmark (and for the marked-up
  // PDF latexdiff and latexmk; for a .tex document latexmk, and latexdiff) and tells only the status: `build-status`, shown in the foot of the window, where it stays: a split button
  // (Build, Building…, Built 14:02, Build failed) whose click builds and shows the PDF, its ▾ the builds to choose
  // from (Build, Build and Show PDF, Marked-up PDF…, Show PDF), and a button for the errors when there are some. A failure, or LaTeX errors in a PDF that was made, are listed in a sheet with the log's
  // path. The marked-up PDF asks first which earlier version: the tags and the last commits of the file in git, or
  // another file. The error of a .tex document at one of its own lines (`here`), or of a Markdown build at the line it
  // was matched to (`md_line`), is a button: it takes the cursor to the line (`ongoto`).

  import { invoke } from "@tauri-apps/api/core";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
  import { onMount } from "svelte";
  import { listen } from "$lib/events";

  type LatexError = { message: string; line: number | null; file: string | null; here: boolean; context: string | null; md_line: number | null };
  type Status = {
    what: "build" | "diff";
    state: "building" | "done" | "failed";
    seconds: number | null;
    pdf: string | null;
    log: string | null;
    errors: LatexError[];
    message: string | null;
  };
  type Revision = { rev: string; label: string; kind: "tag" | "commit"; detail: string };

  let {
    flush,
    can = true,
    onfocus,
    ongoto,
  }: {
    flush: () => unknown;
    /** a document that builds (Markdown, LaTeX): the mark shows before a build */
    can?: boolean;
    onfocus?: () => void;
    ongoto?: (line: number) => void;
  } = $props();

  let status = $state<Status | null>(null);
  /** the sheet of errors is shown */
  let sheet = $state(false);
  /** the choice of an earlier version: the versions in git, or the error that kept them from being listed */
  let choosing = $state<{ revisions: Revision[]; error?: string } | null>(null);
  /** what went wrong before a build could start (the document is not saved) */
  let refused = $state("");
  /** when the last build ended */
  let ended = $state<Date | null>(null);

  const errors = $derived(status?.errors.length ?? 0);
  const label = $derived(
    !status
      ? "Build"
      : status.state === "building"
        ? status.what === "diff"
          ? "Making the marked-up PDF…"
          : "Building…"
        : status.state === "done"
          ? `${status.what === "diff" ? "Marked-up PDF" : "Built"} ${ended?.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) ?? ""}`
          : `${status.what === "diff" ? "Marked-up PDF" : "Build"} failed`,
  );
  const title = $derived(
    status?.state === "done" ? `Made in ${status.seconds?.toFixed(1)} s. Build and Show PDF (⇧⌘B)` : "Build and Show PDF (⇧⌘B)",
  );

  /** The ▾ beside the button: the builds to choose from, and the PDF made last. */
  async function menu(e: MouseEvent) {
    const at = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const pdf = status?.state === "done" ? status.pdf : null;
    const items = await Promise.all([
      MenuItem.new({ text: "Build", accelerator: "CmdOrCtrl+B", action: () => void run("build") }),
      MenuItem.new({ text: "Build and Show PDF", accelerator: "Shift+CmdOrCtrl+B", action: () => void run("build-show") }),
      PredefinedMenuItem.new({ item: "Separator" }),
      MenuItem.new({ text: "Marked-up PDF…", action: () => void run("marked-up") }),
      ...(pdf ? [MenuItem.new({ text: "Show PDF", action: () => void request("open_document", { path: pdf }) })] : []),
    ]);
    await (await Menu.new({ items })).popup(new LogicalPosition(at.left, at.top));
  }

  /** The status, and the sheet for what failed. */
  function told(next: Status) {
    status = next;
    refused = "";
    sheet = next.state === "failed";
    if (next.state !== "building") ended = new Date();
  }

  async function request<T>(command: string, args: Record<string, unknown> = {}): Promise<T | undefined> {
    try {
      refused = "";
      return await invoke<T>(command, args);
    } catch (e) {
      refused = String(e);
    }
  }

  /** The menu's commands: "build", "build-show", "marked-up". */
  export async function run(what: string) {
    if (what !== "build" && what !== "build-show" && what !== "marked-up") return;
    await flush();
    if (what === "marked-up") {
      try {
        choosing = { revisions: await invoke<Revision[]>("revisions") };
      } catch (e) {
        refused = String(e);
      }
    } else {
      await request("build", { show: what === "build-show" });
    }
  }

  async function chooseVersion(earlier: { kind: "git"; rev: string; label: string } | { kind: "file"; path: string }) {
    choosing = null;
    await request("marked_up", { earlier });
    onfocus?.();
  }

  async function chooseFile() {
    const path = await request<string | null>("earlier_file");
    if (path) await chooseVersion({ kind: "file", path });
  }

  function close() {
    sheet = false;
    choosing = null;
    onfocus?.();
  }

  const nameOf = (file: string) => file.split("/").pop() ?? file;
  /** The keys to a sheet as it opens, so Esc reaches it. */
  const grab = (node: HTMLElement) => node.focus();

  onMount(() => {
    const unlisten = listen<Status>("build-status", (e) => told(e.payload));
    return () => void unlisten.then((f) => f());
  });
</script>

{#if refused}
  <button class="mark error" onclick={() => (refused = "")} title="{refused} (a click dismisses it)">{refused}</button>
{:else if status || can}
  <!-- a click builds and shows the PDF; the ▾ is the other builds; what went wrong is a button of its own -->
  <div class="split" class:error={status?.state === "failed"}>
    <button class="mark" onclick={() => status?.state !== "building" && run("build-show")} {title}>
      {#if status?.state === "building"}<span class="spin" aria-hidden="true"></span>{:else}
        <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round">
          <path d="M4 1.75h5.5L12.5 4.75v9.5h-8.5z" /><path d="M9.25 1.75v3.25h3.25" />
        </svg>
      {/if}{label}
    </button>
    <button class="more" onclick={menu} title="Build, Build and Show PDF, Marked-up PDF…" aria-label="Builds">
      <svg viewBox="0 0 10 10" width="8" height="8" aria-hidden="true"><path d="M2 3.5l3 3 3-3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
    </button>
  </div>
  {#if status && status.state !== "building" && (status.state === "failed" || errors)}
    <button class="errors" onclick={() => (sheet = true)} title="Show the errors">
      {status.state === "failed" ? "Show why" : `${errors} LaTeX error${errors === 1 ? "" : "s"}`}
    </button>
  {/if}
{/if}

{#if sheet && status && status.state !== "building"}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="sheet" role="dialog" aria-label="Build errors" tabindex="-1" onkeydown={(e) => e.key === "Escape" && close()} use:grab>
    <header>
      <b>{status.state === "failed" ? (status.what === "diff" ? "Marked-up PDF failed" : "Build failed") : "LaTeX errors"}</b>
      <button onclick={close}>Close</button>
    </header>
    {#if status.message}<pre>{status.message}</pre>{/if}
    {#if status.errors.length}
      <ul>
        {#each status.errors.slice(0, 10) as error, i (i)}
          <li>
            <span class="message">{error.message}</span>
            {#if error.md_line !== null}
              <button class="goto" onclick={() => (close(), ongoto?.(error.md_line!))} title="Go to the line">line {error.md_line}{error.context ? `: ${error.context}` : ""}</button>
            {:else if error.here && error.line !== null}
              <button class="goto" onclick={() => (close(), ongoto?.(error.line!))} title="Go to the line">line {error.line}{error.context ? `: ${error.context}` : ""}</button>
            {:else if error.line !== null}<span class="where">{error.file ? `${nameOf(error.file)} line` : "tex line"} {error.line}{error.context ? `: ${error.context}` : ""}</span>{/if}
          </li>
        {/each}
      </ul>
      {#if status.errors.length > 10}<p class="where">and {status.errors.length - 10} more, in the log</p>{/if}
    {/if}
    {#if status.log}
      {@const log = status.log}
      <p class="where" title={log}>Log: <code>{log}</code></p>
      <div class="buttons">
        <button onclick={() => request("reveal", { path: log })}>Reveal in Finder</button>
        <button onclick={() => request("open_document", { path: log })}>Open Log</button>
      </div>
    {/if}
  </div>
{/if}

{#if choosing}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="sheet" role="dialog" aria-label="Marked-up PDF" tabindex="-1" onkeydown={(e) => e.key === "Escape" && close()} use:grab>
    <header>
      <b>Marked-up PDF against…</b>
      <button onclick={close}>Cancel</button>
    </header>
    {#if choosing.revisions.length}
      <ul class="versions">
        {#each choosing.revisions as v (v.kind + v.rev)}
          <li>
            <button onclick={() => chooseVersion({ kind: "git", rev: v.rev, label: v.label })}>
              <span class="kind">{v.kind === "tag" ? "tag" : "commit"}</span>
              <b>{v.label}</b>
              <span class="where">{v.detail}</span>
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="where">The document is not in a git repository with earlier versions (tags, or commits that touched it).</p>
    {/if}
    <div class="buttons"><button onclick={chooseFile}>Another file…</button></div>
  </div>
{/if}

<style>
  .goto {
    display: block;
    font: inherit;
    font-size: 12px;
    color: var(--accent);
    background: none;
    border: none;
    padding: 0;
    text-align: left;
    cursor: pointer;
  }
  .split {
    display: inline-flex;
    flex: none;
    align-items: stretch;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
  }
  .mark,
  .more,
  .errors {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font: inherit;
    font-size: 11.5px;
    color: var(--muted);
    background: none;
    border: none;
    padding: 1px 7px;
    white-space: nowrap;
  }
  .more {
    padding: 1px 5px;
    border-left: 1px solid var(--line);
  }
  .mark:hover,
  .more:hover,
  .errors:hover {
    color: var(--fg);
    background: var(--chip);
  }
  .split.error .mark,
  .errors {
    color: var(--error);
  }
  .errors {
    flex: none;
    border-radius: 5px;
  }
  .mark.error {
    color: var(--error);
    border-radius: 5px;
  }
  .spin {
    display: inline-block;
    width: 9px;
    height: 9px;
    border: 1.5px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: turn 0.8s linear infinite;
  }
  @keyframes turn {
    to {
      transform: rotate(360deg);
    }
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
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  ul {
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
  }
  li {
    padding: 3px 0;
  }
  li .message {
    display: block;
  }
  .where {
    display: block;
    color: var(--muted);
    font-size: 11.5px;
    overflow-wrap: anywhere;
  }
  p.where {
    margin: 8px 0 0;
  }
  pre {
    margin: 8px 0 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-size: 11.5px;
    color: var(--error);
  }
  .buttons {
    margin-top: 10px;
    display: flex;
    gap: 6px;
  }
  .sheet button {
    font: inherit;
    color: var(--fg);
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 1px 6px;
  }
  .sheet button:hover {
    background: var(--chip);
  }
  .versions button {
    display: block;
    width: 100%;
    text-align: left;
    border-color: transparent;
    padding: 3px 6px;
  }
  .kind {
    display: inline-block;
    min-width: 46px;
    color: var(--muted);
    font-size: 11px;
  }
  code {
    font-size: 11px;
  }
</style>
