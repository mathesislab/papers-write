<script lang="ts">
  // The quick preview: the HTML pandoc made of the text, in a sandboxed frame (no scripts) with a stylesheet of its
  // own, like a preprint's page. `block` is the index of the top-level block the cursor is in; the frame is scrolled
  // to the same one (pandoc's Markdown reader has no source positions, so the blocks are matched by their order).
  // `error` is what pandoc said, in a bar at the foot.

  let { html, error = "", block = 0 }: { html: string; error?: string; block?: number } = $props();

  let frame: HTMLIFrameElement;

  const SHEET = `
    :root { color-scheme: light dark; --fg: #1d1d1f; --bg: #fdfdfd; --muted: #6e6e73; --rule: #888; --code: rgba(0,0,0,.05); --link: #2f6fde; }
    @media (prefers-color-scheme: dark) { :root { --fg: #e8e8ea; --bg: #1e1e20; --muted: #9a9aa0; --rule: #888; --code: rgba(255,255,255,.08); --link: #7ea6f5; } }
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
  `;

  const doc = $derived(`<!doctype html><meta charset="utf-8"><style>${SHEET}</style><body>${html}</body>`);

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
</script>

<aside>
  <!-- no allow-scripts: what the text holds as HTML cannot run; allow-same-origin only for the page to scroll it -->
  <iframe bind:this={frame} title="Preview" sandbox="allow-same-origin" srcdoc={doc} onload={reveal}></iframe>
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
