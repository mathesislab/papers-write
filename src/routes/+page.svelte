<script lang="ts">
  // The page of a document window: the file of the window in the editor, filling it, or untitled text kept in the
  // window until it is saved. Markdown for .md, .markdown and .txt, monospace for the others. A file written as
  // typed (FileEditor); the changes made to it elsewhere come as `file-changed`; the window made to show another
  // file (an untitled one opened over) comes as `opened`. Save (⌘S) of the menu comes as `menu`: it writes what is
  // typed, or for untitled text asks where to; Undo, Redo and Symbols… as `edit`. Whether untitled text is blank is
  // told to the backend, which asks before closing a window that has some.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { FileEditor, PREFS, setEditorFont } from "papers-write/editor";
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

  function edit(what: string) {
    if (!editor) return;
    if (what === "symbols") editor.list();
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
  {#if error || problem}<p class="error">{problem || error}</p>{/if}
</main>

<style>
  main {
    height: 100vh;
    position: relative;
  }
  .error {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    margin: 0;
    padding: 6px 14px;
    color: var(--error);
    background: var(--chip);
    border-top: 1px solid var(--line);
    font-size: 12px;
  }
</style>
