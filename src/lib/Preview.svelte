<script lang="ts">
  // The quick preview: the HTML pandoc made of the text, in a sandboxed frame (no scripts) with a stylesheet of its
  // own, like a preprint's page. `block` is the index of the top-level block the cursor is in; the frame is scrolled
  // to the same one (pandoc's Markdown reader has no source positions, so the blocks are matched by their order).
  // `error` is what pandoc said, in a bar at the foot.


  import { followButtons, popupReleased } from "papers-write/project";
  import { readAloudItems } from "$lib/readAloud";
  import { invoke } from "@tauri-apps/api/core";
  import { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu } from "@tauri-apps/api/menu";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  import { showPdfImages } from "$lib/pdfImage";
  import { keys } from "papers-write/editor";

  let {
    html,
    error = "",
    block = 0,
    onsource,
    onblock,
    onpdf,
    previous = null,
    next = null,
    onchapter,
    onscrolled,
    panes,
  }: {
    html: string;
    error?: string;
    block?: number;
    /** ⌃-click (⌘ on macOS) on the preview, or Go to Source in its menu: the text of the block clicked, and the word */
    onsource?: (text: string, word: string, around: { before: string[]; after: string[] }) => void;
    /** a click in the preview (not a selection): the index of the top-level block clicked, for the text to scroll
     *  to it */
    onblock?: (index: number) => void;
    /** Show in PDF, in the menu: the place clicked found in the text, then shown in the PDF (the block, the word) */
    onpdf?: (text: string, word: string, around: { before: string[]; after: string[] }) => void;
    /** the chapters before and after this one in its document (`chapter_neighbours`), linked at the top and the end */
    previous?: Chapter | null;
    next?: Chapter | null;
    /** a click on one of them: that chapter opened (at its end, going up), its preview shown as this one is */
    onchapter?: (path: string, atEnd: boolean) => void;
    /** scrolled (by hand or not): the index of the top-level block at the top of what shows */
    onscrolled?: (index: number) => void;
    /** View ▸ Panes, in the right-click menu too: the form chosen, and the choice */
    panes?: { mode: "side" | "tabs" | "auto"; onpanes: (mode: "side" | "tabs" | "auto") => void };
  } = $props();

  type Chapter = { path: string; title: string };

  /** The ▾ in the corner: Follows the Cursor, Panes ▸, Settings ▸ Editor. */
  async function settingsMenu() {
    const now = await invoke<{ preview_follow: boolean }>("settings").catch(() => null);
    const items = await Promise.all([
      CheckMenuItem.new({
        text: "Follows the Cursor",
        checked: now?.preview_follow ?? true,
        action: () => void invoke("set_preview_follow", { on: !(now?.preview_follow ?? true) }),
      }),
      ...(panes
        ? [
            Submenu.new({
              text: "Panes",
              items: await Promise.all(
                (
                  [
                    ["side", "Beside the Text"],
                    ["tabs", "In Tabs of Their Own"],
                    ["auto", "In Tabs on a Narrow Window"],
                  ] as const
                ).map(([mode, text]) => CheckMenuItem.new({ text, checked: panes.mode === mode, action: () => panes.onpanes(mode) })),
              ),
            }),
          ]
        : []),
      PredefinedMenuItem.new({ item: "Separator" }),
      MenuItem.new({ text: "Settings…", action: () => void invoke("show_settings", { section: "editor" }) }),
    ]);
    // at the pointer, by the arrow
    await popupReleased(await Menu.new({ items }));
  }
  /** the chapters whose previews were made ahead */
  const prepared = new Set<string>();

  let frame: HTMLIFrameElement;

  /** The blocks of the text in the frame: the body's children, but the links to the chapters around. */
  const blocks = (body: HTMLElement) => [...body.children].filter((e) => !e.classList.contains("pw-chapter")) as HTMLElement[];

  /** The ways on, at the top and the end: the previous chapter (opened at its end) and To the Bottom; the next
   *  chapter (at its start) and To the Top. The two jumps only on a text longer than a screen and a half. */
  function chapterLinks() {
    const doc = frame?.contentDocument;
    const win = frame?.contentWindow;
    if (!doc?.body || !win) return;
    doc.querySelectorAll(".pw-chapter").forEach((n) => n.remove());
    const long = doc.body.scrollHeight > win.innerHeight * 1.5;
    // the arrow, then the words (left out on a narrow pane: the arrow alone, the words in the tip)
    const link = (text: string, title: string, data: Record<string, string>) => {
      const a = doc.createElement("a");
      a.href = "#";
      const [arrow, words] = text.startsWith("↑") ? ["↑", text.slice(1)] : ["↓", text.slice(0, -1)];
      const w = doc.createElement("span");
      w.className = "words";
      w.textContent = words;
      const r = doc.createElement("span");
      r.textContent = arrow;
      if (text.startsWith("↑")) a.append(r, w);
      else a.append(w, r);
      a.title = `${title}: ${words.trim()}`;
      Object.assign(a.dataset, data);
      return a;
    };
    const nav = (which: "previous" | "next", links: HTMLElement[]) => {
      const n = doc.createElement("nav");
      n.className = `pw-chapter ${which}`;
      n.append(...links);
      return n;
    };
    const top = [
      ...(previous ? [link(`↑ ${previous.title}`, "The previous chapter, at its end", { chapter: previous.path, end: "1" })] : []),
      ...(long ? [link("To the bottom ↓", "The end of this text", { jump: "bottom" })] : []),
    ];
    const end = [
      ...(long ? [link("↑ To the top", "The start of this text", { jump: "top" })] : []),
      ...(next ? [link(`${next.title} ↓`, "The next chapter, at its start", { chapter: next.path })] : []),
    ];
    // the chapters around made ahead (once each while this preview shows), for their previews to show at once
    for (const c of [next, previous]) {
      if (c && !prepared.has(c.path)) {
        prepared.add(c.path);
        void invoke("prepare_preview", { path: c.path }).catch(() => prepared.delete(c.path));
      }
    }
    if (top.length) doc.body.prepend(nav("previous", top));
    if (end.length) doc.body.append(nav("next", end));
  }
  $effect(() => {
    void previous;
    void next;
    chapterLinks();
  });

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
    .pw-chapter { display: flex; justify-content: space-between; gap: 1em; font: .85em/1.4 system-ui, sans-serif; margin: 0 0 2em; }
    .pw-chapter.next { margin: 3em 0 0; }
    .pw-chapter a:only-child { margin-left: auto; }
    @media (max-width: 26em) { .pw-chapter .words { display: none; } .pw-chapter a { font-size: 1.3em; } }
    .pw-pull { position: fixed; left: 0; right: 0; z-index: 9; display: flex; flex-direction: column; align-items: center;
      gap: 4px; pointer-events: none; font: .8em/1.3 system-ui, sans-serif; color: var(--muted); }
    .pw-pull.down { bottom: 0; flex-direction: column-reverse; } .pw-pull.up { top: 0; }
    .pw-pull i { display: block; height: 3px; align-self: stretch; background: var(--link); transform-origin: left; }
    .pw-chapter.previous a:only-child[data-chapter] { margin-left: 0; }
    .pw-chapter a { color: var(--muted); }
    .pw-chapter a:hover { color: var(--link); }
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
    const all = blocks(body);
    const el = all[Math.min(block, all.length - 1)];
    if (el) frame.contentWindow?.scrollTo({ top: Math.max(0, el.offsetTop - 48) });
  }

  $effect(() => {
    block;
    reveal();
  });

  /** The block the cursor is in brought into view and marked a moment (Show in Preview). */
  /** The keys to the preview (its side was the one worked in). */
  export function focus() {
    frame?.contentWindow?.focus();
  }

  export function flash() {
    reveal();
    const body = frame?.contentDocument?.body;
    const all = body ? blocks(body) : [];
    const el = all[Math.min(block, all.length - 1)];
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

  /** The frame loaded: its PDF figures drawn as images (WebKitGTK does not draw them), ⌃-click and its menu taken
   *  to the source, the block in view. */
  function loaded() {
    const doc = frame?.contentDocument;
    if (doc?.body) void showPdfImages(doc.body);
    chapterLinks();
    // scrolled on past the end (or the start) with insistence: a bar fills, and when full the next chapter opens (the
    // previous, at its end); a pause empties it
    if (doc && frame?.contentWindow) {
      const win = frame.contentWindow;
      const FULL = 700;
      let pull = 0;
      let way: "down" | "up" | null = null;
      let rest: ReturnType<typeof setTimeout> | undefined;
      let bar: HTMLElement | null = null;
      const clear = () => {
        pull = 0;
        way = null;
        bar?.remove();
        bar = null;
      };
      doc.addEventListener(
        "wheel",
        (e) => {
          const dy = e.deltaY * (e.deltaMode === 1 ? 16 : 1);
          const atEnd = win.scrollY + win.innerHeight >= doc.documentElement.scrollHeight - 2;
          const atStart = win.scrollY <= 0;
          const now = dy > 0 && atEnd && next ? "down" : dy < 0 && atStart && previous ? "up" : null;
          if (!now || (way && way !== now)) return clear();
          way = now;
          pull += Math.abs(dy);
          const chapter = (now === "down" ? next : previous)!;
          if (!bar) {
            bar = doc.createElement("div");
            bar.className = `pw-pull ${now}`;
            bar.innerHTML = "<i></i><span></span>";
            doc.documentElement.append(bar);
          }
          bar.querySelector("span")!.textContent = `Keep scrolling for ${chapter.title}`;
          (bar.querySelector("i") as HTMLElement).style.transform = `scaleX(${Math.min(1, pull / FULL)})`;
          clearTimeout(rest);
          rest = setTimeout(clear, 450);
          if (pull >= FULL) {
            const end = now === "up";
            clear();
            onchapter?.(chapter.path, end);
          }
        },
        { passive: true },
      );
    }
    // the block at the top of what shows, told as it changes (once a frame)
    let atTop = -1;
    let pending = 0;
    frame?.contentWindow?.addEventListener("scroll", () => {
      if (!onscrolled || pending) return;
      pending = requestAnimationFrame(() => {
        pending = 0;
        const top = (frame?.contentWindow?.scrollY ?? 0) + 24;
        const all = doc?.body ? blocks(doc.body) : [];
        const index = all.findIndex((el) => el.offsetTop + el.offsetHeight > top);
        if (index >= 0 && index !== atTop) onscrolled?.((atTop = index));
      });
    });
    if (doc) {
      followButtons(doc);
      // the keys typed in the frame given to the page too (its own keys: ⌃Tab, ⌃+, ⌃ let go), which a frame's keep
      for (const type of ["keydown", "keyup"] as const) {
        doc.addEventListener(type, (e) => {
          const { key, code, ctrlKey, shiftKey, altKey, metaKey, repeat } = e;
          window.dispatchEvent(new KeyboardEvent(type, { key, code, ctrlKey, shiftKey, altKey, metaKey, repeat }));
        });
      }
    }
    if (doc && onsource) {
      doc.addEventListener("click", (e) => {
        if (e.button !== 0) return;
        const chapter = (e.target as Element | null)?.closest?.("a[data-chapter], a[data-jump]") as HTMLAnchorElement | null;
        if (chapter) {
          e.preventDefault();
          if (chapter.dataset.jump) {
            const win = frame?.contentWindow;
            win?.scrollTo({ top: chapter.dataset.jump === "top" ? 0 : doc.body.scrollHeight, behavior: "smooth" });
          } else onchapter?.(chapter.dataset.chapter ?? "", chapter.dataset.end === "1");
          return;
        }
        if (!(e.ctrlKey || e.metaKey)) {
          // a plain click, not the end of a selection: the text scrolled to the block, as the preview follows the
          // cursor
          if (doc.getSelection()?.toString()) return;
          let el = e.target as Element | null;
          while (el && el.parentElement && el.parentElement !== doc.body) el = el.parentElement;
          const index = el && el.parentElement === doc.body ? blocks(doc.body).indexOf(el as HTMLElement) : -1;
          if (index >= 0) onblock?.(index);
          return;
        }
        const at = clicked(e);
        if (!at) return;
        e.preventDefault();
        onsource(at.text, at.word, at.around);
      });
      // a double-click: Go to Source, as ⌃-click; one whose pointer moved before it was let go is a selection made by
      // double-tap and drag, and stays one
      let secondPress: { x: number; y: number } | null = null;
      doc.addEventListener("mousedown", (e) => {
        if (e.detail >= 2) secondPress = { x: e.clientX, y: e.clientY };
      });
      doc.addEventListener("dblclick", (e) => {
        if (e.button !== 0) return;
        const moved = secondPress && Math.hypot(e.clientX - secondPress.x, e.clientY - secondPress.y) > 4;
        secondPress = null;
        if (moved) return;
        const at = clicked(e);
        if (!at) return;
        doc.getSelection()?.removeAllRanges();
        onsource(at.text, at.word, at.around);
      });
      doc.addEventListener("contextmenu", async (e) => {
        const at = clicked(e);
        if (!at) return;
        e.preventDefault();
        // Read Aloud: the selection, else the block (paragraph) clicked
        const selected = doc.getSelection()?.toString().trim() ?? "";
        const reading = await readAloudItems(selected || at.text, selected ? "Read Aloud" : "Read Aloud Paragraph");
        const items = await Promise.all([
          MenuItem.new({ text: keys("Go to Source (⌘-click)"), action: () => onsource(at.text, at.word, at.around) }),
          ...(onpdf ? [MenuItem.new({ text: "Show in PDF", action: () => onpdf(at.text, at.word, at.around) })] : []),
          PredefinedMenuItem.new({ item: "Separator" }),
          PredefinedMenuItem.new({ item: "Copy" }),
          ...(panes
            ? [
                PredefinedMenuItem.new({ item: "Separator" }),
                Submenu.new({
                  text: "Panes",
                  items: await Promise.all(
                    (
                      [
                        ["side", "Beside the Text"],
                        ["tabs", "In Tabs of Their Own"],
                        ["auto", "In Tabs on a Narrow Window"],
                      ] as const
                    ).map(([mode, text]) => CheckMenuItem.new({ text, checked: panes.mode === mode, action: () => panes.onpanes(mode) })),
                  ),
                }),
              ]
            : []),
          ...reading,
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
  <!-- allow-scripts only for the page's own listeners on the frame: the document's policy runs none of its scripts;
       allow-same-origin for the page to scroll it -->
  <iframe bind:this={frame} title="Preview" sandbox="allow-same-origin allow-scripts" srcdoc={doc} onload={loaded}></iframe>
  {#if error}<p class="error">{error}</p>{/if}
  <!-- the preview's settings, in its corner (settings at the foot, as Build's): whether it follows the cursor, the panes -->
  <button class="settings" onclick={settingsMenu} title="The preview's settings" aria-label="Preview settings">Preview ▾</button>
</aside>

<style>
  /* in the corner, over the preview, faint till hovered */
  .settings {
    position: absolute;
    right: 10px;
    bottom: 8px;
    font: 11.5px system-ui, sans-serif;
    padding: 1px 8px;
    color: var(--muted, #888);
    background: var(--bg, transparent);
    border: 1px solid var(--border, #888);
    border-radius: 5px;
    opacity: 0.6;
    cursor: pointer;
  }
  .settings:hover {
    opacity: 1;
  }
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
