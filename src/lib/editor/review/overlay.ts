// The review over the text, as Word's All Markup: the suggestions and comments of everyone as decorations, never in
// the text itself. A suggestion's text is struck through and what it puts there is shown after it, both in the
// colour of its author; a comment is a light highlight of its text (a small mark where it has none). Items whose
// text changed or is gone are dimmed; those decided (accepted, rejected, resolved, withdrawn) are not shown. On hover
// a tooltip says who, when and what; a click on one tells the app (`select`).

import { Facet, StateEffect, StateField, type ChangeDesc, type EditorState, type Extension } from "@codemirror/state";
import { Decoration, EditorView, WidgetType, hoverTooltip, type DecorationSet, type Tooltip } from "@codemirror/view";
import type { ReviewHandlers, ReviewItem } from "./types.ts";

/** What the app gave the editor's review. */
export const handlersFacet = Facet.define<ReviewHandlers, ReviewHandlers | null>({ combine: (v) => v[0] ?? null });

/** A range carried through a change of the text: the text inserted at either end stays out of it. */
export function mapRange(r: { from: number; to: number }, changes: ChangeDesc): { from: number; to: number } {
  const from = changes.mapPos(r.from, 1);
  const to = changes.mapPos(r.to, -1);
  return to < from ? { from: Math.min(from, to), to: Math.min(from, to) } : { from, to };
}

/** The app's items replaced (positions of the text after the transaction). */
export const setItems = StateEffect.define<ReviewItem[]>();
/** An item no longer shown, until the app gives the items again (accepted in the editor). */
export const hideItem = StateEffect.define<string>();

/** The items the app gave, carried through the changes of the text until it gives them again. */
export const itemsField = StateField.define<ReviewItem[]>({
  create: () => [],
  update(items, tr) {
    if (tr.docChanged) items = items.map((i) => ({ ...i, at: mapRange(i.at, tr.changes) }));
    for (const e of tr.effects) {
      if (e.is(setItems)) items = e.value;
      else if (e.is(hideItem)) items = items.filter((i) => i.id !== e.value);
    }
    return items;
  },
});

/** An item as it is drawn: one of the app's, or a suggestion of the user's the editor holds. */
export type Shown = {
  /** "" for a suggestion of the user's the app has not given an id yet */
  id: string;
  kind: "suggestion" | "comment";
  /** whose: the slug, which gives the colour */
  slug: string;
  from: number;
  to: number;
  /** a suggestion's text put at `to` ("" deletes); null for a comment */
  insert: string | null;
  /** its text changed (outdated) or is gone (orphaned) */
  dim: boolean;
  /** the app's item, if it has one */
  item: ReviewItem | null;
};

/** The person's slug of an id (`<slug>-<n>`). */
export const slugOf = (id: string) => id.replace(/-\d+$/, "");

/** An item of the app as drawn, if it is: only the open ones are. */
export function shownItem(item: ReviewItem): Shown | null {
  if (item.state !== "open") return null;
  const suggestion = item.kind === "suggestion";
  return {
    id: item.id,
    kind: item.kind,
    slug: slugOf(item.id),
    from: item.at.from,
    to: item.at.to,
    insert: suggestion ? (item.replacement ?? item.record.insert ?? "") : null,
    dim: item.status.type === "outdated" || item.status.type === "orphaned",
    item,
  };
}

/** What is drawn over the text: given by the parts of the review (the app's items; the user's suggestions). */
export const shownFacet = Facet.define<(state: EditorState) => Shown[]>();

/** The ids of the app's items drawn otherwise (the user's own suggestions, from what the editor holds) or done with:
 *  not drawn from the app's items. */
export const claimedFacet = Facet.define<(state: EditorState) => ReadonlySet<string>>();

export const shownOf = (state: EditorState): Shown[] =>
  state.facet(shownFacet).flatMap((f) => f(state)).sort((a, b) => a.from - b.from || a.to - b.to);

/** The number of colours of the authors: `--review-1` … `--review-6`. */
export const COLOURS = ["#c2410c", "#7c3aed", "#0f766e", "#be185d", "#1d4ed8", "#4d7c0f"];

/** The colour the user chose for themselves (the app's settings), with their slug: the others' stay as the hash. */
let own: { slug: string; colour: number } | null = null;

/** The user `slug` shown in `colour` (1 to 6) from now on; none: from the slug, as everyone's. The editor shows it
 *  from its next items. */
export function setOwnColour(slug: string | null | undefined, colour: number | null | undefined) {
  own = slug && colour && colour >= 1 && colour <= COLOURS.length ? { slug, colour } : null;
}

/** A person's colour, 1 to 6: their own choice if it is the user, else a hash of their slug. The slug, not the name:
 *  a suggestion the app has not yet written has no item and no name, and is coloured as the person's others. */
export function colourOf(slug: string): number {
  if (own && own.slug === slug) return own.colour;
  let h = 5381;
  for (let i = 0; i < slug.length; i++) h = ((h << 5) + h + slug.charCodeAt(i)) | 0;
  return (Math.abs(h) % COLOURS.length) + 1;
}

/** A person's colour as CSS (`var(--review-n, …)`), for the app's lists to match the text. */
export const reviewColour = (slug: string) => `var(--review-${colourOf(slug)}, ${COLOURS[colourOf(slug) - 1]})`;

const classes = (s: Shown, kind: string) => `cm-review-${kind} cm-review-c${colourOf(s.slug)}${s.dim ? " cm-review-dim" : ""}`;

/** What a suggestion puts in, shown after the text it replaces. It sits before its position (side -1), so that the
 *  cursor at that position is drawn after it, where typing goes on. */
class InsertWidget extends WidgetType {
  readonly text: string;
  readonly cls: string;
  readonly id: string;
  constructor(text: string, cls: string, id: string) {
    super();
    [this.text, this.cls, this.id] = [text, cls, id];
  }
  eq(other: InsertWidget) {
    return other.text === this.text && other.cls === this.cls && other.id === this.id;
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = this.cls;
    span.textContent = this.text;
    if (this.id) span.dataset.review = this.id;
    return span;
  }
  // a click on it is the editor's (the cursor goes there) and the app's (`select`)
  ignoreEvent() {
    return false;
  }
}

/** A comment on no text: a small mark. */
class MarkerWidget extends WidgetType {
  readonly cls: string;
  readonly id: string;
  constructor(cls: string, id: string) {
    super();
    [this.cls, this.id] = [cls, id];
  }
  eq(other: MarkerWidget) {
    return other.cls === this.cls && other.id === this.id;
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = this.cls;
    span.setAttribute("aria-label", "comment");
    if (this.id) span.dataset.review = this.id;
    return span;
  }
  ignoreEvent() {
    return false;
  }
}

function decorations(state: EditorState): DecorationSet {
  const ranges = [];
  for (const s of shownOf(state)) {
    const attributes: Record<string, string> = s.id ? { "data-review": s.id } : {};
    if (s.kind === "comment") {
      if (s.from < s.to) ranges.push(Decoration.mark({ class: classes(s, "comment"), attributes }).range(s.from, s.to));
      else ranges.push(Decoration.widget({ widget: new MarkerWidget(classes(s, "marker"), s.id), side: -1 }).range(s.from));
      continue;
    }
    if (s.from < s.to) ranges.push(Decoration.mark({ class: classes(s, "del"), attributes }).range(s.from, s.to));
    if (s.insert) ranges.push(Decoration.widget({ widget: new InsertWidget(s.insert, classes(s, "ins"), s.id), side: -1 }).range(s.to));
  }
  return Decoration.set(ranges, true);
}

/** The items whose range holds `pos` (its ends included), in the order of the text. */
export function shownAt(state: EditorState, pos: number): Shown[] {
  return shownOf(state).filter((s) => s.from <= pos && pos <= s.to);
}

/** At most `n` characters of `text`, on one line. */
const clip = (text: string, n = 60) => {
  const flat = text.replace(/\s+/g, " ");
  return flat.length > n ? `${flat.slice(0, n - 1)}…` : flat;
};

function when(time: string): string {
  const date = new Date(time);
  return Number.isNaN(date.getTime()) ? time : date.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

/** What a suggestion does, in words. */
function what(state: EditorState, s: Shown): string {
  const old = state.sliceDoc(s.from, s.to);
  if (s.from === s.to) return `Insert “${clip(s.insert ?? "")}”`;
  if (!s.insert) return `Delete “${clip(old)}”`;
  return `Replace “${clip(old, 30)}” with “${clip(s.insert, 30)}”`;
}

function tipOf(state: EditorState, shown: Shown[], me: string): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "cm-review-tip";
  for (const s of shown) {
    const box = dom.appendChild(document.createElement("div"));
    box.className = "cm-review-tip-item";
    const head = box.appendChild(document.createElement("div"));
    head.className = `cm-review-tip-head cm-review-c${colourOf(s.slug)}`;
    const dot = head.appendChild(document.createElement("span"));
    dot.className = "cm-review-tip-dot";
    const author = s.item?.author ?? (s.slug === me ? "You" : s.slug);
    head.append(`${author} · ${s.item ? when(s.item.record.time) : "not yet saved"}`);
    const line = (text: string, cls = "") => {
      const div = box.appendChild(document.createElement("div"));
      if (cls) div.className = cls;
      div.textContent = text;
    };
    if (s.kind === "suggestion") line(what(state, s), "cm-review-tip-what");
    if (s.item?.record.text) line(s.item.record.text);
    const status = s.item?.status;
    if (status?.type === "outdated") line(`Outdated: the text is now “${clip(status.now)}”`, "cm-review-tip-note");
    if (status?.type === "orphaned") line("Its text is gone", "cm-review-tip-note");
    const replies = s.item?.thread.filter((r) => r.kind === "reply").length ?? 0;
    if (replies) line(`${replies} ${replies === 1 ? "reply" : "replies"}`, "cm-review-tip-note");
  }
  return dom;
}

/** The items under the mouse: a range holds the side of `pos` the mouse is on (the text inserted after a range is
 *  drawn before its end). */
const tooltip = hoverTooltip(
  (view, pos, side): Tooltip | null => {
    const shown = shownOf(view.state).filter(
      (s) => (pos > s.from || (pos === s.from && (side > 0 || s.from === s.to))) && (pos < s.to || (pos === s.to && side < 0)),
    );
    if (!shown.length) return null;
    const me = view.state.facet(handlersFacet)?.me ?? "";
    return {
      pos: Math.min(...shown.map((s) => s.from)),
      end: Math.max(...shown.map((s) => s.to)),
      above: true,
      create: () => ({ dom: tipOf(view.state, shown, me) }),
    };
  },
  { hoverTime: 350 },
);

/** A click on an item: the app's `select`. */
const clicks = EditorView.domEventHandlers({
  click(event, view) {
    const target = event.target instanceof Element ? event.target.closest("[data-review]") : null;
    const id = target?.getAttribute("data-review");
    if (id) view.state.facet(handlersFacet)?.select?.(id);
    return false;
  },
});

const colourRules = Object.fromEntries(
  COLOURS.map((c, i) => [`.cm-review-c${i + 1}`, { "--review-color": `var(--review-${i + 1}, ${c})` }]),
);

const theme = EditorView.baseTheme({
  ...colourRules,
  ".cm-review-del": {
    textDecoration: "line-through",
    textDecorationColor: "var(--review-color)",
    color: "color-mix(in srgb, var(--review-color) 75%, currentColor)",
  },
  ".cm-review-ins": {
    color: "var(--review-color)",
    textDecoration: "underline",
    textDecorationColor: "color-mix(in srgb, var(--review-color) 60%, transparent)",
    textUnderlineOffset: "2px",
    whiteSpace: "pre-wrap",
  },
  ".cm-review-comment": {
    background: "color-mix(in srgb, var(--review-color) 16%, transparent)",
    borderBottom: "2px solid color-mix(in srgb, var(--review-color) 55%, transparent)",
  },
  ".cm-review-marker": {
    display: "inline-block",
    width: "0.45em",
    height: "0.45em",
    margin: "0 1px",
    borderRadius: "50%",
    verticalAlign: "super",
    background: "var(--review-color)",
  },
  ".cm-review-dim": { opacity: "0.45" },
  ".cm-review-tip": { maxWidth: "26em", padding: "4px 8px", fontSize: "0.9em", lineHeight: "1.4" },
  ".cm-review-tip-item + .cm-review-tip-item": { marginTop: "6px", paddingTop: "6px", borderTop: "1px solid var(--line, rgba(0,0,0,.08))" },
  ".cm-review-tip-head": { fontWeight: "600", display: "flex", alignItems: "center", gap: "6px" },
  ".cm-review-tip-dot": { width: "0.6em", height: "0.6em", borderRadius: "50%", background: "var(--review-color)", flex: "none" },
  ".cm-review-tip-what": { color: "var(--review-color)" },
  ".cm-review-tip-note": { color: "var(--muted, #6e6e73)" },
});

/** The app's items, as drawn. */
const appItems = shownFacet.of((state) => {
  const claimed = state.facet(claimedFacet).map((f) => f(state));
  return state.field(itemsField).flatMap((i) => {
    const s = claimed.some((c) => c.has(i.id)) ? null : shownItem(i);
    return s ? [s] : [];
  });
});

/** The decorations, made again as the text or the review changes (any effect: cheaper to make than to tell). */
const drawn = StateField.define<DecorationSet>({
  create: decorations,
  update: (deco, tr) => (tr.docChanged || tr.effects.length || tr.reconfigured ? decorations(tr.state) : deco),
  provide: (f) => EditorView.decorations.from(f),
});

/** The overlay: the items, their decorations, the tooltip and the clicks. */
export const overlay: Extension = [
  itemsField,
  appItems,
  drawn,
  tooltip,
  clicks,
  theme,
];
