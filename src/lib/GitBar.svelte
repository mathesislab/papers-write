<script lang="ts">
  // The state of the document's repository as a small, quiet item at the right of the window's foot: "main · 3 changed · ↓1".
  // It is asked again a moment after what may change it (`refresh`: a save, the app's own git actions) and as the
  // window gets the keys, when the upstream is fetched too (the backend does so at most every 5 minutes). A click
  // opens the Commit sheet. Nothing shows for a document that is not in a repository.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { summary, type GitState } from "$lib/git";

  let { path, state = $bindable(null), onopen }: { path: string | null | undefined; state?: GitState | null; onopen: () => void } = $props();

  let timer: ReturnType<typeof setTimeout> | undefined;
  let seq = 0;

  /** The state asked for `delay` ms after the last call (a run of saves asks once). */
  export function refresh(delay = 0) {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const mine = ++seq;
      try {
        const got = await invoke<GitState | null>("git_state");
        if (mine === seq) state = got;
      } catch {
        if (mine === seq) state = null;
      }
    }, delay);
  }

  /** The upstream fetched (when it is time), the state asked for again if it was. */
  async function focused() {
    refresh(0);
    try {
      if (await invoke<boolean>("git_fetch")) refresh(0);
    } catch {}
  }

  $effect(() => {
    path;
    refresh(0);
  });

  onMount(() => {
    window.addEventListener("focus", focused);
    return () => {
      window.removeEventListener("focus", focused);
      clearTimeout(timer);
    };
  });
</script>

{#if state}
  <button class="git" title={state.upstream ? `${state.branch} follows ${state.upstream}. Commit… (⌥⌘K)` : `${state.branch}, no upstream. Commit… (⌥⌘K)`} onclick={onopen}>
    <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.3">
      <circle cx="4.5" cy="3.5" r="1.75" /><circle cx="4.5" cy="12.5" r="1.75" /><circle cx="11.5" cy="5.5" r="1.75" />
      <path d="M4.5 5.25v5.5M11.5 7.25c0 2.5-3 2.5-6.2 4" />
    </svg>{summary(state)}</button>
{/if}

<style>
  .git {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
    min-width: 0;
    font: inherit;
    font-size: 11.5px;
    color: var(--muted);
    background: none;
    border: none;
    border-radius: 5px;
    padding: 1px 6px;
    white-space: nowrap;
  }
  .git:hover {
    color: var(--fg);
    background: var(--chip);
  }
</style>
