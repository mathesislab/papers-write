<script lang="ts">
  // Switch Project (⌃⌘P; Ctrl+Alt+P elsewhere), as Sublime's Quick Switch Project: a field over the page and the projects of the bar, then
  // those opened recently. What is typed keeps those whose name (or folder) has its letters in order; Enter switches
  // the window to the project (as its tile does), the last line opens another (Open Project…). Up, Down, Esc.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { cssOf, type Colour } from "papers-write/project";

  let { onclose }: { onclose: () => void } = $props();

  type Offered = { root: string; name: string; colour: Colour; current: boolean };
  let projects = $state.raw<Offered[]>([]);
  let query = $state("");
  let at = $state(0);
  let field = $state<HTMLInputElement>();

  /** Whether the letters of `q` are in `text` in order (case aside). */
  const follows = (q: string, text: string) => {
    let i = 0;
    for (const c of text.toLowerCase()) if (c === q[i]) i++;
    return i === q.length;
  };
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase().replace(/\s+/g, "");
    return q ? projects.filter((p) => follows(q, p.name) || follows(q, p.root)) : projects;
  });
  /** the lines: the projects, then Open Another Project… */
  const count = $derived(shown.length + 1);
  $effect(() => {
    void query;
    at = 0;
  });

  function go(i: number) {
    const project = shown[i];
    onclose();
    if (project) void invoke("switch_project", { root: project.root });
    else void invoke("choose_project");
  }

  function keys(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
    else if (e.key === "Enter") go(at);
    else if (e.key === "ArrowDown") at = Math.min(count - 1, at + 1);
    else if (e.key === "ArrowUp") at = Math.max(0, at - 1);
    else return;
    e.preventDefault();
  }

  onMount(() => {
    field?.focus();
    invoke<Offered[]>("project_list").then((found) => (projects = found));
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="veil" onclick={onclose}>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="sheet" role="dialog" aria-label="Switch Project" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <input bind:this={field} bind:value={query} onkeydown={keys} placeholder="Switch to a project" spellcheck="false" autocomplete="off" />
    <ul>
      {#each shown as p, i (p.root)}
        <li class:on={i === at}>
          <button onclick={() => go(i)} onmouseenter={() => (at = i)}>
            <span class="dot" style:background={cssOf(p.colour) ?? "var(--line)"}></span>
            <span class="name">{p.name}</span>
            {#if p.current}<span class="here">this window</span>{/if}
            <span class="root">{p.root}</span>
          </button>
        </li>
      {/each}
      <li class:on={at === shown.length}>
        <button class="other" onclick={() => go(shown.length)} onmouseenter={() => (at = shown.length)}>Open Another Project…</button>
      </li>
    </ul>
  </div>
</div>

<style>
  .veil {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 8vh;
    background: color-mix(in srgb, #000 22%, transparent);
  }
  .sheet {
    width: min(560px, 92vw);
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.3);
    overflow: hidden;
  }
  input {
    margin: 10px;
    font: inherit;
    font-size: 15px;
    padding: 6px 10px;
    color: var(--fg);
    background: var(--sidebar);
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0 4px 4px;
    overflow: auto;
    border-top: 1px solid var(--line);
  }
  li button {
    display: flex;
    align-items: baseline;
    gap: 8px;
    width: 100%;
    padding: 6px 10px;
    text-align: left;
    font: inherit;
    color: var(--fg);
    background: none;
    border: none;
    border-radius: 6px;
    white-space: nowrap;
    overflow: hidden;
  }
  li.on button {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .dot {
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    align-self: center;
  }
  .name {
    flex: none;
    font-weight: 600;
  }
  .here {
    flex: none;
    font-size: 11px;
    color: var(--accent);
  }
  .root {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 12px;
    color: var(--muted);
  }
  .other {
    color: var(--muted) !important;
  }
</style>
