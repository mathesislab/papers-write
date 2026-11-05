<script lang="ts">
  // The state of the document's repository as a small, quiet line at the foot of the window: "main · 3 changed · ↓1".
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
  <button class="git" title={state.upstream ? `${state.branch} follows ${state.upstream}. Commit… (⌥⌘K)` : `${state.branch}, no upstream. Commit… (⌥⌘K)`} onclick={onopen}>{summary(state)}</button>
{/if}

<style>
  .git {
    position: absolute;
    z-index: 4;
    left: 10px;
    bottom: 4px;
    font: inherit;
    font-size: 11px;
    color: var(--muted);
    background: none;
    border: none;
    padding: 0 4px;
    opacity: 0.8;
    cursor: pointer;
  }
  .git:hover {
    opacity: 1;
    color: var(--fg);
  }
</style>
