// Find and replace in the text (CodeMirror's search), as a bar at the top of the editor: the text to find, how many
// matches and which one the selection is, case, whole word and regular expression, previous and next, and × at the
// right to close it; the replace field, Replace and All under it, shown or hidden by the arrow at the left. Find (⌘F)
// opens the bar without the replace field, Find and Replace (⌥⌘F) with it; the app's menu gives the commands
// (`find`), and ⌘G, ⇧⌘G, ⌘E (the selection to find). In suggest mode a replacement is the user's suggestion, as typing
// is (own.ts).

import {
  SearchQuery,
  closeSearchPanel,
  findNext,
  findPrevious,
  getSearchQuery,
  openSearchPanel,
  replaceAll,
  replaceNext,
  search,
  searchKeymap,
  searchPanelOpen,
  setSearchQuery,
} from "@codemirror/search";
import { StateEffect, StateField, type EditorState, type Extension } from "@codemirror/state";
import { EditorView, keymap, type Panel, type ViewUpdate } from "@codemirror/view";
import { keys } from "./platform.ts";

/** What the menu asks: the bar (on the find field, or the replace field), the next or previous match, the selection
 *  as the text to find; or the bar closed (the toolbar's button of the bar shown). */
export type FindCommand = "find" | "replace" | "next" | "previous" | "selection" | "close";

/** What the bar shows: none (closed), the find field alone, or the replace field too. */
export type FindBar = "find" | "replace" | null;

/** The replace field shown, or hidden. */
const showReplace = StateEffect.define<boolean>();
const replaceShown = StateField.define<boolean>({
  create: () => false,
  update(shown, tr) {
    for (const e of tr.effects) if (e.is(showReplace)) shown = e.value;
    return shown;
  },
});

/** What the bar shows in `state`. */
export const findBar = (state: EditorState): FindBar =>
  !searchPanelOpen(state) ? null : state.field(replaceShown, false) ? "replace" : "find";

/** The bar closed, the keys back to the text (Esc, ×, or the toolbar's button of the bar shown). */
function closeBar(view: EditorView) {
  closeSearchPanel(view);
  view.focus();
}

/** The most matches counted; more are "1000+". */
const MOST = 1000;

/** What a query is made of, to make another with something changed. */
const spec = (q: SearchQuery) => ({ search: q.search, caseSensitive: q.caseSensitive, regexp: q.regexp, wholeWord: q.wholeWord, replace: q.replace, literal: q.literal });

/** The matches of the query in the text, and which one the main selection is (0: none). */
function count(state: EditorState, query: SearchQuery): { total: number; at: number } {
  if (!query.valid) return { total: 0, at: 0 };
  const { from, to } = state.selection.main;
  const cursor = query.getCursor(state);
  let total = 0;
  let at = 0;
  for (let next = cursor.next(); !next.done && total < MOST; next = cursor.next()) {
    total++;
    if (next.value.from === from && next.value.to === to) at = total;
  }
  return { total, at };
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Record<string, string> = {}, ...children: (Node | string)[]) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  node.append(...children);
  return node;
}

const ICONS = {
  previous: '<svg viewBox="0 0 12 12" width="11" height="11"><path d="M2.5 7.5 6 4l3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  next: '<svg viewBox="0 0 12 12" width="11" height="11"><path d="M2.5 4.5 6 8l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  close: '<svg viewBox="0 0 12 12" width="12" height="12"><path d="M3 3l6 6M9 3l-6 6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
  expand: '<svg viewBox="0 0 12 12" width="11" height="11"><path d="M4.5 2.5 8 6l-3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>',
};

function iconButton(name: keyof typeof ICONS, title: string, run: () => void) {
  const b = el("button", { type: "button", title, "aria-label": title, class: "cm-find-icon" });
  b.innerHTML = ICONS[name];
  b.addEventListener("mousedown", (e) => e.preventDefault());
  b.addEventListener("click", run);
  return b;
}

/** The bar. */
function panel(view: EditorView): Panel {
  let query = getSearchQuery(view.state);
  const field = el("input", { class: "cm-find-field", placeholder: "Find", "main-field": "true", "aria-label": "Find", spellcheck: "false", autocomplete: "off" });
  const replace = el("input", { class: "cm-find-field", placeholder: "Replace", "aria-label": "Replace", spellcheck: "false", autocomplete: "off" });
  const total = el("span", { class: "cm-find-count" });
  const toggle = (label: string, title: string, on: () => boolean, set: (q: SearchQuery, v: boolean) => SearchQuery) => {
    const b = el("button", { type: "button", title, "aria-label": title, class: "cm-find-toggle" }, label);
    b.addEventListener("mousedown", (e) => e.preventDefault());
    b.addEventListener("click", () => commit(set(query, !on())));
    return b;
  };
  const caseBtn = toggle("Aa", "Match case", () => query.caseSensitive, (q, v) => new SearchQuery({ ...spec(q), caseSensitive: v }));
  const wordBtn = toggle("W", "Whole words", () => query.wholeWord, (q, v) => new SearchQuery({ ...spec(q), wholeWord: v }));
  const regexBtn = toggle(".*", "Regular expression", () => query.regexp, (q, v) => new SearchQuery({ ...spec(q), regexp: v }));

  function commit(next: SearchQuery) {
    if (!next.eq(query)) view.dispatch({ effects: setSearchQuery.of(next) });
  }
  const typed = () => commit(new SearchQuery({ ...spec(query), search: field.value, replace: replace.value }));
  field.addEventListener("input", typed);
  replace.addEventListener("input", typed);

  field.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      (e.shiftKey ? findPrevious : findNext)(view);
    }
  });
  replace.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      (e.metaKey ? replaceAll : replaceNext)(view);
    }
  });
  for (const input of [field, replace])
    input.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        e.preventDefault();
        closeBar(view);
      }
    });

  const button = (label: string, title: string, run: () => void) => {
    const b = el("button", { type: "button", title, class: "cm-find-button" }, label);
    b.addEventListener("mousedown", (e) => e.preventDefault());
    b.addEventListener("click", run);
    return b;
  };

  // the arrow shows or hides the replace field, the keys to the field shown
  const expand = iconButton("expand", "", () => {
    const shown = !view.state.field(replaceShown);
    view.dispatch({ effects: showReplace.of(shown) });
    (shown ? replace : field).focus();
  });
  expand.classList.add("cm-find-expand");
  const close = iconButton("close", "Close (Esc)", () => closeBar(view));
  close.classList.add("cm-find-close");
  const replaceRow = el(
    "div",
    { class: "cm-find-row cm-find-replace" },
    replace,
    button("Replace", "Replace this match, and find the next (↵)", () => replaceNext(view)),
    button("All", keys("Replace all (⌘↵)"), () => replaceAll(view)),
  );
  const dom = el(
    "div",
    { class: "cm-find" },
    el(
      "div",
      { class: "cm-find-row" },
      expand,
      field,
      total,
      caseBtn,
      wordBtn,
      regexBtn,
      iconButton("previous", keys("Previous (⇧⌘G)"), () => findPrevious(view)),
      iconButton("next", keys("Next (⌘G)"), () => findNext(view)),
      close,
    ),
    replaceRow,
  );

  function showRow(state: EditorState) {
    const shown = state.field(replaceShown);
    replaceRow.style.display = shown ? "" : "none";
    expand.classList.toggle("open", shown);
    const title = shown ? "Hide Replace" : keys("Show Replace (⌥⌘F)");
    expand.title = title;
    expand.setAttribute("aria-label", title);
    expand.setAttribute("aria-expanded", String(shown));
  }
  showRow(view.state);

  function show(state: EditorState) {
    query = getSearchQuery(state);
    if (field.value !== query.search) field.value = query.search;
    if (replace.value !== query.replace) replace.value = query.replace;
    caseBtn.classList.toggle("on", query.caseSensitive);
    wordBtn.classList.toggle("on", query.wholeWord);
    regexBtn.classList.toggle("on", query.regexp);
    field.classList.toggle("bad", !!query.search && !query.valid);
    if (!query.search) total.textContent = "";
    else {
      const { total: n, at } = count(state, query);
      total.textContent = n === 0 ? "No matches" : `${at ? `${at} of ` : ""}${n >= MOST ? `${MOST}+` : n}`;
    }
  }
  show(view.state);

  return {
    dom,
    top: true,
    mount: () => field.select(),
    update(u: ViewUpdate) {
      if (u.docChanged || u.selectionSet || u.transactions.some((t) => t.effects.some((e) => e.is(setSearchQuery)))) show(u.state);
      if (u.startState.field(replaceShown) !== u.state.field(replaceShown)) showRow(u.state);
    },
  };
}

/** The selected text, when it is on one line. */
function selected(state: EditorState): string {
  const { from, to } = state.selection.main;
  const text = state.sliceDoc(from, to);
  return from < to && !text.includes("\n") ? text : "";
}

/** A search of the project (write_core::search::Query), whose matches in this text are to be replaced. */
export type ProjectQuery = { text: string; case: boolean; word: boolean; regex: boolean };

/** Every match of `query` replaced by `by`, as Replace All of the bar does (an edit, or suggestions in suggest mode). */
export function replaceEverywhere(view: EditorView, query: ProjectQuery, by: string) {
  const before = getSearchQuery(view.state);
  const q = new SearchQuery({ search: query.text, caseSensitive: query.case, wholeWord: query.word, regexp: query.regex, literal: true, replace: by });
  view.dispatch({ effects: setSearchQuery.of(q) });
  replaceAll(view);
  view.dispatch({ effects: setSearchQuery.of(before) });
}

/** A command of the menu. */
export function find(view: EditorView, what: FindCommand) {
  const open = searchPanelOpen(view.state);
  switch (what) {
    case "find":
    case "replace": {
      // Find the bar alone, Find and Replace with the replace field: the one made the other
      const text = selected(view.state);
      const effects: StateEffect<unknown>[] = [showReplace.of(what === "replace")];
      if (text) effects.push(setSearchQuery.of(new SearchQuery({ ...spec(getSearchQuery(view.state)), search: text })));
      view.dispatch({ effects });
      openSearchPanel(view);
      const fields = view.dom.querySelectorAll<HTMLInputElement>(".cm-find-field");
      const target = fields[what === "replace" ? 1 : 0];
      target?.focus();
      target?.select();
      return;
    }
    case "next":
    case "previous":
      if (!getSearchQuery(view.state).search) return void find(view, "find");
      (what === "next" ? findNext : findPrevious)(view);
      return;
    case "selection": {
      const text = selected(view.state);
      if (text) view.dispatch({ effects: setSearchQuery.of(new SearchQuery({ ...spec(getSearchQuery(view.state)), search: text })) });
      if (!open) view.focus();
      return;
    }
    case "close":
      closeBar(view);
  }
}

const theme = EditorView.theme({
  ".cm-panels": { background: "var(--sidebar, #eeeef1)", color: "var(--fg)" },
  ".cm-panels-top": { borderBottom: "1px solid var(--line, rgba(0,0,0,0.08))" },
  ".cm-find": { display: "flex", flexDirection: "column", gap: "5px", padding: "6px 10px", fontFamily: "-apple-system, system-ui, sans-serif", fontSize: "12px" },
  ".cm-find-row": { display: "flex", alignItems: "center", gap: "4px" },
  ".cm-find-field": {
    flex: "0 1 280px",
    minWidth: "80px",
    font: "inherit",
    color: "var(--fg)",
    background: "var(--bg)",
    border: "1px solid var(--border, rgba(0,0,0,0.14))",
    borderRadius: "5px",
    padding: "2px 7px",
    outline: "none",
  },
  ".cm-find-field:focus": { borderColor: "var(--accent, #2f6fde)", boxShadow: "0 0 0 2px var(--selection, rgba(47,111,222,0.16))" },
  ".cm-find-field.bad": { borderColor: "var(--error, #c4321f)" },
  ".cm-find-count": { minWidth: "64px", color: "var(--muted)", fontSize: "11px", textAlign: "right", paddingRight: "4px", whiteSpace: "nowrap" },
  ".cm-find-toggle, .cm-find-icon, .cm-find-button": {
    font: "inherit",
    color: "var(--muted)",
    background: "none",
    border: "1px solid transparent",
    borderRadius: "5px",
    padding: "1px 6px",
    lineHeight: "16px",
    display: "inline-flex",
    alignItems: "center",
  },
  ".cm-find-toggle": { fontFamily: "ui-monospace, Menlo, monospace", fontSize: "11px", minWidth: "24px", justifyContent: "center" },
  ".cm-find-toggle:hover, .cm-find-icon:hover, .cm-find-button:hover": { color: "var(--fg)", background: "var(--chip)" },
  ".cm-find-toggle.on": { color: "var(--accent, #2f6fde)", background: "var(--selection)", borderColor: "transparent" },
  ".cm-find-button": { color: "var(--fg)", border: "1px solid var(--border, rgba(0,0,0,0.14))", background: "var(--bg)" },
  ".cm-find-replace": { paddingLeft: "23px" },
  ".cm-find-expand": { padding: "1px 3px" },
  ".cm-find-expand svg": { transition: "transform 0.12s" },
  ".cm-find-expand.open svg": { transform: "rotate(90deg)" },
  // × at the right end of the bar, where a panel's close is looked for
  ".cm-find-close": { marginLeft: "auto", color: "var(--fg)", padding: "2px 5px" },
  ".cm-searchMatch": { background: "rgba(255, 196, 0, 0.32)", borderRadius: "2px" },
  ".cm-searchMatch-selected": { background: "rgba(255, 150, 0, 0.55)" },
});

/** Find and replace, with the bar above. */
export const finding: Extension = [replaceShown, search({ top: true, createPanel: panel }), keymap.of(searchKeymap), theme];
