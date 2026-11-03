<script module lang="ts">
  /** Where a file stands: what is in the editor is in the file, is not yet, or could not be read or written. */
  export type Status = "saved" | "editing" | "error";
</script>

<script lang="ts">
  // A text file edited in place (CodeMirror): the review in its pane, and a file open in a tab of the page. It is
  // written half a second after the typing stops, and as the editor loses the keys. Changed elsewhere (an editor, a
  // sync client: the owner calls `changedOnDisk`), it is read again; with changes of its own not yet written, the bar
  // asks which to keep. Where the file is read from and written to is the owner's: `load` and `save`.

  import { indentLess, indentMore, redo as stepForward, undo as stepBack } from "@codemirror/commands";
  import { Prec, type Extension } from "@codemirror/state";
  import { keymap, type EditorView } from "@codemirror/view";
  import { onMount } from "svelte";
  import { insertSymbol, makeEditor, replaceText, setRules } from "$lib/editor";
  import SymbolList from "$lib/SymbolList.svelte";
  import { DEFAULTS, LATEX, merged } from "$lib/symbols";

  let {
    load,
    save,
    mono = false,
    extensions = [],
    symbols = [],
    onchange,
    status = $bindable("saved"),
    error = $bindable(""),
    path = $bindable(""),
  }: {
    /** the file read: its path, and its text */
    load: () => Promise<{ path: string; text: string }>;
    /** the text written to the file of `path` */
    save: (path: string, text: string) => Promise<void>;
    /** plain text in monospace, not Markdown */
    mono?: boolean;
    /** more extensions of the editor */
    extensions?: Extension[];
    /** the user's own sequences typed that become symbols, and the defaults they turn off (Markdown only) */
    symbols?: [string, string][];
    /** the text after each change typed, and as the file is read */
    onchange?: (text: string) => void;
    /** whether what is shown is written; `error` says what failed */
    status?: Status;
    error?: string;
    /** the path of the file as last read */
    path?: string;
  } = $props();

  let host: HTMLElement;
  let editor: EditorView | undefined;
  /** the text as last read or written */
  let written = "";
  /** changed elsewhere while changes of its own were not written */
  let conflict = $state(false);
  let gone = false;
  /** the list of symbols shown over the editor (⌘/) */
  let listing = $state(false);
  const rules = $derived(merged(DEFAULTS, symbols));
  const entries = $derived([...rules, ...[...LATEX].map(([keys, insert]) => ({ keys, insert }))]);
  // ⌘/ in the editor lists the symbols (the menu's Symbols… has the key first, in the app)
  const listKey = Prec.high(keymap.of([{ key: "Mod-/", run: () => (list(), true) }]));
  $effect(() => {
    const now = rules;
    if (editor) setRules(editor, now);
  });
  let timer: ReturnType<typeof setTimeout> | undefined;

  const text = () => editor?.state.doc.toString() ?? "";
  function failed(e: unknown) {
    status = "error";
    error = String(e);
  }

  /** The file read, into the editor: made the first time. */
  function show(file: { path: string; text: string }) {
    [path, written, conflict] = [file.path, file.text, false];
    if (editor) replaceText(editor, file.text);
    else editor = makeEditor(host, file.text, typed, { mono, rules, extensions: mono ? extensions : [...extensions, listKey] });
    onchange?.(file.text);
    [status, error] = ["saved", ""];
  }

  /** The file read again, what was typed in the editor given up. */
  export async function reload() {
    try {
      const file = await load();
      if (!gone) show(file);
    } catch (e) {
      failed(e);
    }
  }

  function typed(now: string) {
    onchange?.(now);
    status = "editing";
    clearTimeout(timer);
    timer = setTimeout(write, 500);
  }

  /** What was typed, written to the file it was read from. */
  async function write() {
    clearTimeout(timer);
    const now = text();
    if (!editor || now === written || conflict) return;
    try {
      await save(path, now);
      written = now;
      if (text() === now) status = "saved";
    } catch (e) {
      failed(e);
    }
  }

  /** What is typed and not yet written, written now. */
  export function flush() {
    return write();
  }

  /** The file changed on disk: read again, unless it is what was written, or changes typed are not written yet. */
  export async function changedOnDisk() {
    try {
      const file = await load();
      if (gone || file.path !== path || file.text === written) return;
      if (text() === written) show(file);
      else conflict = true;
    } catch (e) {
      failed(e);
    }
  }

  /** The editor of CodeMirror (undefined until the file is read). */
  export const view = () => editor;
  export const focus = () => editor?.focus();
  export const hasFocus = () => editor?.hasFocus ?? false;
  /** The history of the editor stepped back or forward. */
  export const undo = () => editor && stepBack(editor);
  export const redo = () => editor && stepForward(editor);
  /** The lines at the cursor indented less or more (⌘[ and ⌘] of the Go menu). */
  export const outdent = () => editor && indentLess(editor);
  export const indent = () => editor && indentMore(editor);
  /** The symbols listed over the editor, if it is Markdown. */
  export function list() {
    if (!mono && editor) listing = true;
  }

  /** A symbol chosen from the list: at the cursor. */
  function chosen(symbol: string) {
    listing = false;
    if (editor) insertSymbol(editor, symbol);
  }

  onMount(() => {
    reload();
    return () => {
      // what is typed goes out before the editor does
      write();
      gone = true;
      clearTimeout(timer);
      editor?.destroy();
    };
  });
</script>

<div class="file" onfocusout={write} role="presentation">
  {#if conflict}
    <div class="conflict">
      <span>{path.split("/").pop()} changed on disk.</span>
      <button onclick={reload}>Reload</button>
      <button
        onclick={() => {
          conflict = false;
          write();
        }}>Keep mine</button
      >
    </div>
  {/if}
  <div class="host" bind:this={host}></div>
  {#if listing}
    <SymbolList {entries} oninsert={chosen} onclose={() => ((listing = false), editor?.focus())} />
  {/if}
</div>

<style>
  .file {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .host {
    flex: 1;
    min-height: 0;
  }
  .conflict {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 14px;
    background: var(--chip);
    border-bottom: 1px solid var(--line);
  }
  .conflict button {
    font: inherit;
    color: var(--fg);
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 1px 6px;
  }
  .conflict button:hover {
    background: var(--chip);
  }
</style>
