<script lang="ts">
  // The page of a document window: the file of the window in the editor, filling it, or untitled text kept in the
  // window until it is saved. Markdown for .md, .markdown and .txt, monospace for the others. A file written as
  // typed (FileEditor); the changes made to it elsewhere come as `file-changed`; the window made to show another
  // file (an untitled one opened over) comes as `opened`.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { FileEditor, PREFS, setEditorFont } from "papers-write/editor";
  import { listen } from "$lib/events";
  import "$lib/theme.css";

  /** the file of the window; none while untitled; undefined until the backend says */
  let path = $state<string | null | undefined>();
  let editor = $state<FileEditor>();
  let error = $state("");
  const markdown = $derived(!path || /\.(md|markdown|txt)$/i.test(path));

  async function ask() {
    path = (await invoke<{ path: string | null }>("document")).path;
  }

  onMount(() => {
    setEditorFont("system", 13.5);
    ask();
    const unlisten = [
      listen("opened", ask),
      listen("file-changed", () => editor?.changedOnDisk()),
    ];
    return () => unlisten.forEach((u) => u.then((f) => f()));
  });
</script>

<main>
  {#if path !== undefined}
    {#key path}
      {@const file = path}
      <FileEditor
        bind:this={editor}
        load={file
          ? async () => ({ path: file, text: await invoke<string>("read_text", { path: file }) })
          : async () => ({ path: "", text: "" })}
        save={file ? (p, text) => invoke("write_text", { path: p, text }) : async () => {}}
        mono={!markdown}
        prefs={PREFS}
        bind:error
      />
    {/key}
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
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
