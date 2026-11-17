<script lang="ts">
  // The toolbar's Read Aloud: there only when a command is set (Settings ▸ Read Aloud). A click reads `text()` (the
  // selection, else the paragraph of the cursor); while a reading goes on, the button stops it.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { listen } from "$lib/events";
  import { watchSettings } from "$lib/settings";

  let { text }: { text: () => string } = $props();

  let set = $state(false);
  let reading = $state(false);

  function click() {
    if (reading) void invoke("stop_reading");
    else void invoke("read_aloud", { text: text() }).catch(() => {});
  }

  onMount(() => {
    const stop = watchSettings((s) => (set = !!s.read_aloud.trim()));
    invoke<boolean>("reading").then((r) => (reading = r));
    const unlisten = listen<boolean>("reading-changed", (e) => (reading = e.payload));
    return () => {
      stop();
      void unlisten.then((f) => f());
    };
  });
</script>

{#if set}
  <button class:on={reading} onclick={click} title={reading ? "Stop Reading" : "Read Aloud: the selection, else the paragraph"} aria-label={reading ? "Stop Reading" : "Read Aloud"}>
    <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round">
      {#if reading}
        <rect x="4" y="4" width="8" height="8" rx="1" fill="currentColor" stroke="none" />
      {:else}
        <path d="M2.5 6v4h2.75L9 13V3L5.25 6z" />
        <path d="M11.25 5.5a3.5 3.5 0 0 1 0 5M13 3.75a6 6 0 0 1 0 8.5" />
      {/if}
    </svg>
  </button>
{/if}

<style>
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 26px;
    height: 24px;
    padding: 0 5px;
    box-sizing: border-box;
    color: var(--muted);
    background: none;
    border: 1px solid transparent;
    border-radius: 5px;
  }
  button:hover {
    color: var(--fg);
    background: var(--chip);
  }
  button.on {
    color: var(--fg);
    background: var(--chip);
    border-color: var(--line);
  }
</style>
