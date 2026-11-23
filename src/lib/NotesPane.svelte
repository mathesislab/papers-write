<script lang="ts">
  // The project's notes beside a text (View ▸ Show Notes, ⌥⌘N; notes.rs): its notes file in the editor of the texts,
  // written as they are; read again when changed elsewhere (`notes-changed`: their tab, another app). Its header
  // says which file, and renames it or takes another (write.toml's `notes`), or opens it in a tab. Add Note Here
  // starts a line in it (`takeLines`); a ⌘-click (Ctrl elsewhere) on a link to a place goes there (`open_place`).

  import { invoke } from "@tauri-apps/api/core";
  import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  import { onMount, tick } from "svelte";
  import { showMenu } from "papers-write/project";
  import { FileEditor, type EditorPrefs, type FileRead, type Status } from "papers-write/editor";
  import { listen } from "$lib/events";
  import { appendLines, followLink, linkOf } from "$lib/notesLinks";

  let {
    read,
    drafts,
    autosave,
    prefs,
    onsay,
    onhide,
  }: {
    /** a file read as the page reads its own (its draft, its history) */
    read: (path: string) => Promise<FileRead>;
    drafts: { write: (path: string, text: string, base: string) => Promise<void>; drop: (path: string) => Promise<void> };
    autosave: boolean;
    prefs: EditorPrefs;
    onsay: (what: string) => void;
    onhide: () => void;
  } = $props();

  type Notes = { path: string; name: string };
  let notes = $state<Notes | null>(null);
  let error = $state("");
  let editor = $state<FileEditor>();
  /** whether what is in the editor is saved: a star by the file's name while it is not */
  let status = $state<Status>("saved");
  /** the name typed in place of the file's (Rename…) */
  let renaming = $state<string | null>(null);

  async function ask(step: Promise<Notes | null>) {
    try {
      const got = await step;
      if (!got) return;
      [notes, error] = [got, ""];
      await invoke("watch_notes", { path: got.path });
    } catch (e) {
      error = String(e);
    }
  }

  /** The lines Add Note Here started in these notes put at their end, once the notes are read. */
  export async function takeLines() {
    for (let tries = 0; tries < 100 && !editor?.view(); tries++) await new Promise((r) => setTimeout(r, 20));
    const view = editor?.view();
    if (!view || !notes) return;
    appendLines(view, await invoke<string[]>("take_notes_line", { path: notes.path }).catch(() => []));
  }

  export const focus = () => editor?.focus();
  export const hasFocus = () => editor?.hasFocus() ?? false;
  /** what is typed and not yet written, written (before a build or a commit, as the text's) */
  export const flush = () => editor?.flush();

  /** ⌘-click: the place of the link under the cursor shown, when there is one. */
  function follow() {
    const view = editor?.view();
    const link = view && linkOf(view);
    if (link) open(link.target, link.title);
  }
  /** A link of the notes followed (⌘-click, its tooltip's Follow): its place, at the words of its title near there. */
  function open(target: string, quote: string | null) {
    if (notes) followLink(notes.path, { target, title: quote }, onsay);
  }

  /** The file's name clicked: Rename…, Choose File…, Open in a Tab, Hide Notes. */
  async function menu(e: MouseEvent) {
    const anchor = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items = await Promise.all([
      MenuItem.new({ text: "Rename…", action: () => (renaming = notes?.name.split("/").pop() ?? "") }),
      MenuItem.new({ text: "Choose File…", action: () => void ask(invoke<Notes | null>("choose_notes")) }),
      MenuItem.new({ text: "Open in a Tab", action: () => notes && void invoke("open_document", { path: notes.path }).catch((e) => onsay(String(e))) }),
      PredefinedMenuItem.new({ item: "Separator" }),
      MenuItem.new({ text: "Hide Notes", action: onhide }),
    ]);
    await showMenu(await Menu.new({ items }), new LogicalPosition(anchor.left, anchor.bottom + 2));
  }

  async function rename() {
    const name = renaming?.trim();
    renaming = null;
    if (!name || name === notes?.name.split("/").pop()) return;
    await editor?.flush();
    await ask(invoke<Notes>("rename_notes", { name }));
  }

  onMount(() => {
    void ask(invoke<Notes>("project_notes")).then(() => tick()).then(takeLines);
    const unlisten = [
      listen("notes-changed", () => editor?.changedOnDisk()),
      // what was typed in the notes elsewhere and not saved (their tab, the pane beside another text)
      listen<string>("draft-changed", (e) => void (e.payload === notes?.path && editor?.draftChanged())),
    ];
    return () => {
      void invoke("watch_notes", { path: null }).catch(() => {});
      unlisten.forEach((u) => void u.then((f) => f()));
    };
  });
</script>

<aside>
  <header>
    <span class="what">Notes</span>
    {#if renaming !== null}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="rename"
        bind:value={renaming}
        autofocus
        spellcheck="false"
        onkeydown={(e) => (e.key === "Enter" ? void rename() : e.key === "Escape" ? ((renaming = null), focus()) : undefined)}
        onblur={() => void rename()}
      />
    {:else if notes}
      <button class="name" title={status === "editing" ? `${notes.path}: not saved` : notes.path} onclick={menu}>{notes.name}{status === "editing" ? " *" : ""} ▾</button>
    {/if}
  </header>
  <div class="body">
    {#if error}
      <p class="error">{error}</p>
    {:else if notes}
      {#key notes.path}
        <FileEditor
          bind:this={editor}
          load={() => read(notes!.path)}
          save={(p, text) => invoke("write_text", { path: p, text })}
          {autosave}
          {drafts}
          {prefs}
          onsync={follow}
          hover={{ link: open }}
          bind:status
        />
      {/key}
    {/if}
  </div>
</aside>

<style>
  aside {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
  }
  header {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 34px 5px 10px;
    border-bottom: 1px solid var(--line);
    background: var(--sidebar);
  }
  .what {
    color: var(--muted);
    font-size: 12px;
  }
  .name,
  .rename {
    margin-left: auto;
    min-width: 3em;
    max-width: 60%;
    font: inherit;
    font-size: 12px;
    padding: 1px 8px;
    color: var(--fg);
    background: var(--chip);
    border: 1px solid var(--border);
    border-radius: 5px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }
  .body {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .error {
    padding: 12px;
    color: var(--muted);
  }
</style>
