<script lang="ts">
  // The builds of a document, over its editor: Build (⌘B), Build and Show PDF (⇧⌘B) and Marked-up PDF… of the Build
  // menu come to `run`. The document is written first (`flush`), then the backend runs texmark (and for the marked-up
  // PDF latexdiff and latexmk) and tells only the status: `build-status`, shown as a mark in the corner: building,
  // done (the time), failed. A failure, or LaTeX errors in a PDF that was made, are listed in a sheet with the log's
  // path. The marked-up PDF asks first which earlier version: the tags and the last commits of the file in git, or
  // another file.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { listen } from "$lib/events";

  type LatexError = { message: string; line: number | null; context: string | null };
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

  let { flush, onfocus }: { flush: () => unknown; onfocus?: () => void } = $props();

  let status = $state<Status | null>(null);
  /** the sheet of errors is shown */
  let sheet = $state(false);
  /** the choice of an earlier version: the versions in git, or the error that kept them from being listed */
  let choosing = $state<{ revisions: Revision[]; error?: string } | null>(null);
  /** what went wrong before a build could start (the document is not saved) */
  let refused = $state("");
  let timer: ReturnType<typeof setTimeout> | undefined;

  const label = $derived(
    !status
      ? ""
      : status.state === "building"
        ? status.what === "diff"
          ? "Making the marked-up PDF…"
          : "Building…"
        : status.state === "done"
          ? `${status.what === "diff" ? "Marked-up PDF" : "Built"} in ${status.seconds?.toFixed(1)} s${status.errors.length ? `, ${status.errors.length} LaTeX error${status.errors.length === 1 ? "" : "s"}` : ""}`
          : `${status.what === "diff" ? "Marked-up PDF" : "Build"} failed`,
  );

  /** The status, and the sheet for what failed; a build that went well is told for a few seconds. */
  function told(next: Status) {
    clearTimeout(timer);
    status = next;
    refused = "";
    sheet = next.state === "failed";
    if (next.state === "done" && !next.errors.length) timer = setTimeout(() => (status = null), 6000);
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
    return () => {
      clearTimeout(timer);
      unlisten.then((f) => f());
    };
  });
</script>

{#if refused}
  <button class="mark error" onclick={() => (refused = "")} title="Dismiss">{refused}</button>
{:else if status}
  <button
    class="mark"
    class:error={status.state === "failed"}
    onclick={() => (status!.state === "building" ? null : (sheet = status!.errors.length > 0 || status!.state === "failed"))}
    title={status.state === "building" ? "" : status.errors.length || status.state === "failed" ? "Show the errors" : ""}
  >
    {#if status.state === "building"}<span class="spin" aria-hidden="true"></span>{/if}{label}
  </button>
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
            {#if error.line !== null}<span class="where">tex line {error.line}{error.context ? `: ${error.context}` : ""}</span>{/if}
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
  .mark {
    position: absolute;
    z-index: 5;
    right: 14px;
    bottom: 10px;
    max-width: calc(100% - 28px);
    font: inherit;
    font-size: 12px;
    color: var(--fg);
    background: var(--chip);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 2px 10px;
    text-align: left;
  }
  .mark.error {
    color: var(--error);
  }
  .spin {
    display: inline-block;
    width: 9px;
    height: 9px;
    margin-right: 6px;
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
