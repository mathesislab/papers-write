<script lang="ts">
  // The Settings window (⌘,), set out as Papers Review's: a bar of tabs (⌘1…⌘4), the tab shown, which scrolls, and
  // the line at its foot. Editor: the font and size, what Tab puts in, the readable line length, line numbers, spelling,
  // the preview following the cursor, the symbols typed (the defaults each turned off if not wanted, and the user's own).
  // Review: the user's name, e-mail and colour (empty: from the git configuration, as before). Build: where texmark,
  // latexmk and pandoc are (empty: found as usual, which the hint says). Window: files as tabs, the menu bar icon.
  // Every change is kept at once (settings.json of the app's data folder); the windows are told as it changes.

  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import "$lib/theme.css";
  import "$lib/settings.css";
  import { DEFAULTS, REVIEW_COLOURS } from "papers-write/editor";
  import type { EditorFont, Indent } from "papers-write/editor";
  import { watchSettings, type Settings } from "$lib/settings";

  const TABS = [
    { id: "editor", name: "Editor" },
    { id: "review", name: "Review" },
    { id: "build", name: "Build" },
    { id: "window", name: "Window" },
  ] as const;
  type Tab = (typeof TABS)[number]["id"];

  const FONTS: [EditorFont, string][] = [
    ["system", "System"],
    ["serif", "Serif"],
    ["mono", "Monospace"],
  ];
  const INDENTS: [Indent, string][] = [
    ["two", "2 spaces"],
    ["four", "4 spaces"],
    ["tab", "A tab"],
  ];
  type Tool = "texmark" | "latexmk" | "pandoc";
  const TOOLS: { name: Tool; field: "texmark_path" | "latexmk_path" | "pandoc_path"; hint: string }[] = [
    { name: "texmark", field: "texmark_path", hint: "Markdown to LaTeX and PDF (Build)" },
    { name: "latexmk", field: "latexmk_path", hint: "LaTeX documents and the marked-up PDF" },
    { name: "pandoc", field: "pandoc_path", hint: "The preview and the Word exchange" },
  ];

  let settings = $state<Settings | null>(null);
  let tab = $state<Tab>("editor");
  /** where each tool is found as usual: the path, or null; undefined until asked */
  let found = $state<Record<Tool, string | null> | undefined>();
  /** the sequence and the replacement of a pair being added */
  let keys = $state("");
  let insert = $state("");
  const addable = $derived(keys.length >= 2 && insert !== "");

  function save() {
    if (settings) invoke("set_settings", { settings: $state.snapshot(settings) });
  }

  async function choose(tool: (typeof TOOLS)[number]) {
    if (!settings) return;
    const file = await invoke<string | null>("choose_tool", { name: tool.name });
    if (!file) return;
    settings[tool.field] = file;
    save();
  }

  /** The pairs of the user, but for those of `sequence`. */
  const without = (sequence: string) => (settings?.symbols ?? []).filter(([k]) => k !== sequence);
  /** Whether a default sequence is turned off: a pair of the user gives it an empty replacement. */
  const off = (sequence: string) => !!settings?.symbols.some(([k, v]) => k === sequence && !v);

  function turn(sequence: string, on: boolean) {
    if (!settings) return;
    settings.symbols = on ? without(sequence) : [...without(sequence), [sequence, ""]];
    save();
  }

  function add() {
    if (!settings || !addable) return;
    settings.symbols = [...without(keys), [keys, insert]];
    [keys, insert] = ["", ""];
    save();
  }

  function remove(sequence: string) {
    if (!settings) return;
    settings.symbols = without(sequence);
    save();
  }

  function setColour(colour: number | null) {
    if (!settings) return;
    settings.review_colour = colour;
    save();
  }

  onMount(() => {
    invoke<Record<Tool, string | null>>("tools_found").then((f) => (found = f));
    return watchSettings((s) => (settings = s));
  });

  /** ⌘1…⌘4 show a tab. Esc with no field in focus closes the window (a field gives up the focus first). */
  function key(e: KeyboardEvent) {
    const n = e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey ? Number(e.code.replace("Digit", "")) : NaN;
    if (n >= 1 && n <= TABS.length) {
      e.preventDefault();
      tab = TABS[n - 1].id;
      return;
    }
    if (e.key !== "Escape") return;
    const field = document.activeElement;
    if (field instanceof HTMLElement && field !== document.body) field.blur();
    else getCurrentWindow().close();
  }
</script>

<svelte:window onkeydown={key} />

{#if settings}
  <main class="settings">
    <div class="tabs" role="tablist" aria-label="Settings">
      {#each TABS as t, i (t.id)}
        <button
          role="tab"
          id="tab-{t.id}"
          aria-selected={tab === t.id}
          aria-controls="panel-{t.id}"
          class:on={tab === t.id}
          title="⌘{i + 1}"
          onclick={() => (tab = t.id)}>{t.name}</button
        >
      {/each}
    </div>

    <div class="panel" role="tabpanel" id="panel-editor" aria-labelledby="tab-editor" hidden={tab !== "editor"}>
      <section>
        <h2>Editor</h2>
        <label class="row">
          <span>
            <span class="label">Font</span>
            <span class="hint">The text of the documents</span>
          </span>
          <select bind:value={settings.editor_font} onchange={save}>
            {#each FONTS as [font, name] (font)}<option value={font}>{name}</option>{/each}
          </select>
        </label>
        <label class="row">
          <span class="label">Size</span>
          <span class="size">
            <input type="range" min="11" max="20" step="0.5" bind:value={settings.editor_size} onchange={save} />
            <span class="points">{settings.editor_size} pt</span>
          </span>
        </label>
        <label class="row">
          <span>
            <span class="label">Tab inserts</span>
            <span class="hint">With lines selected, Tab indents them and ⇧Tab takes it out. In Markdown, 4 spaces or a tab begin a block of code</span>
          </span>
          <select bind:value={settings.indent} onchange={save}>
            {#each INDENTS as [indent, name] (indent)}<option value={indent}>{name}</option>{/each}
          </select>
        </label>
        <label class="row">
          <span>
            <span class="label">Readable line length</span>
            <span class="hint">The text a column in the middle, with a margin on each side; off: as wide as the window</span>
          </span>
          <input type="checkbox" class="switch" bind:checked={settings.readable_width} onchange={save} />
        </label>
        <label class="row">
          <span class="label">Line numbers</span>
          <input type="checkbox" class="switch" bind:checked={settings.line_numbers} onchange={save} />
        </label>
        <label class="row">
          <span>
            <span class="label">Check spelling</span>
            <span class="hint">As the text is typed</span>
          </span>
          <input type="checkbox" class="switch" bind:checked={settings.spelling} onchange={save} />
        </label>
        <label class="row">
          <span>
            <span class="label">Preview follows the cursor</span>
            <span class="hint">The preview scrolled to the block the cursor is in</span>
          </span>
          <input type="checkbox" class="switch" bind:checked={settings.preview_follow} onchange={save} />
        </label>
      </section>

      <section>
        <h2>Typed symbols</h2>
        <p class="intro">
          In Markdown, a sequence typed becomes its symbol as its last character is typed; ⌘Z gives back what was
          typed. Tab after a LaTeX name (<code>\alpha</code>) gives its symbol; ⌘/ or Edit ▸ Symbols… lists them.
        </p>
        <h3>Your own</h3>
        <ul class="pairs">
          {#each settings.symbols.filter(([, v]) => v) as [sequence, symbol] (sequence)}
            <li>
              <code>{sequence}</code>
              <span class="arrow">→</span>
              <span class="symbol">{symbol}</span>
              <button class="remove" onclick={() => remove(sequence)} title="Remove" aria-label="Remove {sequence}">✕</button>
            </li>
          {:else}
            <li class="none">None yet</li>
          {/each}
        </ul>
        <form
          onsubmit={(e) => {
            e.preventDefault();
            add();
          }}
        >
          <input class="text" bind:value={keys} placeholder="Typed" spellcheck="false" autocomplete="off" />
          <span class="arrow">→</span>
          <input class="text" bind:value={insert} placeholder="Becomes" spellcheck="false" autocomplete="off" />
          <button type="submit" disabled={!addable}>Add</button>
        </form>
        <p class="hint">At least two characters. A sequence that is a default gives it another symbol.</p>
      </section>

      <section>
        <h2>Defaults</h2>
        <ul class="pairs">
          {#each DEFAULTS as rule (rule.keys)}
            <li>
              <label>
                <input type="checkbox" checked={!off(rule.keys)} onchange={(e) => turn(rule.keys, e.currentTarget.checked)} />
                <code>{rule.keys}</code>
                <span class="arrow">→</span>
                <span class="symbol">{rule.insert}</span>
              </label>
            </li>
          {/each}
        </ul>
        <p class="hint">Powers (<code>^2</code>) follow a letter, a digit or <code>)</code>; indices (<code>_2</code>) a letter.</p>
      </section>
    </div>

    <div class="panel" role="tabpanel" id="panel-review" aria-labelledby="tab-review" hidden={tab !== "review"}>
      <section>
        <h2>You in the review</h2>
        <label class="row">
          <span>
            <span class="label">Name</span>
            <span class="hint">Empty: the name of the git configuration, else the system's</span>
          </span>
          <input class="text name" bind:value={settings.review_name} onchange={save} placeholder="Your name" autocomplete="off" spellcheck="false" />
        </label>
        <label class="row">
          <span>
            <span class="label">E-mail</span>
            <span class="hint">Empty: the git configuration's</span>
          </span>
          <input class="text name" bind:value={settings.review_email} onchange={save} placeholder="you@example.org" autocomplete="off" spellcheck="false" />
        </label>
        <div class="row">
          <span>
            <span class="label">Colour</span>
            <span class="hint">Of your suggestions and comments, as everyone sees them (kept in your review files); automatic: from your name</span>
          </span>
          <span class="swatches" role="radiogroup" aria-label="Review colour">
            <button
              class="swatch none"
              class:on={settings.review_colour === null}
              role="radio"
              aria-checked={settings.review_colour === null}
              title="Automatic"
              aria-label="Automatic"
              onclick={() => setColour(null)}
            ></button>
            {#each REVIEW_COLOURS as css, i (css)}
              <button
                class="swatch"
                class:on={settings.review_colour === i + 1}
                style:--c="var(--review-{i + 1}, {css})"
                role="radio"
                aria-checked={settings.review_colour === i + 1}
                title="Colour {i + 1}"
                aria-label="Colour {i + 1}"
                onclick={() => setColour(i + 1)}
              ></button>
            {/each}
          </span>
        </div>
      </section>
    </div>

    <div class="panel" role="tabpanel" id="panel-build" aria-labelledby="tab-build" hidden={tab !== "build"}>
      <section>
        <h2>Tools</h2>
        <p class="intro">Empty: the tool is found on PATH, else in the usual folders (Homebrew, conda, MacTeX).</p>
        {#each TOOLS as tool (tool.name)}
          <div class="row">
            <span class="grow">
              <span class="label">{tool.name}</span>
              <span class="hint">{tool.hint}</span>
              <span class="hint" class:error={found && !found[tool.name]}>
                {#if !found}&nbsp;{:else if found[tool.name]}Found: <code>{found[tool.name]}</code>{:else}Not found{/if}
              </span>
            </span>
            <span class="buttons">
              <input
                class="text path"
                bind:value={settings[tool.field]}
                onchange={save}
                placeholder={found?.[tool.name] ?? "Path"}
                autocomplete="off"
                spellcheck="false"
                aria-label="{tool.name} path"
              />
              <button onclick={() => choose(tool)}>Choose…</button>
            </span>
          </div>
        {/each}
      </section>
    </div>

    <div class="panel" role="tabpanel" id="panel-window" aria-labelledby="tab-window" hidden={tab !== "window"}>
      <section>
        <h2>Window</h2>
        <label class="row">
          <span>
            <span class="label">Open files as tabs</span>
            <span class="hint">Off: each file in a window of its own. Applies to the files opened from now on</span>
          </span>
          <input type="checkbox" class="switch" bind:checked={settings.tabs} onchange={save} />
        </label>
        <label class="row">
          <span>
            <span class="label">Stay in the menu bar</span>
            <span class="hint">When the last window closes; off: the app quits</span>
          </span>
          <input type="checkbox" class="switch" bind:checked={settings.menu_bar} onchange={save} />
        </label>
        <label class="row">
          <span>
            <span class="label">Project colour bar</span>
            <span class="hint">A thin bar in the project's colour along the top of its windows</span>
          </span>
          <input type="checkbox" class="switch" bind:checked={settings.project_colour_bar} onchange={save} />
        </label>
      </section>
    </div>

    <footer>Changes apply at once.</footer>
  </main>
{/if}

<style>
  :global(html, body) {
    margin: 0;
    background: var(--bg);
    color: var(--fg);
    font-size: 13px;
    cursor: default;
    -webkit-user-select: none;
    user-select: none;
  }

  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  /* the bar of tabs: the one shown lifted, in the accent */
  .tabs {
    flex: none;
    display: flex;
    justify-content: center;
    gap: 2px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--sidebar);
  }
  .tabs button {
    padding: 4px 12px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: none;
    color: var(--muted);
  }
  .tabs button:hover {
    color: var(--fg);
  }
  .tabs button.on {
    border-color: var(--line);
    background: var(--menu-bg);
    color: var(--accent);
  }

  /* a tab: its cards, scrolled on their own */
  .panel {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px 22px;
  }
  .panel[hidden] {
    display: none;
  }

  footer {
    flex: none;
    padding: 8px 22px 10px;
    border-top: 1px solid var(--line);
    color: var(--muted);
    font-size: 11.5px;
    text-align: center;
  }

  .grow {
    flex: 1;
    min-width: 0;
  }
  .hint code {
    overflow-wrap: anywhere;
    -webkit-user-select: text;
    user-select: text;
  }
  .text.name,
  .text.path {
    -webkit-user-select: text;
    user-select: text;
  }
  .text.name {
    font-family: inherit;
    font-size: inherit;
  }
  .text.path {
    width: 200px;
  }
  .size {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  input[type="range"] {
    width: 150px;
    margin: 0;
  }
  .points {
    min-width: 42px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  /* the colours of the review: a dot each, the one chosen ringed; automatic crossed */
  .swatches {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 6px;
  }
  .swatch {
    width: 16px;
    height: 16px;
    padding: 0;
    border: 2px solid transparent;
    border-radius: 50%;
    background: var(--c);
    box-shadow: 0 0 0 1px var(--border);
    cursor: pointer;
  }
  .swatch.none {
    background: linear-gradient(135deg, transparent 45%, var(--muted) 45%, var(--muted) 55%, transparent 55%);
  }
  .swatch.on {
    border-color: var(--bg);
    box-shadow: 0 0 0 2px var(--fg);
  }

  /* the symbols */
  .intro {
    margin: 4px 0 0;
    color: var(--muted);
    font-size: 12px;
  }
  .pairs {
    list-style: none;
    margin: 4px 0 8px;
    padding: 0;
    columns: 2;
  }
  .pairs li,
  .pairs label {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 1px 0;
    break-inside: avoid;
  }
  .pairs .none {
    color: var(--muted);
  }
  .pairs input[type="checkbox"] {
    margin: 0;
  }
  .pairs .remove {
    margin-left: auto;
  }
  .arrow {
    color: var(--muted);
  }
  .symbol {
    font-size: 14px;
  }
  form {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 4px 0 6px;
  }
  form .text {
    width: 90px;
    -webkit-user-select: text;
    user-select: text;
  }
</style>
