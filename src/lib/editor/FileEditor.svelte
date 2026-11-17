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
  import { Decoration, EditorView, keymap } from "@codemirror/view";
  import { StateEffect, StateField } from "@codemirror/state";
  import { onMount } from "svelte";
  import { PREFS, insertSymbol, makeEditor, replaceText, selectedWords, setPrefs, setRules, setSpelling, type EditorPrefs } from "./editor";
  import type { Completion, Diagnostics, HoverSources, Spelling } from "./types";
  import { recheck } from "./diagnostics";
  import { findBar, find as findIn, replaceEverywhere, type FindBar, type FindCommand, type ProjectQuery } from "./find";
  import { format as formatIn, type Format } from "./format";
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
    spelling,
    onoutline,
    review,
    placeholder,
    autofocus = false,
    symbols = [],
    prefs = PREFS,
    onsync,
    oncontext,
    onchange,
    status = $bindable("saved"),
    error = $bindable(""),
    path = $bindable(""),
    selected = $bindable(0),
    findbar = $bindable(null),
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
    /** the system's spell checker on the prose (Markdown files only); none turns it off */
    spelling?: Spelling;
    /** the headings of the text and the one the cursor is under, told as they change (Markdown and LaTeX), for an
     *  outline the app shows (Outline); `goToHeading` takes the cursor to one */
    onoutline?: (headings: Heading[], here: number) => void;
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
    /** ⌃-click (⌘ on macOS) in the text: the cursor put there, then this (its place shown in the PDF or the preview);
     *  extra cursors are made with ⌥-click instead */
    onsync?: () => void;
    /** a right-click in the text: the cursor put there (unless it is in the selection), then the app's menu */
    oncontext?: (e: MouseEvent) => void;
    /** the text after each change typed, and as the file is read */
    onchange?: (text: string) => void;
    /** whether what is shown is written; `error` says what failed */
    status?: Status;
    error?: string;
    /** the path of the file as last read */
    path?: string;
    /** the words selected */
    selected?: number;
    /** what the find bar shows: none, Find, or Find and Replace */
    findbar?: FindBar;
  } = $props();

  /** The place found from elsewhere (`markFound`), marked till the text changes or the cursor moves. */
  const showFound = (from: number, to: number) => foundEffect.of({ from, to });
  const foundEffect = StateEffect.define<{ from: number; to: number }>();
  const foundMark = Decoration.mark({ class: "cm-found" });
  const found = [
    StateField.define({
      create: () => Decoration.none,
      update(set, tr) {
        for (const e of tr.effects) if (e.is(foundEffect)) return e.value.from < e.value.to ? Decoration.set(foundMark.range(e.value.from, e.value.to)) : Decoration.none;
        return tr.docChanged || tr.selection ? Decoration.none : set;
      },
      provide: (f) => EditorView.decorations.from(f),
    }),
    EditorView.baseTheme({
      // a highlighter's yellow: the text's own colours (links, citations, review) are often blue
      ".cm-found": { background: "rgba(255, 214, 0, 0.45)", borderRadius: "2px" },
      "&dark .cm-found": { background: "rgba(255, 214, 0, 0.32)" },
    }),
  ];

  /** A place shown a moment (`flash`: the paragraph a click in the preview scrolled to), its lines lit; the cursor
   *  left where it is. */
  const flashEffect = StateEffect.define<{ from: number; to: number } | null>();
  const flashLine = Decoration.line({ class: "cm-flash" });
  const flashing = [
    StateField.define({
      create: () => Decoration.none,
      update(set, tr) {
        for (const e of tr.effects) {
          if (!e.is(flashEffect)) continue;
          if (!e.value) return Decoration.none;
          const { doc } = tr.state;
          const [first, last] = [doc.lineAt(Math.min(e.value.from, doc.length)).number, doc.lineAt(Math.min(e.value.to, doc.length)).number];
          const lines = [];
          for (let n = first; n <= last; n++) lines.push(flashLine.range(doc.line(n).from));
          return Decoration.set(lines);
        }
        return tr.docChanged ? Decoration.none : set;
      },
      provide: (f) => EditorView.decorations.from(f),
    }),
    EditorView.baseTheme({
      ".cm-flash": { background: "rgba(255, 221, 0, 0.22)" },
      "&dark .cm-flash": { background: "rgba(255, 221, 0, 0.14)" },
    }),
  ];
  let flashTimer: ReturnType<typeof setTimeout> | undefined;

  /** ⌃-click: the place shown elsewhere (`onsync`); a right-click: the app's menu (`oncontext`). */
  const clicks = [
    EditorView.clickAddsSelectionRange.of((e) => e.altKey),
    EditorView.domEventHandlers({
      mousedown(e, view) {
        if (!onsync || e.button !== 0 || !(e.ctrlKey || e.metaKey) || e.altKey) return false;
        const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
        if (pos == null) return false;
        e.preventDefault();
        view.dispatch({ selection: { anchor: pos } });
        view.focus();
        onsync();
        return true;
      },
      contextmenu(e, view) {
        if (!oncontext) return false;
        e.preventDefault();
        const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
        const { from, to } = view.state.selection.main;
        if (pos != null && (pos < from || pos > to)) view.dispatch({ selection: { anchor: pos } });
        oncontext(e);
        return true;
      },
    }),
  ];

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
  /** the headings of the text and the one the cursor is under (kept while someone listens) */
  let headings: Heading[] = [];
  let here = -1;
  const selection = EditorView.updateListener.of((u) => {
    if (u.selectionSet || u.docChanged) selected = selectedWords(u.state);
    findbar = findBar(u.state);
    if (onoutline && (u.docChanged || u.selectionSet)) {
      const before = here;
      if (u.docChanged) headings = outlineOf(u.state);
      here = currentHeading(headings, u.state.selection.main.head);
      if (u.docChanged || here !== before) onoutline(headings, here);
    }
  });
  /** The headings of the text as it is, told (when someone listens). */
  function readHeadings() {
    if (!onoutline || !editor) return;
    headings = outlineOf(editor.state);
    here = currentHeading(headings, editor.state.selection.main.head);
    onoutline(headings, here);
  }
  /** A heading chosen in the outline: the cursor there, its line near the top. */
  export function goToHeading(heading: Heading) {
    if (!editor) return;
    editor.dispatch({ selection: { anchor: heading.from }, effects: EditorView.scrollIntoView(heading.from, { y: "start", yMargin: 12 }) });
    editor.focus();
  }
  $effect(() => {
    const now = rules;
    if (editor) setRules(editor, now);
  });
  $effect(() => {
    const now = spelling;
    if (editor && !mono && !latex) setSpelling(editor, now);
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
        spelling,
        placeholder,
        extensions: [...extensions, selection, found, flashing, clicks, ...(mono || latex ? [] : [listKey]), reviewSlot.of(review ? reviewOf(review) : [])],
      });
    if (review && reviewed !== review) {
      reviewed = review;
      if (mode !== "edit") setReviewMode(editor, mode);
      if (shownView !== "markup") setReviewView(editor, shownView);
      if (items) showReview(editor, items);
    }
    readHeadings();
    if (selecting) {
      const { from, to, mark } = selecting;
      selecting = null;
      queueMicrotask(() => (mark ? markFound(from, to) : select(from, to)));
    }
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
  /** A command of Find (the menu's): the bar, the next or previous match, the selection to find. */
  export function find(what: FindCommand) {
    if (editor) findIn(editor, what);
  }

  /** Bold, italic, a heading, a list… at the selection (Markdown, or LaTeX in a LaTeX file; none in plain text). */
  export function format(what: Format) {
    if (editor && (!mono || latex)) formatIn(editor, what, latex);
  }

  /** Every match of a search of the project replaced (in suggest mode, as suggestions). */
  export function replaceAll(query: ProjectQuery, by: string) {
    if (editor) replaceEverywhere(editor, query, by);
  }

  /** the range to select once the file is read, when asked before */
  let selecting: { from: number; to: number; mark?: boolean } | null = null;

  /** The range [from, to] (UTF-16) selected, in view, with the keys; once the file is read if it is not yet. */
  export function select(from: number, to: number) {
    if (!editor) return void (selecting = { from, to });
    const end = editor.state.doc.length;
    const range = { anchor: Math.min(from, end), head: Math.min(to, end) };
    editor.dispatch({ selection: range, effects: EditorView.scrollIntoView(range.anchor, { y: "center" }) });
    editor.focus();
  }

  /** The lines from `from` to `to` lit a moment (1.5 s), the cursor left where it is. */
  export function flash(from: number, to: number) {
    if (!editor) return;
    editor.dispatch({ effects: flashEffect.of({ from, to }) });
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => editor?.dispatch({ effects: flashEffect.of(null) }), 1500);
  }

  /** A place found elsewhere (from a PDF): the cursor at its start, the place marked till the user types or moves. */
  export function markFound(from: number, to: number) {
    if (!editor) return void (selecting = { from, to, mark: true });
    const end = editor.state.doc.length;
    [from, to] = [Math.min(from, end), Math.min(to, end)];
    editor.dispatch({ selection: { anchor: from }, effects: [EditorView.scrollIntoView(from, { y: "center" }), showFound(from, to)] });
    editor.focus();
  }

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
