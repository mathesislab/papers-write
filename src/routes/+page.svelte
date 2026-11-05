<script lang="ts">
  // The page of a document window: the file of the window in the editor, filling it, or untitled text kept in the
  // window until it is saved. Markdown for .md, .markdown and .txt, monospace for the others. A file written as
  // typed (FileEditor); the changes made to it elsewhere come as `file-changed`; the window made to show another
  // file (an untitled one opened over) comes as `opened`. Save (⌘S) of the menu comes as `menu`: it writes what is
  // typed, or for untitled text asks where to; Undo, Redo and Symbols… as `edit`. Whether untitled text is blank is
  // told to the backend, which asks before closing a window that has some.
  //
  // Build, Build and Show PDF and Marked-up PDF… of the Build menu come as `menu` too, to Build (the text is written first).
  //
  // References… (⇧⌘R) lists the references over the editor (RefList): the document's bibliography and the papers
  // library. What is put in at the cursor as `[@key]` or `\cite{key}` (⌘. comes as `cite-key`, ⌘L) is added to the
  // document's .bib (`cite_key`), and the front matter named in the text if there was none. The backend reads the
  // references from the text typed, which each call sends. Check Citations… lists in a sheet the keys cited that the
  // document's own bibliography lacks (each to add from the library) and the entries it never cites.
  //
  // View ▸ Show Preview (⇧⌘P, `preview`) toggles the pane at the right of a Markdown document: pandoc's HTML of the text
  // as typed (`preview`), asked 300 ms after the typing stops, and scrolled to the block the cursor is in.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { EditorView } from "@codemirror/view";
  import { ensureSyntaxTree } from "@codemirror/language";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import Preview from "$lib/Preview.svelte";
  import { openPath } from "@tauri-apps/plugin-opener";
  import {
    FileEditor,
    PREFS,
    RefList,
    citation,
    insertSymbol,
    replaceText,
    setEditorFont,
    type CitationOption,
    type Completion,
    type Found,
  } from "papers-write/editor";
  import Build from "$lib/Build.svelte";
  import { listen } from "$lib/events";
  import "$lib/theme.css";

  /** the file of the window; none while untitled; undefined until the backend says */
  let path = $state<string | null | undefined>();
  let editor = $state<FileEditor>();
  let builds = $state<Build>();
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
  /** the pane of the preview: shown, what pandoc made of the text, the block of the cursor, its width */
  let previewing = $state(false);
  let html = $state("");
  let said = $state("");
  let block = $state(0);
  const widthKey = `preview-width:${getCurrentWebview().label}`;
  const stored = (() => {
    try {
      return Number(localStorage.getItem(widthKey));
    } catch {
      return 0;
    }
  })();
  let width = $state(stored >= 240 ? stored : 0);
  let panes = $state<HTMLElement>();
  const markdown = $derived(!path || /\.(md|markdown|txt)$/i.test(path));

  async function ask() {
    path = (await invoke<{ path: string | null }>("document")).path;
  }

  function changed(text: string) {
    typed = text;
    if (previewing) later();
    if (!path && blank !== !text.trim()) invoke("set_blank", { blank: (blank = !text.trim()) });
  }

  let waiting: ReturnType<typeof setTimeout> | undefined;
  let following: ReturnType<typeof setTimeout> | undefined;

  /** The preview asked for after the typing stops. */
  function later() {
    clearTimeout(waiting);
    waiting = setTimeout(refresh, 300);
  }

  /** One run at a time per window: a request while another runs supersedes it (the backend kills the old one). */
  async function refresh() {
    if (!previewing) return;
    try {
      const made = await invoke<{ html: string; error: string | null }>("preview", { text: current() });
      [html, said] = [made.html, made.error ?? ""];
    } catch (e) {
      if (String(e) !== "superseded") said = String(e);
    }
  }

  /** The top-level blocks of the text that pandoc shows, before and including the cursor's: front matter, link
   *  definitions, comments and raw LaTeX environments are not in its HTML. */
  function follow() {
    const view = editor?.view();
    if (!previewing || !view) return;
    const { state } = view;
    const head = state.selection.main.head;
    const tree = ensureSyntaxTree(state, state.doc.length, 50);
    if (!tree) return;
    const hidden = new Set(["FrontMatter", "LinkReference", "CommentBlock", "LatexEnvironment"]);
    let index = -1;
    for (let n = tree.topNode.firstChild; n && n.from <= head; n = n.nextSibling) if (!hidden.has(n.name)) index++;
    block = Math.max(index, 0);
  }

  /** The cursor's moves, told a moment later. */
  const cursor = EditorView.updateListener.of((u) => {
    if (!u.selectionSet && !u.docChanged) return;
    clearTimeout(following);
    following = setTimeout(follow, 150);
  });

  function togglePreview() {
    if (!markdown) return;
    previewing = !previewing;
    if (previewing) {
      follow();
      refresh();
    }
  }

  /** The split dragged: the width of the preview, kept for the window. */
  function drag(e: PointerEvent) {
    const bar = e.currentTarget as HTMLElement;
    bar.setPointerCapture(e.pointerId);
    const move = (m: PointerEvent) => {
      const total = panes?.clientWidth ?? 0;
      width = Math.round(Math.min(Math.max(panes!.getBoundingClientRect().right - m.clientX, 240), total - 240));
    };
    const stop = () => {
      bar.removeEventListener("pointermove", move);
      bar.removeEventListener("pointerup", stop);
      try {
        localStorage.setItem(widthKey, String(width));
      } catch {}
    };
    bar.addEventListener("pointermove", move);
    bar.addEventListener("pointerup", stop);
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

  /** The completion of citations as typed (`[@`, `@`, `\cite{`): the references matching what is typed, those of the
   *  document's own bibliography marked as such, a key in both once; a key taken is added to the bibliography. */
  const completion: Completion = {
    async citations(prefix) {
      const [found, own] = await Promise.all([search(prefix, 30), invoke<string[]>("bibliography_files", { text: current() })]);
      const options = new Map<string, CitationOption>();
      for (const { item } of found.hits) {
        const inBib = own.includes(item.bib);
        if (!options.get(item.key)?.inBib) options.set(item.key, { key: item.key, label: citation(item), detail: item.title, inBib });
      }
      // the keys that start with what is typed first, then those of the bibliography, else as the references found them
      const rank = (o: CitationOption) => (o.key.toLowerCase().startsWith(prefix.toLowerCase()) ? 0 : 2) + (o.inBib ? 0 : 1);
      return [...options.values()].sort((a, b) => rank(a) - rank(b));
    },
    oncite: (key) => void add(key),
  };

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

  /** The sheet of Check Citations…, made or made again. */
  async function check() {
    try {
      checking = { check: await invoke<Check>("bib_check", { text: current() }) };
    } catch (e) {
      checking = { error: String(e) };
    }
  }

  async function addMissing(key: string) {
    if (await add(key)) await check();
  }

  function closeCheck() {
    checking = null;
    editor?.focus();
  }

  function edit(what: string) {
    if (!editor) return;
    if (what === "check") check();
    else if (what === "references") listing = true;
    else if (what === "symbols") editor.list();
    else (what === "redo" ? editor.redo : editor.undo)();
  }

  /** The keys to the sheet as it opens, so Esc reaches it. */
  const grab = (node: HTMLElement) => node.focus();

  onMount(() => {
    setEditorFont("system", 13.5);
    ask();
    const unlisten = [
      listen("opened", ask),
      listen("file-changed", () => editor?.changedOnDisk()),
      listen<string>("menu", (e) => (e.payload === "save" ? save() : builds?.run(e.payload))),
      listen<string>("edit", (e) => edit(e.payload)),
      listen("preview", togglePreview),
      listen("cite-key", () => listing && refs?.cite("pandoc")),
      listen("references-changed", () => version++),
    ];
    return () => unlisten.forEach((u) => u.then((f) => f()));
  });
</script>

<main>
  <div class="panes" bind:this={panes}>
  <div class="text">
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
        {completion}
        autofocus
        extensions={[cursor]}
        onchange={changed}
        bind:error
      />
    {/key}
  {/if}
  </div>
  {#if previewing && markdown}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="split" onpointerdown={drag}></div>
    <div class="pane" style:width={width ? `${width}px` : "45%"}><Preview {html} error={said} {block} /></div>
  {/if}
  </div>
  {#if listing}
    {#key version}
      <RefList bind:this={refs} enter="pandoc" {search} {closest} {sources} oninsert={inserted} onopen={openPdf} onclose={closeList} />
    {/key}
  {/if}
  {#if checking}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="sheet" role="dialog" aria-label="Check Citations" tabindex="-1" onkeydown={(e) => e.key === "Escape" && closeCheck()} use:grab>
      <header>
        <b>Check Citations</b>
        <button onclick={closeCheck}>Close</button>
      </header>
      {#if checking.error}
        <p class="error">{checking.error}</p>
      {:else if checking.check}
        {@const { missing, unused } = checking.check}
        {#if !missing.length && !unused.length}<p>Every key cited is in the bibliography, and every entry is cited.</p>{/if}
        {#if missing.length}
          <h3>Not in the bibliography ({missing.length})</h3>
          <ul>
            {#each missing as key (key)}
              <li><code>{key}</code><button onclick={() => addMissing(key)}>Add from library</button></li>
            {/each}
          </ul>
        {/if}
        {#if unused.length}
          <h3>Never cited ({unused.length})</h3>
          <ul>
            {#each unused as key (key)}<li><code>{key}</code></li>{/each}
          </ul>
        {/if}
      {/if}
    </div>
  {/if}
  <Build bind:this={builds} flush={() => editor?.flush()} onfocus={() => editor?.focus()} />
  {#if error || problem}<p class="error">{problem || error}</p>{:else if note}<p class="note">{note}</p>{/if}
</main>

<style>
  main {
    height: 100vh;
    position: relative;
  }
  .panes {
    display: flex;
    height: 100%;
  }
  .text {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 100%;
  }
  .split {
    flex: none;
    width: 5px;
    cursor: col-resize;
    background: var(--line);
  }
  .pane {
    flex: none;
    min-width: 240px;
    max-width: 80%;
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
  .sheet {
    position: absolute;
    z-index: 6;
    top: 12px;
    right: 14px;
    width: min(420px, calc(100% - 28px));
    max-height: calc(100% - 60px);
    overflow: auto;
    box-sizing: border-box;
    padding: 10px 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.18);
    font-size: 12.5px;
    outline: none;
  }
  .sheet header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .sheet h3 {
    margin: 12px 0 4px;
    font-size: 12px;
    color: var(--muted, inherit);
  }
  .sheet ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .sheet li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 2px 0;
  }
  .sheet button {
    font: inherit;
    color: var(--fg);
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 1px 6px;
  }
  .sheet button:hover {
    background: var(--chip);
  }
  .sheet p.error {
    position: static;
    padding: 4px 0;
    border: none;
  }
</style>
