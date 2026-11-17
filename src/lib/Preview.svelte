<script lang="ts">
  // The quick preview: the HTML pandoc made of the text, in a sandboxed frame (no scripts) with a stylesheet of its
  // own, like a preprint's page. `block` is the index of the top-level block the cursor is in; the frame is scrolled
  // to the same one (pandoc's Markdown reader has no source positions, so the blocks are matched by their order).
  // `error` is what pandoc said, in a bar at the foot.

  import { followButtons, popupReleased } from "$lib/contextMenu";
  import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  const keysOf = (k: string) => (navigator.userAgent.includes("Mac") ? k : "Ctrl");

  let {
    html,
    error = "",
    block = 0,
    onsource,
    onblock,
    onpdf,
  }: {
    html: string;
    error?: string;
    block?: number;
    /** ⌃-click (⌘ on macOS) on the preview, or Go to Source in its menu: the text of the block clicked, and the word */
    onsource?: (text: string, word: string, around: { before: string[]; after: string[] }) => void;
    /** a click in the preview (not a selection): the index of the top-level block clicked, for the text to scroll
     *  to it */
    onblock?: (index: number) => void;
    /** Show PDF, in the menu: the pane switched to the PDF */
    onpdf?: () => void;
  } = $props();

  let frame: HTMLIFrameElement;

  const SHEET = `
    :root { color-scheme: light dark; --fg: #1d1d1f; --bg: #fdfdfd; --muted: #6e6e73; --rule: #888; --code: rgba(0,0,0,.05); --link: #2f6fde; }
    @media (prefers-color-scheme: dark) { :root:not([data-theme=light]) { --fg: #e8e8ea; --bg: #1e1e20; --muted: #9a9aa0; --rule: #888; --code: rgba(255,255,255,.08); --link: #7ea6f5; } }
    :root[data-theme=dark] { color-scheme: dark; --fg: #e8e8ea; --bg: #1e1e20; --muted: #9a9aa0; --rule: #888; --code: rgba(255,255,255,.08); --link: #7ea6f5; }
    :root[data-theme=light] { color-scheme: light; }
    html { background: var(--bg); }
    body { box-sizing: border-box; max-width: 42em; margin: 0 auto; padding: 2em 1.5em 40vh; color: var(--fg);
      font: 16px/1.55 "Iowan Old Style", "Palatino", "Charter", Georgia, serif; hyphens: auto; overflow-wrap: break-word; }
    h1, h2, h3, h4 { line-height: 1.25; margin: 1.6em 0 .5em; }
    h1 { font-size: 1.6em; } h2 { font-size: 1.3em; } h3 { font-size: 1.1em; } h4 { font-size: 1em; font-style: italic; }
    p { margin: 0 0 .9em; text-align: justify; }
    a { color: var(--link); text-decoration: none; }
    code, pre { font: .85em ui-monospace, Menlo, monospace; background: var(--code); border-radius: 3px; }
    code { padding: 0 .25em; } pre { padding: .6em .8em; overflow-x: auto; } pre code { padding: 0; background: none; }
    blockquote { margin: 1em 0; padding-left: 1em; border-left: 3px solid var(--rule); color: var(--muted); }
    figure { margin: 1.4em 0; text-align: center; }
    img { max-width: 100%; height: auto; }
    figcaption, caption { font-size: .85em; color: var(--muted); margin-top: .4em; text-align: left; }
    math[display="block"] { display: block; margin: 1em 0; text-align: center; overflow-x: auto; }
    .eqn { position: relative; } .eqn p { margin: 0; text-align: center; }
    .eqno { position: absolute; right: 0; top: 50%; transform: translateY(-50%); }
    table { border-collapse: collapse; margin: 1.2em auto; font-size: .92em; border-top: 1.5px solid var(--fg); border-bottom: 1.5px solid var(--fg); }
    caption { caption-side: top; text-align: center; margin-bottom: .4em; }
    th { border-bottom: 1px solid var(--fg); } th, td { padding: .25em .7em; text-align: left; }
    hr { border: 0; border-top: 1px solid var(--rule); margin: 2em 0; }
    #refs { margin-top: 2em; font-size: .9em; }
    .csl-entry { padding-left: 2em; text-indent: -2em; margin: 0 0 .5em; text-align: left; }
    .footnotes { font-size: .85em; border-top: 1px solid var(--rule); margin-top: 2em; }
    .citation, .footnote-ref { white-space: nowrap; }
    .synced { background: rgba(255, 221, 0, 0.35); border-radius: 3px; transition: background 1.2s ease-out; }
  `;

  /** light or dark as the window is (`data-theme` of its root, Settings ▸ Appearance), else as the system is */
  let theme = $state(document.documentElement.dataset.theme ?? "");
  $effect(() => {
    const watch = new MutationObserver(() => (theme = document.documentElement.dataset.theme ?? ""));
    watch.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => watch.disconnect();
  });
  // No script of the document runs (its policy); the frame allows scripts only so that the page's own listeners on it
  // run (⌃-click and the menu to the source): WebKit runs no listener in a frame that has none.
  const POLICY = `<meta http-equiv="Content-Security-Policy" content="script-src 'none'; object-src 'none'">`;
  const doc = $derived(`<!doctype html><html${theme ? ` data-theme="${theme}"` : ""}><meta charset="utf-8">${POLICY}<style>${SHEET}</style><body>${html}</body></html>`);

  /** The frame scrolled so that the block is a little below its top. */
  function reveal() {
    const body = frame?.contentDocument?.body;
    if (!body) return;
    const el = body.children[Math.min(block, body.children.length - 1)] as HTMLElement | undefined;
    if (el) frame.contentWindow?.scrollTo({ top: Math.max(0, el.offsetTop - 48) });
  }

  $effect(() => {
    block;
    reveal();
  });

  /** The block the cursor is in brought into view and marked a moment (Show in Preview). */
  export function flash() {
    reveal();
    const body = frame?.contentDocument?.body;
    const el = body?.children[Math.min(block, body.children.length - 1)] as HTMLElement | undefined;
    if (!el) return;
    el.classList.add("synced");
    setTimeout(() => el.classList.remove("synced"), 1400);
  }

  /** What was clicked in the frame: the text of its block (a paragraph, an item, a heading, a caption) and the word. */
  function clicked(e: MouseEvent): { text: string; word: string; around: { before: string[]; after: string[] } } | null {
    const doc = frame?.contentDocument;
    const target = e.target as Element | null;
    const block = target?.closest?.("p, li, h1, h2, h3, h4, h5, h6, figcaption, td, th, blockquote, dd, dt");
    if (!doc || !block) return null;
    const range = doc.caretRangeFromPoint?.(e.clientX, e.clientY);
    const node = range?.startContainer;
    let word = "";
    const around = { before: [] as string[], after: [] as string[] };
    if (range && node?.nodeType === Node.TEXT_NODE) {
      const t = node.textContent ?? "";
      const isWord = (c: string) => /[\p{L}\p{N}_-]/u.test(c);
      let [a, b] = [range.startOffset, range.startOffset];
      while (a > 0 && isWord(t[a - 1])) a--;
      while (b < t.length && isWord(t[b])) b++;
      word = t.slice(a, b);
      // the words around it in its block, which pick its place in the text among the word's places
      const pre = doc.createRange();
      pre.setStart(block, 0);
      pre.setEnd(node, a);
      const post = doc.createRange();
      post.setStart(node, b);
      post.setEndAfter(block.lastChild ?? block);
      const words = (s: string) => s.toLowerCase().split(/[^\p{L}\p{N}]+/u).filter((w) => w.length > 1);
      around.before = words(pre.toString()).slice(-6);
      around.after = words(post.toString()).slice(0, 6);
    }
    return { text: (block.textContent ?? "").replace(/\s+/g, " ").trim(), word, around };
  }

  /** The frame loaded: ⌃-click and its menu taken to the source. */
  function loaded() {
    const doc = frame?.contentDocument;
    if (doc) followButtons(doc);
    if (doc && onsource) {
      doc.addEventListener("click", (e) => {
        if (e.button !== 0) return;
        if (!(e.ctrlKey || e.metaKey)) {
          // a plain click, not the end of a selection: the text scrolled to the block, as the preview follows the
          // cursor
          if (doc.getSelection()?.toString()) return;
          let el = e.target as Element | null;
          while (el && el.parentElement && el.parentElement !== doc.body) el = el.parentElement;
          const index = el && el.parentElement === doc.body ? [...doc.body.children].indexOf(el) : -1;
          if (index >= 0) onblock?.(index);
          return;
        }
        const at = clicked(e);
        if (!at) return;
        e.preventDefault();
        onsource(at.text, at.word, at.around);
      });
      doc.addEventListener("contextmenu", async (e) => {
        const at = clicked(e);
        if (!at) return;
        e.preventDefault();
        const items = await Promise.all([
          MenuItem.new({ text: `Go to Source (${keysOf("⌘")}-click)`, action: () => onsource(at.text, at.word, at.around) }),
          ...(onpdf ? [MenuItem.new({ text: "Show PDF", action: onpdf })] : []),
          PredefinedMenuItem.new({ item: "Separator" }),
          PredefinedMenuItem.new({ item: "Copy" }),
        ]);
        // at the pointer: the frame's own place added to where the click is in it
        const box = frame.getBoundingClientRect();
        await popupReleased(await Menu.new({ items }), new LogicalPosition(box.left + e.clientX, box.top + e.clientY));
      });
    }
    reveal();
  }
</script>

<aside>
  <!-- no allow-scripts: what the text holds as HTML cannot run; allow-same-origin only for the page to scroll it -->
  <iframe bind:this={frame} title="Preview" sandbox="allow-same-origin allow-scripts" srcdoc={doc} onload={loaded}></iframe>
  {#if error}<p class="error">{error}</p>{/if}
</aside>

<style>
  aside {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  iframe {
    flex: 1;
    min-height: 0;
    width: 100%;
    border: 0;
  }
  .error {
    flex: none;
    margin: 0;
    max-height: 30%;
    overflow: auto;
    padding: 6px 14px;
    background: var(--chip);
    border-top: 1px solid var(--line);
    color: var(--error);
    font-size: 12px;
    white-space: pre-wrap;
  }
</style>
