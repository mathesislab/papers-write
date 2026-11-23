<script lang="ts">
  // The card of a project over the top of the window, like a macOS sheet (papers-review's info card and card of a
  // new project, in one). A folder with write.toml: Project Info, its name, short name (the tile of the project
  // bar), colour, main document, build folder, what builds the Markdown (texmark or Quarto) and library, and the
  // folder; written on Done, dropped on Cancel or Esc.
  // A folder without: New project, the name, short name, colour and main document (a Markdown or LaTeX file of the
  // folder, or a new paper from the paper-draft skeleton); Create writes write.toml (and the paper), Cancel leaves
  // nothing. There is no Esc for a new one: it would drop it by a stray key.

  import { invoke } from "@tauri-apps/api/core";
  import { REVEAL, keys } from "papers-write/editor";
  import { onMount } from "svelte";
  import { COLOURS, SHORT, initials, type Colour } from "papers-write/project";
  import "./card.css";
  import type { Card, Manifest } from "./projects";
  import { overPage } from "$lib/overlay.svelte";

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
  let engine = $state<"texmark" | "quarto">("texmark");
  /** where the suggestions and comments are kept */
  let reviewStore = $state<"notes" | "in-file">("notes");
  /** a paper or a folder of notes; the notes app that keeps the folder, if one does (it makes notes the choice) */
  let kind = $state<"paper" | "notes">("paper");
  let keptBy = $state<string | null>(null);
  const notes = $derived(kind === "notes");
  let library = $state("");
  /** the project's notes file (View ▸ Show Notes): notes.md when blank */
  let notesFile = $state("");
  /** what Go to File and Find in Project leave out (exclude.rs): one pattern a line */
  let gitignore = $state(true);
  let hide = $state("");
  let show = $state("");
  let error = $state("");
  /** asked of the app, not yet answered */
  let busy = $state(false);

  const folder = $derived(root.slice(root.lastIndexOf("/") + 1));
  const fresh = $derived(card !== null && !card.defined);
  /** the documents to choose from: the folder's, and the main one should it not be among them */
  const documents = $derived(card ? [...new Set([...card.manifest.main.slice(0, 1), ...card.documents])] : []);

  /** The patterns of a field, one a line. */
  const lines = (text: string) => text.split("\n").map((l) => l.trim()).filter(Boolean);

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
      engine = m.engine ?? "texmark";
      reviewStore = m.review ?? "notes";
      library = m.library ?? "";
      notesFile = m.notes ?? "";
      keptBy = await invoke<string | null>("kept_by", { root }).catch(() => null);
      kind = m.kind ?? (!next.defined && keptBy ? "notes" : "paper");
      gitignore = m.gitignore ?? true;
      hide = (m.hide ?? []).join("\n");
      show = (m.show ?? []).join("\n");
      main = m.main[0] ?? likelyMain(next.documents) ?? (next.defined ? "" : NEW);
    } catch (e) {
      error = String(e);
    }
  }

  /** The document a folder's main one most likely is: the one named after the folder (thesis.md in thesis/), else
   *  paper.md, main.md or main.tex, else the first; none when the folder has no Markdown or LaTeX file. */
  function likelyMain(documents: string[]): string | undefined {
    const named = [`${folder}.md`, `${folder}.tex`, "paper.md", "main.md", "main.tex"];
    return named.find((n) => documents.includes(n)) ?? documents[0];
  }

  /** The name typed for a new paper, unless it is a document of the folder already: then that document. */
  const existing = $derived(main === NEW && card?.documents.includes(paper.trim()) ? paper.trim() : null);
  /** The main document chosen: a document of the folder, or the name of the new paper. */
  const chosen = $derived(existing ?? (main === NEW ? paper.trim() : main));

  /** What write.toml is to say. */
  function edited(c: Card): Manifest {
    const was = c.manifest;
    const shortName = Array.from(short.trim()).slice(0, SHORT).join("");
    return {
      name: unless(name, folder, was.name),
      short: unless(shortName, initials(name.trim() || folder), was.short),
      // no colour set and none chosen: unset still
      colour: was.colour === null && colour === "none" ? null : colour,
      main: notes ? was.main : [chosen, ...was.main.slice(1)].filter(Boolean),
      build: blank(build),
      // texmark and none set: unset still
      engine: was.engine === null && engine === "texmark" ? null : engine,
      // notes and none set: unset still
      review: was.review === null && reviewStore === "notes" ? null : reviewStore,
      library: blank(library),
      notes: blank(notesFile),
      profile: was.profile,
      // a paper and none set: unset still
      kind: was.kind === null && kind === "paper" ? null : kind,
      gitignore: gitignore ? null : false,
      hide: lines(hide),
      show: lines(show),
    };
  }

  async function submit() {
    if (!card) return;
    const manifest = edited(card);
    busy = true;
    try {
      if (fresh) await invoke("create_project", { root, manifest, paper: !notes && main === NEW && !existing });
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
    const chosen = await invoke<string | null>("choose_in_project", { root, what: "library" }).catch((e) => ((error = String(e)), null));
    if (chosen) library = chosen;
  }

  async function chooseNotes() {
    const chosen = await invoke<string | null>("choose_in_project", { root, what: "notes" }).catch((e) => ((error = String(e)), null));
    if (chosen) notesFile = chosen;
  }

  function focus(input: HTMLInputElement) {
    input.focus();
  }

  onMount(() => {
    load();
  });
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && card?.defined && cancel()} />

<div class="scrim" role="presentation" {@attach overPage}></div>
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

      <label for="project-kind">Kind</label>
      <div class="row">
        <select id="project-kind" bind:value={kind}>
          <option value="paper">A paper (documents built into a PDF)</option>
          <option value="notes">Notes</option>
        </select>
        <span class="hint">
          {#if notes}opened on the last note changed, searched by title, name and text {keys("(⇧⌘F)")}{#if keptBy} — this folder is kept by {keptBy}{/if}{:else}its main document opens first{/if}
        </span>
      </div>

      {#if !notes}
      <label for="project-main">Main document</label>
      <div class="column">
        <select id="project-main" bind:value={main}>
          {#each documents as path (path)}<option value={path}>{path}</option>{/each}
          {#if fresh}<option value={NEW}>New paper…</option>{/if}
          {#if !fresh && !documents.length}<option value="">None</option>{/if}
        </select>
        {#if main === NEW}
          <input bind:value={paper} placeholder="paper.md" aria-label="Name of the new paper" autocomplete="off" spellcheck="false" />
          {#if existing}
            <span class="hint">{existing} is in the folder already: it becomes the main document as it is</span>
          {:else}
            <span class="hint">Front matter (title, authors, bibliography, journal to choose) and the headings of the sections</span>
          {/if}
        {/if}
      </div>
      {/if}

      {#if !fresh}
        <label for="project-build">Build folder</label>
        <input id="project-build" bind:value={build} placeholder="build" autocomplete="off" spellcheck="false" />

        <label for="project-review">Suggestions</label>
        <div class="row">
          <select id="project-review" bind:value={reviewStore}>
            <option value="notes">In review notes</option>
            <option value="in-file">In the text (CriticMarkup)</option>
          </select>
          <span class="hint">
            {reviewStore === "notes" ? "beside the documents, with authors, dates and replies" : "as {++ ++} {-- --} {~~ ~> ~~} marks, which texmark shows in the PDF"}
            {#if card && (card.manifest.review ?? "notes") !== reviewStore}
              — suggestions made so far stay where they are: File ▸ Exchange ▸ {reviewStore === "in-file" ? "Export as CriticMarkup… writes the review's notes as marks" : "Import Review… reads marks into the review's notes"}
            {/if}
          </span>
        </div>

        <label for="project-engine">Build with</label>
        <div class="row">
          <select id="project-engine" bind:value={engine}>
            <option value="texmark">texmark</option>
            <option value="quarto">Quarto</option>
          </select>
          <span class="hint">the Markdown documents; a .qmd is always built by Quarto</span>
        </div>

        <label for="project-library">Library</label>
        <div class="row">
          <input id="project-library" class="grow" bind:value={library} placeholder="The papers library" autocomplete="off" spellcheck="false" />
          <button type="button" onclick={chooseLibrary}>Choose…</button>
        </div>

        <label for="project-notes">Notes</label>
        <div class="row">
          <input id="project-notes" class="grow" bind:value={notesFile} placeholder="notes.md" autocomplete="off" spellcheck="false" />
          <button type="button" onclick={chooseNotes}>Choose…</button>
        </div>

        <span class="label">Go to File, Find in Project</span>
        <div class="search">
          <label class="check"><input type="checkbox" bind:checked={gitignore} /> Leave out what .gitignore ignores</label>
          <span class="hint">Hidden files and build*, _build, dist, node_modules, target, __pycache__, venv are left out too. One pattern a line, as in .gitignore: a name anywhere (*.log), a path from the folder (data/raw/).</span>
          <div class="patterns">
            <label>Also leave out <textarea bind:value={hide} rows="2" placeholder="data/raw/" spellcheck="false"></textarea></label>
            <label>Bring back <textarea bind:value={show} rows="2" placeholder="build-full/thesis.pdf" spellcheck="false"></textarea></label>
          </div>
        </div>

        <span class="label">Folder</span>
        <div class="row">
          <span class="path" title={root}>{root.replace(/^\/Users\/[^/]+/, "~")}</span>
          <button type="button" onclick={() => invoke("reveal", { path: root }).catch((e) => (error = String(e)))}>{REVEAL}</button>
        </div>
      {/if}
    </div>
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
  <footer>
    <button type="button" onclick={cancel} disabled={busy}>Cancel</button>
    <button type="submit" class="default" disabled={!card || busy || (fresh && !notes && !chosen)}>{fresh ? "Create" : "Done"}</button>
  </footer>
</form>

<style>
  .search {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .patterns {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .patterns label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
    color: var(--muted);
  }
  .patterns textarea {
    font: 12px ui-monospace, monospace;
    padding: 4px 6px;
    color: var(--fg);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 5px;
    resize: vertical;
  }
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
