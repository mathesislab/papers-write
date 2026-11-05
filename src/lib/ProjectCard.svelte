<script lang="ts">
  // The card of a project over the top of the window, like a macOS sheet (papers-review's info card and card of a
  // new project, in one). A folder with write.toml: Project Info, its name, short name (the tile of the project
  // bar), colour, main document, build folder and library, and the folder; written on Done, dropped on Cancel or Esc.
  // A folder without: New project, the name, short name, colour and main document (a Markdown or LaTeX file of the
  // folder, or a new paper from the paper-draft skeleton); Create writes write.toml (and the paper), Cancel leaves
  // nothing. There is no Esc for a new one: it would drop it by a stray key.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { COLOURS, SHORT, initials, type Colour } from "papers-write/project";
  import "./card.css";
  import type { Card, Manifest } from "./projects";

  let { root, onclose }: { /** the project's folder */ root: string; onclose: () => void } = $props();

  /** the value of the main document's choice that is a new paper */
  const NEW = "\u0000new";

  /** the project as it is on disk */
  let card = $state<Card | null>(null);
  /** the fields, as edited */
  let name = $state("");
  let short = $state("");
  let colour = $state<Colour>("none");
  let main = $state("");
  /** the name of the new paper, when the main document is one */
  let paper = $state("paper.md");
  let build = $state("");
  let library = $state("");
  let error = $state("");
  /** asked of the app, not yet answered */
  let busy = $state(false);

  const folder = $derived(root.slice(root.lastIndexOf("/") + 1));
  const fresh = $derived(card !== null && !card.defined);
  /** the documents to choose from: the folder's, and the main one should it not be among them */
  const documents = $derived(card ? [...new Set([...card.manifest.main.slice(0, 1), ...card.documents])] : []);

  /** Nothing but spaces: not set. */
  const blank = (text: string) => text.trim() || null;

  /** A field shown filled with what stands in for it when unset (the folder's name, the initials): unset still,
   *  unless changed. */
  const unless = (text: string, standIn: string, was: string | null) => (was === null && text.trim() === standIn ? null : blank(text));

  async function load() {
    try {
      const next = await invoke<Card>("project_card", { root });
      card = next;
      const m = next.manifest;
      name = m.name ?? (next.defined ? folder : "");
      short = m.short ?? (next.defined ? initials(name) : "");
      colour = m.colour ?? "none";
      build = m.build ?? "";
      library = m.library ?? "";
      main = m.main[0] ?? (next.documents.includes("paper.md") ? "paper.md" : next.defined ? (next.documents[0] ?? "") : NEW);
    } catch (e) {
      error = String(e);
    }
  }

  /** What write.toml is to say. */
  function edited(c: Card): Manifest {
    const was = c.manifest;
    const shortName = Array.from(short.trim()).slice(0, SHORT).join("");
    return {
      name: unless(name, folder, was.name),
      short: unless(shortName, initials(name.trim() || folder), was.short),
      // no colour set and none chosen: unset still
      colour: was.colour === null && colour === "none" ? null : colour,
      main: [main === NEW ? paper.trim() : main, ...was.main.slice(1)].filter(Boolean),
      build: blank(build),
      library: blank(library),
    };
  }

  async function submit() {
    if (!card) return;
    const manifest = edited(card);
    busy = true;
    try {
      if (fresh) await invoke("create_project", { root, manifest, paper: main === NEW });
      else if (JSON.stringify(manifest) !== JSON.stringify(card.manifest)) await invoke("set_project_info", { root, manifest });
      onclose();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function cancel() {
    if (fresh) await invoke("cancel_project").catch(() => {});
    onclose();
  }

  async function chooseLibrary() {
    const chosen = await invoke<string | null>("choose_library", { root }).catch((e) => ((error = String(e)), null));
    if (chosen) library = chosen;
  }

  function focus(input: HTMLInputElement) {
    input.focus();
  }

  onMount(() => {
    load();
  });
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && card?.defined && cancel()} />

<div class="scrim" role="presentation"></div>
<form class="card" aria-label={fresh ? "New project" : "Project Info"} onsubmit={(e) => (e.preventDefault(), submit())}>
  {#if fresh}<h2>New project</h2>{/if}
  {#if card}
    <div class="grid">
      <label for="project-name">Name</label>
      <input id="project-name" bind:value={name} use:focus placeholder={folder} autocomplete="off" spellcheck="false" />

      <label for="project-short">Short name</label>
      <input id="project-short" class="short" bind:value={short} maxlength={SHORT} placeholder={initials(name.trim() || folder)} autocomplete="off" spellcheck="false" />

      <span class="label">Colour</span>
      <div class="row swatches">
        {#each COLOURS as c (c.colour)}
          <button
            type="button"
            class="swatch"
            class:none={c.css === null}
            class:on={c.colour === colour}
            style:background={c.css}
            title={c.name}
            aria-label={c.name}
            aria-pressed={c.colour === colour}
            onclick={() => (colour = c.colour)}
          ></button>
        {/each}
      </div>

      <label for="project-main">Main document</label>
      <div class="column">
        <select id="project-main" bind:value={main}>
          {#each documents as path (path)}<option value={path}>{path}</option>{/each}
          {#if fresh}<option value={NEW}>New paper…</option>{/if}
          {#if !fresh && !documents.length}<option value="">None</option>{/if}
        </select>
        {#if main === NEW}
          <input bind:value={paper} placeholder="paper.md" aria-label="Name of the new paper" autocomplete="off" spellcheck="false" />
          <span class="hint">Front matter (title, authors, bibliography, journal to choose) and the headings of the sections</span>
        {/if}
      </div>

      {#if !fresh}
        <label for="project-build">Build folder</label>
        <input id="project-build" bind:value={build} placeholder="build" autocomplete="off" spellcheck="false" />

        <label for="project-library">Library</label>
        <div class="row">
          <input id="project-library" class="grow" bind:value={library} placeholder="The papers library" autocomplete="off" spellcheck="false" />
          <button type="button" onclick={chooseLibrary}>Choose…</button>
        </div>

        <span class="label">Folder</span>
        <div class="row">
          <span class="path" title={root}>{root.replace(/^\/Users\/[^/]+/, "~")}</span>
          <button type="button" onclick={() => invoke("reveal", { path: root }).catch((e) => (error = String(e)))}>Show in Finder</button>
        </div>
      {/if}
    </div>
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
  <footer>
    <button type="button" onclick={cancel} disabled={busy}>Cancel</button>
    <button type="submit" class="default" disabled={!card || busy || (fresh && !(main === NEW ? paper.trim() : main))}>{fresh ? "Create" : "Done"}</button>
  </footer>
</form>

<style>
  h2 {
    margin: 0 0 14px;
    font-size: 13px;
    font-weight: 600;
    text-align: center;
  }
  .column {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .hint {
    color: var(--muted);
    font-size: 11px;
  }
  .grow {
    flex: 1;
  }
  .swatches {
    gap: 7px;
  }
  .swatch {
    flex: none;
    width: 20px;
    height: 20px;
    padding: 0;
    border-radius: 50%;
  }
  .swatch.none {
    background: linear-gradient(to top right, transparent calc(50% - 1px), var(--muted) 50%, transparent calc(50% + 1px));
  }
  .swatch.on {
    box-shadow:
      0 0 0 2px var(--bg),
      0 0 0 4px var(--accent);
  }
  .path {
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
    font-size: 11px;
  }
</style>
