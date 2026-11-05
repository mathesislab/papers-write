<script lang="ts">
  // The page of a document window: the file of the window in the editor, filling it, or untitled text kept in the
  // window until it is saved. Markdown for .md, .markdown and .txt, LaTeX for .tex, .sty and .cls, monospace for the others. A file written as
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
  // The foot of the window says the state of the document's repository (GitBar; a click opens the Commit sheet). The Git
  // menu (`git-menu`) is Git: Commit…, Pull, Push. After suggestions are accepted or rejected the review panel offers
  // a commit with a message that says what was decided.
  //
  // File ▸ Exchange (`exchange`) is Exchange: Compare as Suggestions… and the Word round trip.
  //
  // A document of a project (a folder above it has write.toml) shows the project bar at the left (ProjectBar of
  // papers-write/project: a tile per project, the main document's words and open suggestions; a click opens the
  // project's main document, a right-click its menu) and a thin bar in the project's colour at the top. Project
  // Info… (⌘I, `project-info`) is a card over the window (ProjectCard); so is the card of a new project, asked for
  // when File ▸ Open Project… chose a folder without write.toml (`asking-project`). The bar is asked again on
  // `projects-changed`, when the window gets the focus, and a moment after the file or its review changes.
  //
  // View ▸ Show Preview (⇧⌘P, `preview`) toggles the pane at the right of a Markdown document: pandoc's HTML of the text
  // as typed (`preview`), asked 300 ms after the typing stops, and scrolled to the block the cursor is in.
  // View ▸ Show Outline (⌥⌘O, `outline`) toggles the column of headings at the left of a Markdown or LaTeX document.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount, untrack } from "svelte";
  import { EditorView } from "@codemirror/view";
  import { ensureSyntaxTree } from "@codemirror/language";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { Menu, MenuItem } from "@tauri-apps/api/menu";
  import { ProjectBar, cssOf } from "papers-write/project";
  import ProjectCard from "$lib/ProjectCard.svelte";
  import type { Bar } from "$lib/projects";
  import Preview from "$lib/Preview.svelte";
  import CommentBox from "$lib/CommentBox.svelte";
  import ReviewPanel from "$lib/ReviewPanel.svelte";
  import { ReviewSession } from "$lib/review.svelte";
  import { colorOf } from "$lib/reviewFormat";
  import { openPath } from "@tauri-apps/plugin-opener";
  import {
    FileEditor,
    PREFS,
    RefList,
    citation,
    insertSymbol,
    replaceText,
    setOwnColour,
    type EditorPrefs,
    type Item,
    type Completion,
    type Diagnostics,
    type Found,
    type ReviewView,
    type HoverSources,
  } from "papers-write/editor";
  import Build from "$lib/Build.svelte";
  import Exchange from "$lib/Exchange.svelte";
  import Git from "$lib/Git.svelte";
  import GitBar from "$lib/GitBar.svelte";
  import type { GitState } from "$lib/git";
  import { listen } from "$lib/events";
  import { applyEditor, watchSettings, type Settings } from "$lib/settings";
  import "$lib/theme.css";

  // The review (Review menu, `review-menu`): suggesting or editing (a badge at the top right while suggesting), the
  // suggestions and comments of the review files carried onto the text as typed (`review_items`, asked again 500 ms
  // after the typing stops, after each write of the user's, on `review-changed` and when the file changes on disk),
  // given to the editor with `setReview`. Review ▸ Show Review Panel (⌥⌘R) shows them in a column at the right
  // (ReviewPanel), in place of the preview: the two do not show together. Add Comment… (⌥⌘M) opens a box at the selection; ⌘↵ posts.

  /** the file of the window; none while untitled; undefined until the backend says */
  let path = $state<string | null | undefined>();
  let editor = $state<FileEditor>();
  let builds = $state<Build>();
  let exchange = $state<Exchange>();
  let gitbar = $state<GitBar>();
  /** the repository of the document (git.rs); none when it is not in one */
  let repo = $state<GitState | null>(null);
  let git = $state<Git>();
  /** the message of the commit offered in the review panel after suggestions were decided; none when not offered */
  let offered = $state("");
  /** whose draft this window is a review of, when it was opened from a `.review` (a bar at the top; suggest mode to begin with) */
  let reviewing = $state<{ name: string; email: string; round: string; document: string } | null>(null);
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
  /** the outline of the headings at the left of the text: shown, kept for the next window */
  let outlining = $state(
    (() => {
      try {
        return localStorage.getItem("outline") === "1";
      } catch {
        return false;
      }
    })(),
  );
  function toggleOutline() {
    outlining = !outlining;
    try {
      localStorage.setItem("outline", outlining ? "1" : "0");
    } catch {}
    if (!outlining) editor?.focus();
  }
  const session = new ReviewSession({ text: () => current(), editor: () => editor, say: (what) => say(what), decided: (m) => (offered = m) });
  /** the review panel at the right: shown (the preview is not while it is) */
  let panel = $state(false);
  /** the comment being written: the selection it is on and where the box goes */
  let commenting = $state<{ from: number; to: number; x: number; y: number } | null>(null);
  const markdown = $derived(!path || /\.(md|markdown|txt)$/i.test(path));
  /** a LaTeX file: highlighted and completed, with no symbols typed */
  const latex = $derived(!!path && /\.(tex|sty|cls)$/i.test(path));

  async function ask() {
    path = (await invoke<{ path: string | null }>("document")).path;
    loadBar();
  }

  /** the project bar: the project of the document (none: no bar) and its tiles */
  let bar = $state<Bar>({ current: null, tiles: [] });
  /** the folder of the project whose info card is shown */
  let carding = $state<string | null>(null);
  /** the folder opened as a project that has no write.toml yet, whose card asks what to write in one */
  let asking = $state<string | null>(null);
  let barTimer: ReturnType<typeof setTimeout> | undefined;
  const barColour = $derived(bar.current ? cssOf(bar.current.colour) : null);

  async function loadBar() {
    clearTimeout(barTimer);
    try {
      bar = await invoke<Bar>("project_bar");
    } catch (e) {
      problem = String(e);
    }
  }

  /** The bar asked again a moment later (the words and suggestions of a file being written). */
  function loadBarLater(ms: number) {
    clearTimeout(barTimer);
    barTimer = setTimeout(loadBar, ms);
  }

  async function askProject() {
    asking = await invoke<string | null>("asking_project");
  }

  function switchProject(root: string) {
    invoke("switch_project", { root }).catch((e) => (problem = String(e)));
  }

  /** A right-click on a tile. */
  async function tileMenu(root: string) {
    const items = await Promise.all([
      MenuItem.new({ text: "Open Project Folder in Finder", action: () => void invoke("reveal", { path: root }).catch((e) => (problem = String(e))) }),
      MenuItem.new({ text: "Project Info…", action: () => (carding = root) }),
      MenuItem.new({ text: "Remove from Bar", action: () => void invoke("remove_from_bar", { root }) }),
    ]);
    await (await Menu.new({ items })).popup();
  }

  function projectInfo() {
    if (bar.current) carding = bar.current.root;
    else say("Not in a project: File ▸ Open Project… makes a folder one");
  }

  function closeCard() {
    carding = null;
    asking = null;
    editor?.focus();
  }

  function changed(text: string) {
    typed = text;
    if (previewing) later();
    session.reload(500);
    // the file is written half a second after the typing stops
    gitbar?.refresh(1500);
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
    if (!previewing || !view || !follows) return;
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
      panel = false;
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
    // the backend ranks them: the document's own entries, then the keys that start so, then other matches
    async citations(prefix) {
      const found = await invoke<{ item: Item; in_bib: boolean }[]>("cite_options", { prefix, limit: 30, text: current() });
      return found.map(({ item, in_bib }) => ({ key: item.key, label: citation(item), detail: item.title, inBib: in_bib }));
    },
    oncite: (key) => void add(key),
  };

  /** The tooltips over the text: a formula rendered (pandoc's MathML), a citation key's reference, an image's thumbnail. */
  const hover: HoverSources = {
    math: (tex, display) => invoke<string>("render_math", { tex, display }),
    async citation(key) {
      const item = await invoke<Item | null>("reference_of", { key, text: current() });
      if (!item) return null;
      const detail = [item.title, item.container ?? item.server].filter(Boolean).join(". ");
      return { label: citation(item), detail };
    },
    image: (path) => invoke<string | null>("image_data", { path }),
  };

  /** The keys underlined: the citations neither the document's bibliography nor the library has (labels the editor sees itself). */
  const diagnostics: Diagnostics = {
    missingKeys: async (keys) => new Set(await invoke<string[]>("missing_keys", { keys, text: current() })),
  };
  // the references changed (a .bib was edited, a key added): the keys are asked again
  $effect(() => {
    version;
    untrack(() => editor?.recheckKeys());
  });

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
      if (cited.markdown !== null) say(`${nameOf(cited.bib)}${cited.added ? " created" : ""} and named in the ${markdown ? "front matter" : "document"}`);
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

  function togglePanel() {
    panel = !panel;
    if (panel) previewing = false;
  }

  /** A choice of the Review menu. */
  function reviewMenu(what: string) {
    if (what === "suggest") session.toggleMode();
    else if (what.startsWith("view-")) session.setView(what.slice(5) as ReviewView);
    else if (what === "panel") togglePanel();
    else if (what === "comment") comment();
    else if (what === "accept-all") session.acceptAll().catch(() => {});
    else if (what === "reject-all") session.rejectAll().catch(() => {});
    else if (what === "cleanup") void session.cleanUp();
    else if (what === "next" || what === "previous") session.step(what === "next");
  }

  /** Add Comment…: the box beside the end of the selection. */
  function comment() {
    const view = editor?.view();
    if (!view) return;
    if (!session.me) return say("Save the document first, to comment on it");
    const { from, to } = view.state.selection.main;
    if (from === to) return say("Select the text to comment on");
    const at = view.coordsAtPos(to) ?? view.coordsAtPos(from);
    commenting = { from, to, x: at?.left ?? 40, y: (at?.bottom ?? 40) + 4 };
  }

  async function post(body: string) {
    const c = commenting;
    commenting = null;
    editor?.focus();
    if (c) await session.comment(c.from, c.to, body).catch(() => {});
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

  // the review begins again for each file, and the editor is told its items and the mode
  $effect(() => {
    if (path !== undefined) untrack(() => session.start(!!path));
  });
  $effect(() => editor?.setReview(session.items));
  $effect(() => editor?.setMode(session.mode));
  $effect(() => editor?.setView(session.view));
  // a document to review opens suggesting, once (the user may switch to editing)
  let suggested = "";
  $effect(() => {
    const key = reviewing && session.me ? `${path}` : "";
    if (key && key !== suggested) {
      suggested = key;
      untrack(() => session.mode !== "suggest" && session.toggleMode());
    }
  });

  // The settings (the Settings window): the font and the editor's prefs, the symbols typed, the user's colour in the
  // review (and who the user is, if that changed), whether the preview follows the cursor.
  let prefs = $state<EditorPrefs>(PREFS);
  let symbols = $state<[string, string][]>([]);
  let colour = $state<number | null>(null);
  let follows = true;
  /** the system's spelling on the prose (Settings ▸ Editor) */
  let spelling = $state(true);
  let who = "";
  /** how often the colours were set: the text and the lists are drawn again with them */
  let coloured = $state(0);

  function apply(s: Settings) {
    prefs = applyEditor(s);
    spelling = s.spelling;
    symbols = s.symbols;
    colour = s.review_colour;
    follows = s.preview_follow;
    const now = `${s.review_name}\n${s.review_email}`;
    if (who && who !== now) session.identity();
    who = now;
    if (follows) follow();
  }

  const userColour = $derived.by(() => {
    void coloured;
    return colorOf(session.me?.slug ?? "");
  });

  $effect(() => {
    setOwnColour(session.me?.slug, colour);
    untrack(() => {
      coloured++;
      session.reload();
    });
  });

  onMount(() => {
    const stop = watchSettings(apply);
    ask();
    askProject();
    window.addEventListener("focus", loadBar);
    const unlisten = [
      listen("opened", ask),
      listen("file-changed", () => {
        editor?.changedOnDisk();
        session.reload(300);
        gitbar?.refresh(1000);
        loadBarLater(2000);
      }),
      listen("projects-changed", loadBar),
      listen("asking-project", askProject),
      listen("project-info", projectInfo),
      listen<string>("review-menu", (e) => reviewMenu(e.payload)),
      listen("review-changed", () => {
        session.reload(100);
        gitbar?.refresh(500);
        loadBarLater(1000);
      }),
      listen<string>("menu", (e) => (e.payload === "save" ? save() : builds?.run(e.payload))),
      listen<string>("edit", (e) => edit(e.payload)),
      listen<string>("git-menu", (e) => git?.run(e.payload)),
      listen<string>("exchange", (e) => exchange?.run(e.payload)),
      listen("preview", togglePreview),
      listen("outline", toggleOutline),
      listen("cite-key", () => listing && refs?.cite("pandoc")),
      listen("references-changed", () => version++),
    ];
    return () => {
      stop();
      window.removeEventListener("focus", loadBar);
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });
</script>

<!-- the project's colour, at the top, to tell its windows apart among others -->
{#if barColour}<div class="colour" style:background={barColour}></div>{/if}
<div class="window" class:coloured={barColour}>
{#if bar.current && bar.tiles.length}
  <ProjectBar items={bar.tiles} onswitch={switchProject} onmenu={tileMenu} onadd={() => invoke("choose_project")} addTitle="Open Project… (⇧⌘O)" />
{/if}
<main style:--bar={reviewing ? "28px" : "0px"}>
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
        mono={!markdown && !latex}
        {latex}
        {prefs}
        {symbols}
        {completion}
        {hover}
        {diagnostics}
        spellcheck={spelling}
        outline={outlining && (markdown || latex)}
        autofocus
        extensions={[cursor]}
        review={session.handlers}
        onchange={changed}
        bind:error
      />
    {/key}
  {/if}
  {#if session.mode === "suggest"}
    <button class="mode" style:--user={userColour} title="Back to editing (⇧⌘E)" onclick={() => session.toggleMode()}>Suggesting</button>
  {/if}
  </div>
  {#if previewing && markdown}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="split" onpointerdown={drag}></div>
    <div class="pane" style:width={width ? `${width}px` : "45%"}><Preview {html} error={said} {block} /></div>
  {/if}
  {#if panel}
    <div class="pane review"><ReviewPanel {session} offer={offered} oncommit={() => git?.run("git-commit", offered)} ondismiss={() => (offered = "")} /></div>
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
  <GitBar bind:this={gitbar} {path} bind:state={repo} onopen={() => git?.run("git-commit", offered)} />
  <Git
    bind:this={git}
    flush={() => editor?.flush()}
    text={current}
    docName={() => (path ? nameOf(path) : "")}
    me={() => session.me?.name}
    say={(what) => say(what)}
    onchange={() => gitbar?.refresh(0)}
    onfocus={() => editor?.focus()}
    oncommitted={() => (offered = "")}
  />
  <Build bind:this={builds} flush={() => editor?.flush()} onfocus={() => editor?.focus()} ongoto={(line) => editor?.goToLine(line)} />
  {#if commenting}<CommentBox x={commenting.x} y={commenting.y} onpost={post} oncancel={() => ((commenting = null), editor?.focus())} />{/if}
  <Exchange bind:this={exchange} bind:reviewing mode={session.mode} flush={() => editor?.flush()} onfocus={() => editor?.focus()} />
  {#if error || problem || session.problem}<p class="error">{problem || error || session.problem}</p>{:else if note}<p class="note">{note}</p>{/if}
</main>
</div>
{#if asking}
  {#key asking}<ProjectCard root={asking} onclose={closeCard} />{/key}
{:else if carding}
  {#key carding}<ProjectCard root={carding} onclose={closeCard} />{/key}
{/if}

<style>
  /* absolute, not fixed: WebKit tints the title bar with what is fixed at the top of the page */
  .colour {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: 50;
    height: 6px;
  }
  .window {
    display: flex;
    height: 100vh;
    box-sizing: border-box;
    /* the tiles' detail: words and open suggestions, on two lines */
    --tile-detail-size: 10.5px;
  }
  .window.coloured {
    padding-top: 6px;
  }
  main {
    flex: 1;
    min-width: 0;
    height: 100%;
    position: relative;
  }
  .panes {
    display: flex;
    height: calc(100% - var(--bar, 0px));
    margin-top: var(--bar, 0px);
  }
  .text {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 100%;
  }
  .mode {
    position: absolute;
    z-index: 5;
    top: 8px;
    right: 18px;
    font: inherit;
    font-size: 11.5px;
    color: #fff;
    background: var(--user);
    border: none;
    border-radius: 10px;
    padding: 1px 9px;
    opacity: 0.92;
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
  .review {
    width: 320px;
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
