<script lang="ts">
  // The page of a document window: the file of the window in the editor, filling it, or untitled text kept in the
  // window until it is saved. Markdown for .md, .markdown and .txt, monospace for the others. A file written as
  // typed (FileEditor); the changes made to it elsewhere come as `file-changed`; the window made to show another
  // file (an untitled one opened over) comes as `opened`. Save (⌘S) of the menu comes as `menu`: it writes what is
  // typed, or for untitled text asks where to; Undo, Redo and Symbols… as `edit`. Whether untitled text is blank is
  // told to the backend, which asks before closing a window that has some.
  //
  // References… (⇧⌘R) lists the references over the editor (RefList): the document's bibliography and the papers
  // library. What is put in at the cursor as `[@key]` or `\cite{key}` (⌘. comes as `cite-key`, ⌘L) is added to the
  // document's .bib (`cite_key`), and the front matter named in the text if there was none. The backend reads the
  // references from the text typed, which each call sends.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { FileEditor, PREFS, RefList, insertSymbol, replaceText, setEditorFont, type Found } from "papers-write/editor";
  import { listen } from "$lib/events";
  import "$lib/theme.css";

  /** the file of the window; none while untitled; undefined until the backend says */
  let path = $state<string | null | undefined>();
  let editor = $state<FileEditor>();
  let error = $state("");
  /** a save that failed, where the editor's own errors do not say */
  let problem = $state("");
  /** the text of an untitled window, and whether the backend has been told it is blank */
  let typed = "";
  let blank = true;
  /** the list of references shown over the editor, and how many times the references were read again (it is made anew) */
  let listing = $state(false);
  let refs = $state<RefList>();
  let version = $state(0);
  /** what was done for a key cited, for a moment */
  let note = $state("");
  let noteTimer: ReturnType<typeof setTimeout> | undefined;
  type Check = { missing: string[]; unused: string[] };
  /** the sheet of Check Citations…: the keys, or what went wrong */
  let checking = $state<{ check?: Check; error?: string } | null>(null);
  const markdown = $derived(!path || /\.(md|markdown|txt)$/i.test(path));

  async function ask() {
    path = (await invoke<{ path: string | null }>("document")).path;
  }

  function changed(text: string) {
    typed = text;
    if (!path && blank !== !text.trim()) invoke("set_blank", { blank: (blank = !text.trim()) });
  }

  async function save() {
    if (path) return editor?.flush();
    try {
      problem = "";
      await invoke("save_untitled", { text: typed });
    } catch (e) {
      problem = String(e);
    }
  }

  /** The text as typed. */
  const current = () => editor?.view()?.state.doc.toString() ?? typed;

  const search = (query: string, limit: number) => invoke<Found>("references", { query, limit, text: current() });
  const closest = (selected: string, limit: number) => invoke<Found>("closest_references", { selected, limit, text: current() });
  const sources = () => invoke<string[]>("reference_sources", { text: current() });

  function say(what: string) {
    note = what;
    clearTimeout(noteTimer);
    noteTimer = setTimeout(() => (note = ""), 4000);
  }

  const nameOf = (file: string) => file.split("/").pop() ?? file;

  /** The key cited in the document's bibliography; the front matter that names it replaces the text, the cursor kept. */
  async function add(key: string): Promise<boolean> {
    try {
      problem = "";
      const cited = await invoke<{ bib: string; added: boolean; markdown: string | null }>("cite_key", { key, text: current() });
      const view = editor?.view();
      if (cited.markdown !== null && view) replaceText(view, cited.markdown);
      if (cited.markdown !== null) say(`${nameOf(cited.bib)}${cited.added ? " created" : ""} and named in the front matter`);
      else if (cited.added) say(`Added ${key} to ${nameOf(cited.bib)}`);
      return true;
    } catch (e) {
      problem = String(e);
      return false;
    }
  }

  /** What the list puts in at the cursor; a citation key in it (`[@key]`, `\cite{key}`) is added to the bibliography. */
  function inserted(text: string) {
    const view = editor?.view();
    if (!view) return;
    listing = false;
    insertSymbol(view, text);
    editor?.focus();
    const key = /^\[@([^\]\s]+)\]$/.exec(text)?.[1] ?? /^\\cite\{([^}\s,]+)\}$/.exec(text)?.[1];
    if (key) add(key);
  }

  function openPdf(file: string) {
    listing = false;
    editor?.focus();
    openPath(file).catch((e) => (problem = String(e)));
  }

  function closeList() {
    listing = false;
    editor?.focus();
  }

  function edit(what: string) {
    if (!editor) return;
    if (what === "references") listing = true;
    else if (what === "symbols") editor.list();
    else (what === "redo" ? editor.redo : editor.undo)();
  }

  onMount(() => {
    setEditorFont("system", 13.5);
    ask();
    const unlisten = [
      listen("opened", ask),
      listen("file-changed", () => editor?.changedOnDisk()),
      listen<string>("menu", (e) => e.payload === "save" && save()),
      listen<string>("edit", (e) => edit(e.payload)),
      listen("cite-key", () => listing && refs?.cite("pandoc")),
      listen("references-changed", () => version++),
    ];
    return () => unlisten.forEach((u) => u.then((f) => f()));
  });
</script>

<main>
  {#if path !== undefined}
    {#key path}
      {@const file = path}
      <!-- the text typed is blank until said otherwise: a window is made blank, and one opened over has no text -->
      <FileEditor
        bind:this={editor}
        load={file
          ? async () => ({ path: file, text: await invoke<string>("read_text", { path: file }) })
          : async () => ({ path: "", text: "" })}
        save={file ? (p, text) => invoke("write_text", { path: p, text }) : async () => {}}
        mono={!markdown}
        prefs={PREFS}
        onchange={changed}
        bind:error
      />
    {/key}
  {/if}
  {#if listing}
    {#key version}
      <RefList bind:this={refs} {search} {closest} {sources} oninsert={inserted} onopen={openPdf} onclose={closeList} />
    {/key}
  {/if}
  {#if error || problem}<p class="error">{problem || error}</p>{:else if note}<p class="note">{note}</p>{/if}
</main>

<style>
  main {
    height: 100vh;
    position: relative;
  }
  .error,
  .note {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    margin: 0;
    padding: 6px 14px;
    background: var(--chip);
    border-top: 1px solid var(--line);
    font-size: 12px;
  }
  .error {
    color: var(--error);
  }
</style>
