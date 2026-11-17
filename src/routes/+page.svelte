<script lang="ts">
  // The page of a document window: the file of the window in the editor, filling it, or untitled text kept in the
  // window until it is saved. Markdown for .md, .markdown, .qmd and .txt, LaTeX for .tex, .sty and .cls, monospace for the others. A file written as
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
  // The foot of the window says the build (Build), what was just done or went wrong,
  // and the state of the document's repository (GitBar; a click opens the Commit sheet). The Git
  // menu (`git-menu`) is Git: Commit…, Pull, Push, Review Branch…, Review Commits…. The Commit sheet is the one way to
  // commit; its message is prefilled from the window's log (`log`, commitLog.ts) of what it did since the last commit:
  // suggestions accepted or rejected, the review cleaned up, edits brought in as suggestions. The log is emptied by a
  // commit, and when the window shows another file.
  //
  // File ▸ Exchange (`exchange`) is Exchange: Compare as Suggestions… and the Word round trip.
  //
  // The project bar, the sidebar and the strip of tabs around the text are those of every tab (Frame).
  //
  // View ▸ Show Preview (⇧⌘P, `preview`) toggles the pane at the right of a Markdown document: pandoc's HTML of the text
  // as typed (`preview`), asked 300 ms after the typing stops, and scrolled to the block the cursor is in.
  // Edit ▸ Find ▸ Find in Project… (⇧⌘F) shows the sidebar on its search (Frame). A match opened comes as `go-to` (or
  // `pending_go_to` for a window made for it), Replace All of the search as `replace-all` to the windows of the files
  // it changes.

  import { invoke } from "@tauri-apps/api/core";
  import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  import { onMount, untrack } from "svelte";
  import { EditorView } from "@codemirror/view";
  import { ensureSyntaxTree } from "@codemirror/language";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { part, ready } from "papers-write/project";
  import Preview from "$lib/Preview.svelte";
  import CommentBox from "$lib/CommentBox.svelte";
  import ReviewPanel from "$lib/ReviewPanel.svelte";
  import CriticPanel from "$lib/CriticPanel.svelte";
  import { ReviewSession } from "$lib/review.svelte";
  import { colorOf } from "$lib/reviewFormat";
  import { message, type Entry } from "$lib/commitLog";
  import { openPath } from "@tauri-apps/plugin-opener";
  import {
    FileEditor,
    PREFS,
    RefList,
    citation,
    insertSymbol,
    replaceText,
    setColours,
    type EditorPrefs,
    type Item,
    type Completion,
    type Diagnostics,
    type Spelling,
    type Found,
    type ReviewView,
    type HoverSources,
    type FindBar,
    type FindCommand,
    type Heading,
    type ProjectQuery,
    type Format,
    type FileRead,
    type Status,
    COLUMN_WIDTH_EVENT,
    type Mark,
  } from "papers-write/editor";
  import Build from "$lib/Build.svelte";
  import Exchange from "$lib/Exchange.svelte";
  import Git from "$lib/Git.svelte";
  import GitBar from "$lib/GitBar.svelte";
  import Toolbar from "$lib/Toolbar.svelte";
  import Frame from "$lib/Frame.svelte";
  import { figureImage } from "$lib/pdfImage";
  import SidePdf from "$lib/SidePdf.svelte";
  import { phrasesAt, plainOf } from "$lib/pdfSearch";
  import type { GitState } from "$lib/git";
  import { listen } from "$lib/events";
  import { applyEditor, watchSettings, type Appearance, type Settings } from "$lib/settings";
  import "$lib/theme.css";

  // The toolbar above the text (Toolbar): the formats (also the Format menu, `format`: ⌘B, ⌘I, ⌘K), Add Comment…, Find and Find and
  // Replace, the review panel and the mode.
  //
  // The review (Review menu, `review-menu`): suggesting or editing (ModeControl, at the right of the toolbar), the
  // suggestions and comments of the review files carried onto the text as typed (`review_items`, asked again 500 ms
  // after the typing stops, after each write of the user's, on `review-changed` and when the file changes on disk),
  // given to the editor with `setReview`. Review ▸ Show Review Panel (⌥⌘R) shows them in a column at the right
  // (ReviewPanel), in place of the preview: the two do not show together. Add Comment… (⌥⌘M) opens a box at the selection; ⌘↵ posts.

  /** the file of the window; none while untitled; undefined until the backend says */
  let path = $state<string | null | undefined>();
  let editor = $state<FileEditor>();
  /** what the editor's find bar shows (the toolbar's Find buttons) */
  let findbar = $state<FindBar>(null);
  let builds = $state<Build>();
  let exchange = $state<Exchange>();
  let gitbar = $state<GitBar>();
  /** the repository of the document (git.rs); none when it is not in one */
  let repo = $state<GitState | null>(null);
  let git = $state<Git>();
  // a commit made elsewhere (a terminal) moves HEAD: the log of what the window did is of the commit before it. What the
  // app's own git steps do (a commit, a pull, a checkout) is not that, for the few seconds the state takes to follow.
  let ownStep = false;
  let ownTimer: ReturnType<typeof setTimeout> | undefined;
  function ours() {
    ownStep = true;
    clearTimeout(ownTimer);
    ownTimer = setTimeout(() => (ownStep = false), 6000);
  }
  let head: string | undefined;
  $effect(() => {
    const now = repo?.head;
    untrack(() => {
      if (head && now && now !== head && !ownStep) log = [];
      head = now;
    });
  });
  /** what the window did since the last commit, for the message of the next one */
  let log = $state<Entry[]>([]);
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
  /** the PDF beside the text rather than the preview (View ▸ Show PDF), and its pane */
  let pdfBeside = $state(false);
  let sidePdf = $state<SidePdf>();
  let html = $state("");
  let said = $state("");
  let block = $state(0);
  // the pane's width as a share of the window's (not pixels: zoomed, the pane would take the text's room)
  const widthKey = `pane-share:${getCurrentWebview().label}`;
  const stored = (() => {
    try {
      return Number(localStorage.getItem(widthKey));
    } catch {
      return 0;
    }
  })();
  let width = $state(stored > 0.1 && stored < 0.9 ? stored : 0);
  let panes = $state<HTMLElement>();
  /** the project bar and the sidebar around the text (Frame) */
  let frame = $state<Frame>();
  /** the headings of the document and the one the cursor is under, for the outline */
  let headings = $state.raw<Heading[]>([]);
  let here = $state(-1);
  const session = new ReviewSession({ text: () => current(), editor: () => editor, say: (what) => say(what), log: (e) => log.push(e), colours: (m) => setColours(withOwn(m)) });
  /** the review panel at the right: shown (the preview is not while it is) */
  let panel = $state(false);
  /** where the document's suggestions are kept, as its project says: the review's notes, or the text (CriticMarkup) */
  let store = $state<"notes" | "in-file">("notes");
  const inText = $derived(store === "in-file");
  /** the CriticMarkup marks of the text (in-file) */
  let marks = $state.raw<Mark[]>([]);
  /** the user's initials, after the marks they write (Settings ▸ Review) */
  let initials = $state("");

  /** Where the suggestions are kept, asked again (the document read, its project changed). */
  async function askStore() {
    store = await invoke<"notes" | "in-file">("review_store").catch(() => "notes" as const);
  }
  /** the comment being written: the selection it is on and where the box goes */
  let commenting = $state<{ from: number; to: number; x: number; y: number } | null>(null);
  const markdown = $derived(!path || /\.(md|markdown|qmd|txt)$/i.test(path));
  /** a LaTeX file: highlighted and completed, with no symbols typed */
  const latex = $derived(!!path && /\.(tex|sty|cls)$/i.test(path));

  async function ask() {
    log = [];
    path = (await invoke<{ path: string | null }>("document")).path;
    askStore();
    takeReceived();
    // a match of the project's search opened this document: selected once it is read
    const going = await invoke<{ at: number; end: number; mark: boolean } | null>("pending_go_to").catch(() => null);
    if (going) queueMicrotask(() => goTo(going));
  }

  /** A place to go to: a match selected, or a place found from a PDF marked (the cursor at its start). */
  function goTo(go: { at: number; end: number; mark: boolean }) {
    if (go.mark) editor?.markFound(go.at, go.end);
    else editor?.select(go.at, go.end);
  }

  /** the reviews sent back and joined to this document (the backend says them, a window made for it is not listening yet) */
  async function takeReceived() {
    try {
      for (const did of await invoke<string[]>("review_received")) log.push({ did });
    } catch {}
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
      pdfBeside = false;
      follow();
      refresh();
    }
  }

  /** View ▸ Show PDF: the PDF beside the text, in the preview's place. */
  function togglePdf() {
    if (!path) return say("Save the document first, to show its PDF");
    pdfBeside = !pdfBeside;
    if (pdfBeside) [previewing, panel] = [false, false];
  }

  let previewPane = $state<Preview>();

  /** ⌃-click in the text, or Show in … of its menu: where the cursor is, shown in the pane beside it: the preview's
   *  block when the preview is shown, else in the PDF (shown first when it is not). */
  function syncHere() {
    if (previewing && markdown) {
      follow();
      queueMicrotask(() => previewPane?.flash());
    } else void showInPdf();
  }

  /** A right-click in the text: Show in Preview or in PDF, and the edits of the selection. */
  async function editorMenu(e: MouseEvent) {
    const where = previewing && markdown ? "Preview" : "PDF";
    const mod = navigator.userAgent.includes("Mac") ? "⌘" : "Ctrl";
    const items = await Promise.all([
      MenuItem.new({ text: `Show in ${where} (${mod}-click)`, enabled: !!path, action: syncHere }),
      PredefinedMenuItem.new({ item: "Separator" }),
      PredefinedMenuItem.new({ item: "Cut" }),
      PredefinedMenuItem.new({ item: "Copy" }),
      PredefinedMenuItem.new({ item: "Paste" }),
      PredefinedMenuItem.new({ item: "SelectAll" }),
    ]);
    await (await Menu.new({ items })).popup(new LogicalPosition(e.clientX, e.clientY));
  }

  /** ⌃-click in the preview: the words of its block looked for in the text, the word clicked marked there. */
  async function previewToSource(text: string, word: string, around: { before: string[]; after: string[] }) {
    const place = await invoke<[number, number] | null>("place_in_text", { text: current(), query: text, word, around }).catch(() => null);
    if (place) editor?.markFound(place[0], place[1]);
    else say("Not found in the text");
  }

  // View ▸ Show Preview and Show PDF say Hide while shown: told as they change, and as the window comes to the front
  $effect(() => {
    const shown = { preview: previewing && markdown, pdf: pdfBeside };
    const tell = () => invoke("view_shown", shown).catch(() => {});
    tell();
    window.addEventListener("focus", tell);
    return () => window.removeEventListener("focus", tell);
  });

  /** Show in PDF: the place of the cursor in the PDF beside the text (shown first if it is not): exactly from the
   *  map the build left beside the PDF, else by the words around the cursor, the longest phrase of them first. */
  async function showInPdf() {
    try {
      await showInPdfNow();
    } catch (e) {
      say(`Show in PDF: ${e}`);
      sidePdf?.tell(`Show in PDF: ${e}`);
    }
  }

  async function showInPdfNow() {
    const view = editor?.view();
    if (!view || !path) return;
    if (!pdfBeside) {
      togglePdf();
      // the pane and its PDF read first
      await new Promise((r) => setTimeout(r, 800));
    }
    const head = view.state.selection.main.head;
    const line = view.state.doc.lineAt(head);
    // exactly, from the map the build left beside the PDF; else by the words around the cursor
    // the word selected, else the word at the cursor
    const { from, to } = view.state.selection.main;
    const selected = view.state.sliceDoc(from, to).trim();
    const range = view.state.wordAt(head);
    const word = selected && !/\s/.test(selected) ? selected : range ? view.state.sliceDoc(range.from, range.to) : "";
    const along = line.length ? (head - line.from) / line.length : 0;
    // the words around it, which pick its place among the word's places in the PDF
    const words = (t: string) => plainOf(t).toLowerCase().split(/[^\p{L}\p{N}]+/u).filter((w) => w.length > 1);
    const before = words(line.text.slice(0, Math.min(from, head) - line.from)).slice(-6);
    const after = words(line.text.slice(Math.max(to, head) - line.from)).slice(0, 6);
    const page = await sidePdf?.showSource(path, line.number, word, along, before, after);
    if (page) return say(`Shown in the PDF: page ${page}`);
    for (const phrase of phrasesAt(line.text, head - line.from)) {
      if (await sidePdf?.find(phrase)) {
        sidePdf?.tell(`found by its words: “${phrase}”`);
        return say("Found in the PDF by its words (no map from the build beside it)");
      }
    }
    sidePdf?.tell("not found");
    say(sidePdf?.showing() ? "Not found in the PDF" : "Choose the PDF to show beside the text");
  }

  /** ⌃-click in the PDF beside the text: its source there (this text, or another file in its tab). */
  async function toSource(pdf: string, text: string, at: { page: number; x: number; y: number } | null, word = "", around = { before: [] as string[], after: [] as string[] }) {
    const found = await invoke<boolean>("open_source_at", { pdf, text, at, word, around }).catch(() => false);
    if (!found) say("Not found in the project's sources");
  }

  /** The split dragged: the width of the preview, kept for the window. */
  function drag(e: PointerEvent) {
    const bar = e.currentTarget as HTMLElement;
    bar.setPointerCapture(e.pointerId);
    const move = (m: PointerEvent) => {
      const total = panes?.clientWidth ?? 0;
      if (!total) return;
      const px = Math.min(Math.max(panes!.getBoundingClientRect().right - m.clientX, 240), total - 240);
      width = Math.round((px / total) * 1000) / 1000;
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
  const leftOut = () => invoke<string[]>("references_left_out", { text: current() });

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
    // none in a tooltip while the figures are in the text
    image: (path) => (inlineFigures ? Promise.resolve(null) : figureImage(path)),
  };

  /** The prose checked by the system's spell checker (the app's; the editor draws the misses), when Settings ▸ Editor ▸ Check spelling is on. */
  const speller: Spelling = {
    check: (texts, language) => invoke("check_spelling", { texts, language }),
    guesses: (word, language) => invoke("spelling_guesses", { word, language }),
    learn: (word) => invoke("learn_word", { word }),
    ignore: (word) => invoke("ignore_word", { word }),
  };

  /** The keys underlined: the citations neither the document's bibliography nor the library has (labels the editor sees itself). */
  const diagnostics: Diagnostics = {
    missingKeys: async (keys) => new Set(await invoke<string[]>("missing_keys", { keys, text: current() })),
    labels: (text) => invoke<string[]>("document_labels", { text }),
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
    if (panel) [previewing, pdfBeside] = [false, false];
  }

  /** A choice of the Review menu. */
  function reviewMenu(what: string) {
    if (what === "suggest") session.toggleMode();
    else if (what.startsWith("view-")) session.setView(what.slice(5) as ReviewView);
    else if (what === "panel") togglePanel();
    else if (what === "comment") comment();
    else if (what === "accept-here" || what === "reject-here") changeHere(what === "accept-here" ? "accept" : "reject");
    else if (inText && (what === "accept-all" || what === "reject-all")) void allMarks(what === "accept-all");
    else if (inText && (what === "next" || what === "previous")) stepMarks(what === "next");
    else if (inText && what.startsWith("view-")) say("Accepted and Original show the review's notes; the marks in the text are shown as they are");
    else if (what === "accept-all") session.acceptAll().catch(() => {});
    else if (what === "reject-all") session.rejectAll().catch(() => {});
    else if (what === "cleanup") void session.cleanUp();
    else if (what === "next" || what === "previous") session.step(what === "next");
  }

  /** Review ▸ Accept Change and Reject Change: the suggestion at the cursor (the first, where several meet). */
  function changeHere(action: "accept" | "reject") {
    if (inText) {
      const mark = editor?.marksAtCursor().find((m) => m.kind !== "comment" && m.kind !== "highlight");
      if (!mark) return say("No suggestion at the cursor");
      return editor?.decideMarkAt(mark, action === "accept");
    }
    const here = editor?.itemsHere().find((i) => i.kind === "suggestion" && i.id && session.items.some((s) => s.id === i.id && s.state === "open"));
    if (!here) return say("No suggestion at the cursor");
    session.act(here.id, action).catch(() => {});
  }

  /** Accept All and Reject All of the marks in the text, asked first. */
  async function allMarks(accept: boolean) {
    const n = marks.filter((m) => m.kind !== "comment" && m.kind !== "highlight").length;
    if (!n) return say("No suggestions in the text");
    const verb = accept ? "Accept" : "Reject";
    const ok = await invoke<boolean>("review_confirm", { message: `${verb} the ${n} suggestion${n === 1 ? "" : "s"} in the text?`, ok: `${verb} All` }).catch(() => false);
    if (ok) editor?.decideAllMarks(accept);
  }

  /** Next Change and Previous Change among the marks in the text. */
  function stepMarks(forward: boolean) {
    if (!marks.length) return say("No changes or comments to go to");
    const head = editor?.view()?.state.selection.main.head ?? 0;
    const mark = forward ? (marks.find((m) => m.from > head) ?? marks[0]) : (marks.findLast((m) => m.to < head) ?? marks[marks.length - 1]);
    editor?.select(mark.from, mark.to);
  }

  /** Add Comment…: the box beside the end of the selection. */
  function comment() {
    const view = editor?.view();
    if (!view) return;
    if (!session.me && !inText) return say("Save the document first, to comment on it");
    const { from, to } = view.state.selection.main;
    if (from === to) return say("Select the text to comment on");
    const at = view.coordsAtPos(to) ?? view.coordsAtPos(from);
    commenting = { from, to, x: at?.left ?? 40, y: (at?.bottom ?? 40) + 4 };
  }

  async function post(body: string) {
    const c = commenting;
    commenting = null;
    editor?.focus();
    if (c && inText) editor?.commentInText(c.from, c.to, body);
    else if (c) await session.comment(c.from, c.to, body).catch(() => {});
  }

  function edit(what: string) {
    if (!editor) return;
    if (what === "check") check();
    else if (what === "references") listing = true;
    else if (what === "symbols") editor.list();
    else (what === "redo" ? editor.redo : editor.undo)();
  }

  /** Edit ▸ Find: in the text (the editor's bar), or in the project (the sidebar's search). */
  function findCommand(what: string) {
    if (what === "find-project") {
      return frame?.findInProject();
    }
    // the PDF beside the text in front: its own find (it cannot be replaced in)
    if (pdfFront && pdfBeside && sidePdf && what !== "find-replace") {
      if (what === "find") return sidePdf.focusFind();
      if (what === "find-next" || what === "find-previous") return void sidePdf.findAgain(what === "find-previous");
      if (what === "find-selection") return sidePdf.findSelection();
    }
    editor?.find(what === "find-replace" ? "replace" : (what.replace(/^find-?/, "") || "find") as FindCommand);
  }

  /** Whether the PDF beside the text had the keys last (a click in it, its find field), rather than the text: Find,
   *  Find Next and Find Previous are then its own. */
  let pdfFront = $state(false);
  $effect(() => {
    const focused = (e: FocusEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest?.(".side-pdf")) pdfFront = true;
      else if (target?.closest?.(".editor, .toolbar")) pdfFront = false;
    };
    // F3 and ⇧F3 in the PDF, as Linux and Windows have them beside the menu's ⌘G and ⇧⌘G (the text's search has them)
    const f3 = (e: KeyboardEvent) => {
      if (e.key !== "F3" || e.ctrlKey || e.metaKey || e.altKey || !pdfFront) return;
      e.preventDefault();
      findCommand(e.shiftKey ? "find-previous" : "find-next");
    };
    document.addEventListener("focusin", focused);
    document.addEventListener("keydown", f3, true);
    return () => {
      document.removeEventListener("focusin", focused);
      document.removeEventListener("keydown", f3, true);
    };
  });

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
  /** the file written as it is typed; off: only when saved, a draft kept meanwhile (Settings ▸ Editor) */
  let autosave = $state(true);
  /** light, dark or as the system is (Settings, the toolbar) */
  let appearance = $state<Appearance>("system");
  /** the figures in the text rather than in tooltips (Settings ▸ Editor, View) */
  let inlineFigures = $state(false);
  let who = "";
  /** how often the colours were set: the text and the lists are drawn again with them */
  let coloured = $state(0);

  function apply(s: Settings) {
    prefs = applyEditor(s);
    spelling = s.spelling;
    autosave = s.autosave;
    appearance = s.appearance;
    inlineFigures = s.inline_figures;
    symbols = s.symbols;
    colour = s.review_colour;
    initials = s.review_initials;
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

  /** the people's colours with the user's own as the settings have it (their file's header follows a moment later) */
  function withOwn(m: Record<string, number>): Record<string, number | null> {
    const slug = session.me?.slug;
    return slug && colour ? { ...m, [slug]: colour } : m;
  }

  $effect(() => {
    void colour;
    untrack(() => {
      setColours(withOwn(session.colours));
      coloured++;
      session.reload();
    });
  });

  /** the text read the first time, for the tab to be shown (`ready`) */
  const textIn = part();

  /** whether the file in the editor is as it is typed, and whether the backend was told it is not (the star of the
   *  tab and window) */
  let status = $state<Status>("saved");
  let modified = false;
  $effect(() => {
    const now = !!path && status === "editing";
    if (now !== modified) invoke("set_modified", { modified: (modified = now) });
  });

  /** The file read, with the draft of it typed and not saved, if any. */
  async function readFile(file: string): Promise<FileRead> {
    const [text, draft] = await Promise.all([
      invoke<string>("read_text", { path: file }),
      invoke<FileRead["draft"]>("draft_read", { path: file }).catch(() => null),
    ]);
    return { path: file, text, draft };
  }

  const drafts = {
    write: (p: string, text: string, base: string) => invoke<void>("draft_write", { path: p, text, base }),
    drop: (p: string) => invoke<void>("draft_drop", { path: p }),
  };

  /** Close ▸ Save: the file written, then its tab closed (unless the writing failed, which the editor says). */
  async function saveAndClose() {
    await editor?.flush();
    if (status !== "saved") return;
    await invoke("set_modified", { modified: (modified = false) });
    await invoke("close_tab", { label: getCurrentWebview().label });
  }

  onMount(() => {
    const stop = watchSettings(apply);
    ready(ask(), textIn.promise, frame?.loaded() ?? Promise.resolve());
    const unlisten = [
      listen("opened", ask),
      listen("save-and-close", saveAndClose),
      // the project's write.toml changed (Project Info): the suggestions may be kept elsewhere now
      listen("projects-changed", askStore),
      listen("file-changed", () => {
        editor?.changedOnDisk();
        session.reload(300);
        gitbar?.refresh(1000);
        frame?.loadBarLater(2000);
      }),
      listen<string>("review-menu", (e) => reviewMenu(e.payload)),
      listen("review-received", takeReceived),
      listen("review-changed", () => {
        session.reload(100);
        gitbar?.refresh(500);
        frame?.loadBarLater(1000);
      }),
      listen<string>("menu", (e) => (e.payload === "save" ? save() : builds?.run(e.payload))),
      listen<string>("edit", (e) => edit(e.payload)),
      listen<string>("git-menu", (e) => git?.run(e.payload)),
      listen<string>("exchange", (e) => exchange?.run(e.payload)),
      listen("preview", togglePreview),
      listen("pdf-beside", togglePdf),
      listen<{ query: ProjectQuery; with: string }>("replace-all", (e) => editor?.replaceAll(e.payload.query, e.payload.with)),
      listen<{ at: number; end: number; mark: boolean }>("go-to", (e) => goTo(e.payload)),
      listen("cite-key", () => listing && refs?.cite("pandoc")),
      listen<string>("find", (e) => findCommand(e.payload)),
      listen<Format>("format", (e) => editor?.format(e.payload)),
      // Go ▸ Back and Forward have ⌘[ and ⌘]: in the text, the lines indented less and more
      listen<string>("go", (e) => {
        if (!editor?.hasFocus()) return;
        if (e.payload === "back") editor.outdent();
        else if (e.payload === "forward") editor.indent();
      }),
      listen("references-changed", () => version++),
    ];
    return () => {
      stop();
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });
</script>

<Frame
  bind:this={frame}
  {path}
  {headings}
  {here}
  outlined={markdown || latex}
  offset={reviewing ? 28 : 0}
  onheading={(h) => editor?.goToHeading(h)}
  onsay={say}
  onproblem={(e) => (problem = e)}
  onfocus={() => editor?.focus()}
>
  <div class="body">
  <!-- the column of text dragged at its edges: its width kept (COLUMN_WIDTH_EVENT) -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="panes" bind:this={panes} {@attach (el) => {
    const kept = (e: Event) => invoke("set_text_width", { width: (e as CustomEvent<number>).detail });
    el.addEventListener(COLUMN_WIDTH_EVENT, kept);
    return () => el.removeEventListener(COLUMN_WIDTH_EVENT, kept);
  }}>
  <!-- the text's column: its toolbar above it, so that its tools stay with it when a pane is beside it -->
  <div class="text">
  <Toolbar
    kind={markdown ? "markdown" : latex ? "latex" : "text"}
    mode={session.mode}
    view={session.view}
    open={inText ? marks.length : session.items.filter((item) => item.state === "open").length}
    where={inText ? "in the text" : "in notes"}
    {store}
    onstore={(s) => invoke("set_review_store", { store: s }).then(askStore).catch((e) => say(String(e)))}
    colour={userColour}
    {panel}
    reviewing={!!session.me}
    onformat={(what) => editor?.format(what)}
    oncomment={comment}
    {findbar}
    onfind={(what) => editor?.find(what)}
    onpanel={togglePanel}
    onmode={(m) => m !== session.mode && session.toggleMode()}
    onview={(v) => session.setView(v)}
    width={prefs.readable ? prefs.width : null}
    onwidth={(w) => invoke("set_text_width", { width: w })}
    {appearance}
    onsize={(by) => invoke("zoom_window", { by })}
    onappearance={(a) => invoke("set_appearance", { appearance: a })}
  />
  <div class="editor">
  {#if path !== undefined}
    {#key path}
      {@const file = path}
      <!-- the text typed is blank until said otherwise: a window is made blank, and one opened over has no text -->
      <FileEditor
        bind:this={editor}
        load={file
          ? async () => {
              try {
                return await readFile(file);
              } finally {
                textIn.done();
              }
            }
          : async () => (textIn.done(), { path: "", text: "" })}
        save={file ? (p, text) => invoke("write_text", { path: p, text }) : async () => {}}
        autosave={autosave || !file}
        drafts={file ? drafts : undefined}
        mono={!markdown && !latex}
        {latex}
        {prefs}
        figures={inlineFigures && (markdown || latex) ? figureImage : undefined}
        onsync={syncHere}
        oncontext={editorMenu}
        {symbols}
        {completion}
        {hover}
        {diagnostics}
        spelling={spelling ? speller : undefined}
        onoutline={markdown || latex ? (h, i) => ([headings, here] = [h, i]) : undefined}
        autofocus
        extensions={[cursor]}
        review={inText ? undefined : session.handlers}
        critic={inText ? { initials } : undefined}
        onmarks={(m) => (marks = m)}
        onchange={changed}
        bind:error
        bind:findbar
        bind:status
      />
    {/key}
  {/if}
  </div>
  </div>
  {#if previewing && markdown}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="split" onpointerdown={drag}></div>
    <div class="pane" style:width={width ? `${width * 100}%` : "45%"}>
      <Preview bind:this={previewPane} {html} error={said} {block} onsource={previewToSource} />
      <button class="close-pane" onclick={togglePreview} title="Hide Preview" aria-label="Hide Preview">×</button>
    </div>
  {:else if pdfBeside && path}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="split" onpointerdown={drag}></div>
    <div class="pane" style:width={width ? `${width * 100}%` : "45%"}>
      <SidePdf bind:this={sidePdf} onsource={toSource} onsay={say} />
      <button class="close-pane" onclick={togglePdf} title="Hide PDF" aria-label="Hide PDF">×</button>
    </div>
  {/if}
  {#if panel}
    <div class="pane review">
      {#if inText}
        <CriticPanel
          {marks}
          text={current}
          ondecide={(m, accept) => editor?.decideMarkAt(m, accept)}
          onall={(accept) => void allMarks(accept)}
          onshow={(m) => editor?.select(m.from, m.to)}
        />
      {:else}
        <ReviewPanel {session} />
      {/if}
    </div>
  {/if}
  </div>
  {#if listing}
    {#key version}
      <RefList bind:this={refs} enter="pandoc" {search} {closest} {sources} dropped={leftOut} oninsert={inserted} onopen={openPdf} onclose={closeList} />
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
  <Git
    bind:this={git}
    flush={() => editor?.flush()}
    text={current}
    docName={() => (path ? nameOf(path) : "")}
    me={() => session.me?.name}
    say={(what) => say(what)}
    message={() => message(log)}
    onchange={() => (ours(), gitbar?.refresh(0))}
    onfocus={() => editor?.focus()}
    oncommitted={() => (log = [])}
    onlog={(did) => log.push({ did })}
  />
  {#if commenting}<CommentBox x={commenting.x} y={commenting.y} onpost={post} oncancel={() => ((commenting = null), editor?.focus())} />{/if}
  <Exchange
    bind:this={exchange}
    bind:reviewing
    mode={session.mode}
    flush={() => editor?.flush()}
    onfocus={() => editor?.focus()}
    onlog={(did) => log.push({ did })}
  />
  <!-- the foot of the window: the build, what was just done or went wrong, the repository -->
  <footer>
    <Build bind:this={builds} can={markdown || latex} flush={() => editor?.flush()} onfocus={() => editor?.focus()} ongoto={(line) => editor?.goToLine(line)} />
    {#if error || problem || session.problem}
      {@const said = problem || error || session.problem}
      <span class="said error" title={said}>{said}</span>
    {:else if note}<span class="said" title={note}>{note}</span>{:else}<span class="said"></span>{/if}
    <GitBar bind:this={gitbar} {path} bind:state={repo} onopen={() => git?.run("git-commit")} />
  </footer>
  </div>
</Frame>

<style>
  /* not a box: the panes and the foot are the frame's; what is placed absolutely is placed in its content */
  .body {
    display: contents;
    --foot: 26px;
  }
  .panes {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  footer {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    height: var(--foot);
    box-sizing: border-box;
    padding: 0 8px;
    border-top: 1px solid var(--line);
    background: var(--sidebar);
  }
  .said {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
    color: var(--muted);
  }
  .said.error {
    color: var(--error);
  }
  .text {
    flex: 1;
    min-width: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .editor {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .split {
    flex: none;
    width: 5px;
    cursor: col-resize;
    background: var(--line);
  }
  .pane {
    position: relative;
    flex: none;
    min-width: 240px;
    max-width: 80%;
  }
  /* the pane closed: its × at the top right */
  .close-pane {
    position: absolute;
    z-index: 4;
    top: 3px;
    right: 6px;
    width: 22px;
    height: 22px;
    padding: 0;
    font-size: 16px;
    line-height: 1;
    color: var(--muted);
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 5px;
    opacity: 0.85;
  }
  .close-pane:hover {
    color: var(--fg);
    opacity: 1;
  }
  .review {
    width: 320px;
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
    margin: 0;
    padding: 4px 0;
    color: var(--error);
  }
</style>
