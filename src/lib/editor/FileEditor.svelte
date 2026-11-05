<script module lang="ts">
  /** Where a file stands: what is in the editor is in the file, is not yet, or could not be read or written. */
  export type Status = "saved" | "editing" | "error";
</script>

<script lang="ts">
  // A text file edited in place (CodeMirror): the review in its pane, and a file open in a tab of the page. It is
  // written half a second after the typing stops, and as the editor loses the keys. Changed elsewhere (an editor, a
  // sync client: the owner calls `changedOnDisk`), it is read again; with changes of its own not yet written, the bar
  // asks which to keep. Where the file is read from and written to is the owner's: `load` and `save`. With `review`,
  // the suggestions and comments are drawn over the text and typing in suggest mode makes the user's suggestions.

  import { indentLess, indentMore, redo as stepForward, undo as stepBack } from "@codemirror/commands";
  import { Compartment, Prec, type Extension } from "@codemirror/state";
  import { EditorView, keymap } from "@codemirror/view";
  import { onMount } from "svelte";
  import { PREFS, insertSymbol, makeEditor, replaceText, selectedWords, setPrefs, setRules, type EditorPrefs } from "./editor";
  import type { Completion, Diagnostics, HoverSources } from "./types";
  import { recheck } from "./diagnostics";
  import Outline from "./Outline.svelte";
  import { currentHeading, outline as outlineOf, type Heading } from "./outline";
  import {
    accept as acceptItem,
    redoSuggestion,
    review as reviewOf,
    scrollToItem,
    setReview as showReview,
    setReviewMode,
    setReviewView,
    undoSuggestion,
  } from "./review/review";
  import type { ReviewHandlers, ReviewItem, ReviewMode, ReviewView } from "./review/types";
  import SymbolList from "./SymbolList.svelte";
  import { DEFAULTS, LATEX, merged } from "./symbols";

  let {
    load,
    save,
    mono = false,
    latex = false,
    extensions = [],
    completion,
    hover,
    diagnostics,
    outline = false,
    review,
    placeholder,
    autofocus = false,
    symbols = [],
    prefs = PREFS,
    onchange,
    status = $bindable("saved"),
    error = $bindable(""),
    path = $bindable(""),
    selected = $bindable(0),
  }: {
    /** the file read: its path, and its text */
    load: () => Promise<{ path: string; text: string }>;
    /** the text written to the file of `path` */
    save: (path: string, text: string) => Promise<void>;
    /** plain text in monospace, not Markdown */
    mono?: boolean;
    /** a LaTeX file: highlighted and completed in monospace, no symbols typed */
    latex?: boolean;
    /** more extensions of the editor */
    extensions?: Extension[];
    /** what the completion of citations, references and LaTeX takes from the app (none without it) */
    completion?: Completion;
    /** what the tooltips over formulas, citation keys and images take from the app (cross-references need none; no
     *  tooltips without it) */
    hover?: HoverSources;
    /** the keys underlined where the text cannot resolve them (labels, and the citations the app says are missing; none
     *  without it) */
    diagnostics?: Diagnostics;
    /** the outline of the headings in a column at the left of the text (Markdown and LaTeX) */
    outline?: boolean;
    /** what the review asks of the app: with it, the items given by `setReview` are drawn over the text and typing in
     *  suggest mode (`setMode`) makes the user's suggestions (none without it) */
    review?: ReviewHandlers;
    /** what the editor shows while empty (none without it) */
    placeholder?: string;
    /** the keys to the editor once the file is read (a window of its own) */
    autofocus?: boolean;
    /** the user's own sequences typed that become symbols, and the defaults they turn off (Markdown only) */
    symbols?: [string, string][];
    /** what the editor takes from the settings */
    prefs?: EditorPrefs;
    /** the text after each change typed, and as the file is read */
    onchange?: (text: string) => void;
    /** whether what is shown is written; `error` says what failed */
    status?: Status;
    error?: string;
    /** the path of the file as last read */
    path?: string;
    /** the words selected */
    selected?: number;
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
  /** the headings of the text and the one the cursor is under (kept while the outline is shown) */
  let headings = $state.raw<Heading[]>([]);
  let here = $state(-1);
  const selection = EditorView.updateListener.of((u) => {
    if (u.selectionSet || u.docChanged) selected = selectedWords(u.state);
    if (outline && (u.docChanged || u.selectionSet || u.viewportChanged)) {
      if (u.docChanged) headings = outlineOf(u.state);
      here = currentHeading(headings, u.state.selection.main.head);
    }
  });
  function readHeadings() {
    if (!outline || !editor) return;
    headings = outlineOf(editor.state);
    here = currentHeading(headings, editor.state.selection.main.head);
  }
  // shown: the headings of the text as it is
  $effect(() => {
    outline;
    readHeadings();
  });
  /** A heading chosen in the outline: the cursor there, its line near the top. */
  function goTo(heading: Heading) {
    if (!editor) return;
    editor.dispatch({ selection: { anchor: heading.from }, effects: EditorView.scrollIntoView(heading.from, { y: "start", yMargin: 12 }) });
    editor.focus();
  }
  $effect(() => {
    const now = rules;
    if (editor) setRules(editor, now);
  });
  $effect(() => {
    const now = prefs;
    if (editor) setPrefs(editor, now);
  });
  let timer: ReturnType<typeof setTimeout> | undefined;
  /** the review's items and mode given before the file was read */
  let items: ReviewItem[] | undefined;
  let mode: ReviewMode = "edit";
  let shownView: ReviewView = "markup";

  const text = () => editor?.state.doc.toString() ?? "";
  function failed(e: unknown) {
    status = "error";
    error = String(e);
  }

  /** the review in the editor: given as the app has it, which may be after the file is read */
  const reviewSlot = new Compartment();
  let reviewed: ReviewHandlers | undefined;
  $effect(() => {
    const now = review;
    if (!editor || now === reviewed) return;
    reviewed = now;
    editor.dispatch({ effects: reviewSlot.reconfigure(now ? reviewOf(now) : []) });
    if (now) {
      if (mode !== "edit") setReviewMode(editor, mode);
      if (shownView !== "markup") setReviewView(editor, shownView);
      if (items) showReview(editor, items);
    }
  });

  /** The file read, into the editor: made the first time. */
  function show(file: { path: string; text: string }) {
    [path, written, conflict] = [file.path, file.text, false];
    if (editor) replaceText(editor, file.text);
    else editor = makeEditor(host, file.text, typed, {
        mono,
        latex,
        rules,
        prefs,
        completion,
        hover,
        diagnostics,
        placeholder,
        extensions: [...extensions, selection, ...(mono || latex ? [] : [listKey]), reviewSlot.of(review ? reviewOf(review) : [])],
      });
    if (review && reviewed !== review) {
      reviewed = review;
      if (mode !== "edit") setReviewMode(editor, mode);
      if (shownView !== "markup") setReviewView(editor, shownView);
      if (items) showReview(editor, items);
    }
    readHeadings();
    onchange?.(file.text);
    [status, error] = ["saved", ""];
    if (autofocus && !editor.hasFocus) editor.focus();
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
      // what is in the editor already: it is what was written
      if (file.text === text()) {
        written = file.text;
        [status, error] = ["saved", ""];
      } else if (text() === written) show(file);
      else conflict = true;
    } catch (e) {
      failed(e);
    }
  }

  /** The citation keys asked of the app again (its bibliography or library changed), with `diagnostics`. */
  export const recheckKeys = () => editor && recheck(editor);

  /** The editor of CodeMirror (undefined until the file is read). */
  export const view = () => editor;
  export const focus = () => editor?.focus();
  export const hasFocus = () => editor?.hasFocus ?? false;
  /** The history of the editor stepped back or forward; in suggest mode that of the user's suggestions. */
  export const undo = () => editor && (undoSuggestion(editor) || stepBack(editor));
  export const redo = () => editor && (redoSuggestion(editor) || stepForward(editor));

  /** The review's items, as the app has them on the text in the editor (with `review`). */
  export function setReview(now: ReviewItem[]) {
    items = now;
    if (editor && review) showReview(editor, now);
  }
  /** Edit or suggest (with `review`). */
  export function setMode(now: ReviewMode) {
    mode = now;
    if (now === "suggest") shownView = "markup";
    if (editor && review) setReviewMode(editor, now);
  }
  /** All Markup, Accepted or Original (with `review`): what is drawn of the suggestions; the text is the file's in all
   *  (not in suggest mode, which is All Markup). */
  export function setView(now: ReviewView) {
    if (mode === "suggest" && now !== "markup") return;
    shownView = now;
    if (editor && review) setReviewView(editor, now);
  }
  /** A suggestion accepted: its text put in, as typed (written as any change). */
  export const accept = (item: ReviewItem) => !!editor && !!review && acceptItem(editor, item);
  /** The item `id` scrolled to and selected; false if it is not shown. */
  export const scrollTo = (id: string) => !!editor && !!review && scrollToItem(editor, id);
  /** The lines at the cursor indented less or more (⌘[ and ⌘] of the Go menu). */
  export const outdent = () => editor && indentLess(editor);
  export const indent = () => editor && indentMore(editor);
  /** The cursor at the start of line `n` (1-based; the last if there are fewer), in view, with the keys. */
  export function goToLine(n: number) {
    if (!editor) return;
    const { doc } = editor.state;
    const line = doc.line(Math.max(1, Math.min(n, doc.lines)));
    editor.dispatch({ selection: { anchor: line.from }, effects: EditorView.scrollIntoView(line.from, { y: "center" }) });
    editor.focus();
  }
  /** The symbols listed over the editor, if it is Markdown. */
  export function list() {
    if (!mono && !latex && editor) listing = true;
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
  <div class="body">
    {#if outline}<Outline {headings} current={here} onselect={goTo} />{/if}
    <div class="host" bind:this={host}></div>
  </div>
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
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .host {
    flex: 1;
    min-width: 0;
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
