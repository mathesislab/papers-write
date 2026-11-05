// The user's own suggestions in the editor, and suggest mode: a transaction filter turns what the user types into
// their suggestions (suggest.ts) and the text does not change. The suggestions are kept in a field, carried through
// the changes of the text, sent to the app as they change (each one 400 ms after its last change; one request at a
// time for each, so that a new one is never written twice) and reconciled with the app's items by id. Undo and redo
// in suggest mode step through the field's own history of them.

import { EditorSelection, EditorState, Prec, StateEffect, StateField, Transaction, type Extension, type TransactionSpec } from "@codemirror/state";
import { EditorView, ViewPlugin, keymap, type ViewUpdate } from "@codemirror/view";
import { modeField, suggesting } from "./mode.ts";
import { claimedFacet, handlersFacet, hideItem, itemsField, setItems, shownFacet, type Shown } from "./overlay.ts";
import { mapOwn, suggestEdit, type Edit, type Own, type Result } from "./suggest.ts";
import type { ReviewItem } from "./types.ts";

/** The suggestions as they were, and where the cursor was: a step of the history of suggest mode. */
type Snap = { own: Own[]; cursor: number };

export type OwnState = {
  /** the user's suggestions, in the order of the text */
  own: Own[];
  nextKey: number;
  /** the app's id of each (by key), once known */
  ids: ReadonlyMap<number, string>;
  /** what the app has of each, as last answered or given */
  sent: ReadonlyMap<number, Own>;
  /** those asked of the app and not yet answered */
  busy: ReadonlySet<number>;
  /** ids the app gave that were since replaced or withdrawn: not taken from its items again */
  retired: ReadonlySet<string>;
  /** ids of the user's seen in the app's items */
  seen: ReadonlySet<string>;
  done: readonly Snap[];
  undone: readonly Snap[];
  /** the last edit, to run typing together into one step */
  last: { key: number | null; kind: string; time: number } | null;
};

const EMPTY: OwnState = {
  own: [],
  nextKey: 1,
  ids: new Map(),
  sent: new Map(),
  busy: new Set(),
  retired: new Set(),
  seen: new Set(),
  done: [],
  undone: [],
  last: null,
};

/** Typing in suggest mode: the suggestions after it. */
const editOwn = StateEffect.define<{ result: Result; kind: string }>();
/** A step back (-1) or forward (1) in the history of suggest mode. */
const stepOwn = StateEffect.define<-1 | 1>();
/** A suggestion asked of the app (suggest or withdraw). */
const asking = StateEffect.define<number>();
/** The app's answer: the id it gave and what was sent (none: it failed). */
const answered = StateEffect.define<{ key: number; id: string; sent: Own } | { key: number; failed: true }>();
/** A suggestion withdrawn by the app: its key and id done with. */
const forget = StateEffect.define<{ key: number; id: string }>();

/** Typing within this long of the last, into the same suggestion, is one step of the history. */
const GROUP = 500;
const DEPTH = 200;

const same = (a: Own | undefined, b: Own | undefined) => a?.from === b?.from && a?.to === b?.to && a?.insert === b?.insert;
const byPlace = (a: Own, b: Own) => a.from - b.from || a.to - b.to || a.key - b.key;
const slugOf = (id: string) => id.replace(/-\d+$/, "");

/** Whether the user's suggestion `key` is being changed: the app's items do not overwrite it. */
function dirty(st: OwnState, key: number): boolean {
  return st.busy.has(key) || !same(st.own.find((o) => o.key === key), st.sent.get(key));
}

/** The app's items taken in: the user's open suggestions in place become theirs to type into (unless being typed
 *  into); those decided, out of place or gone (having been seen) leave the field, shown by the app's items. */
function reconcile(st: OwnState, items: ReviewItem[], me: string): OwnState {
  const keyOf = new Map([...st.ids].map(([k, id]) => [id, k]));
  const byId = new Map(items.map((i) => [i.id, i]));
  let own = [...st.own];
  const ids = new Map(st.ids);
  const sent = new Map(st.sent);
  const seen = new Set(st.seen);
  const gone = new Set<number>();
  let nextKey = st.nextKey;
  const drop = (k: number) => {
    own = own.filter((o) => o.key !== k);
    ids.delete(k);
    sent.delete(k);
    gone.add(k);
  };
  for (const i of items) {
    if (i.kind !== "suggestion" || slugOf(i.id) !== me || st.retired.has(i.id)) continue;
    seen.add(i.id);
    const live = i.state === "open" && (i.status.type === "exact" || i.status.type === "moved");
    const k = keyOf.get(i.id);
    if (k !== undefined && dirty(st, k)) continue;
    if (!live) {
      if (k !== undefined) drop(k);
      continue;
    }
    const key = k ?? nextKey++;
    const now: Own = { key, from: i.at.from, to: i.at.to, insert: i.replacement ?? i.record.insert ?? "" };
    own = [...own.filter((o) => o.key !== key), now];
    ids.set(key, i.id);
    sent.set(key, now);
  }
  for (const [k, id] of st.ids) if (!byId.has(id) && st.seen.has(id) && !dirty(st, k)) drop(k);
  const purge = (snaps: readonly Snap[]) => (gone.size ? snaps.map((s) => ({ ...s, own: s.own.filter((o) => !gone.has(o.key)) })) : snaps);
  return { ...st, own: own.sort(byPlace), ids, sent, seen, nextKey, done: purge(st.done), undone: purge(st.undone) };
}

export const ownField = StateField.define<OwnState>({
  create: () => EMPTY,
  update(st, tr) {
    if (tr.docChanged) {
      const map = (own: Own[]) => mapOwn(own, (pos, assoc) => tr.changes.mapPos(pos, assoc));
      const snaps = (s: readonly Snap[]) => s.map((x) => ({ own: map(x.own), cursor: tr.changes.mapPos(x.cursor) }));
      st = {
        ...st,
        own: map(st.own),
        sent: new Map(map([...st.sent.values()]).map((o) => [o.key, o])),
        done: snaps(st.done),
        undone: snaps(st.undone),
      };
    }
    for (const e of tr.effects) {
      if (e.is(editOwn)) {
        const { result, kind } = e.value;
        const time = tr.annotation(Transaction.time) ?? Date.now();
        const joined = st.last && st.last.kind === kind && st.last.key === result.touched && time - st.last.time < GROUP;
        const done = joined ? st.done : [...st.done, { own: st.own, cursor: tr.startState.selection.main.head }].slice(-DEPTH);
        st = { ...st, own: result.own, nextKey: result.nextKey, done, undone: joined ? st.undone : [], last: { key: result.touched, kind, time } };
      } else if (e.is(stepOwn)) {
        const back = e.value < 0;
        const from = back ? st.done : st.undone;
        const snap = from[from.length - 1];
        if (!snap) continue;
        const now = { own: st.own, cursor: tr.startState.selection.main.head };
        st = {
          ...st,
          own: snap.own,
          done: back ? st.done.slice(0, -1) : [...st.done, now],
          undone: back ? [...st.undone, now] : st.undone.slice(0, -1),
          last: null,
        };
      } else if (e.is(asking)) {
        st = { ...st, busy: new Set(st.busy).add(e.value) };
      } else if (e.is(answered)) {
        const a = e.value;
        const busy = new Set(st.busy);
        busy.delete(a.key);
        st = { ...st, busy };
        if ("failed" in a) continue;
        const old = st.ids.get(a.key);
        st = {
          ...st,
          ids: new Map(st.ids).set(a.key, a.id),
          sent: new Map(st.sent).set(a.key, a.sent),
          retired: old && old !== a.id ? new Set(st.retired).add(old) : st.retired,
        };
      } else if (e.is(forget)) {
        const { key, id } = e.value;
        const [ids, sent, busy] = [new Map(st.ids), new Map(st.sent), new Set(st.busy)];
        ids.delete(key);
        sent.delete(key);
        busy.delete(key);
        st = { ...st, ids, sent, busy, retired: new Set(st.retired).add(id) };
      } else if (e.is(hideItem)) {
        // accepted: the app writes the decision; nothing to send or withdraw
        const key = [...st.ids].find(([, id]) => id === e.value)?.[0];
        if (key === undefined) continue;
        const [ids, sent] = [new Map(st.ids), new Map(st.sent)];
        ids.delete(key);
        sent.delete(key);
        const out = (snaps: readonly Snap[]) => snaps.map((x) => ({ ...x, own: x.own.filter((o) => o.key !== key) }));
        st = {
          ...st,
          own: st.own.filter((o) => o.key !== key),
          ids,
          sent,
          retired: new Set(st.retired).add(e.value),
          done: out(st.done),
          undone: out(st.undone),
        };
      } else if (e.is(setItems)) {
        const me = tr.state.facet(handlersFacet)?.me;
        if (me) st = reconcile(st, e.value, me);
      }
    }
    return st;
  },
});

/** What a change of a transaction is, as typing: Backspace and Delete at the cursor, or text replaced. */
function editOf(tr: Transaction, fromA: number, toA: number, insert: string): Edit {
  const sel = tr.startState.selection.main;
  if (!insert && sel.empty && tr.isUserEvent("delete.backward") && toA === sel.head) return { type: "back", at: toA, size: toA - fromA };
  if (!insert && sel.empty && tr.isUserEvent("delete.forward") && fromA === sel.head) return { type: "forward", at: fromA, size: toA - fromA };
  return { type: "replace", from: fromA, to: toA, insert };
}

/** A step of the history of suggest mode, as a transaction (null: nothing to step to). */
function stepSpec(state: EditorState, dir: -1 | 1): TransactionSpec | null {
  const st = state.field(ownField, false);
  const stack = dir < 0 ? st?.done : st?.undone;
  const snap = stack?.[stack.length - 1];
  if (!snap) return null;
  return {
    effects: stepOwn.of(dir),
    selection: EditorSelection.cursor(Math.min(snap.cursor, state.doc.length)),
    scrollIntoView: true,
    userEvent: dir < 0 ? "review.undo" : "review.redo",
  };
}

/** In suggest mode, what the user types (input, delete, move: not the app's own changes, nor an accepted suggestion)
 *  becomes their suggestions and the text stays as it is; the history of the text is not stepped through. */
const filter = EditorState.transactionFilter.of((tr) => {
  if (!tr.docChanged || !suggesting(tr.startState)) return tr;
  if (tr.isUserEvent("undo") || tr.isUserEvent("redo")) return stepSpec(tr.startState, tr.isUserEvent("undo") ? -1 : 1) ?? {};
  if (!(tr.isUserEvent("input") || tr.isUserEvent("delete") || tr.isUserEvent("move"))) return tr;
  const start = tr.startState;
  const st = start.field(ownField);
  const text = { slice: (from: number, to?: number) => start.sliceDoc(from, to), length: start.doc.length };
  let result: Result = { own: st.own, cursor: start.selection.main.head, nextKey: st.nextKey, touched: null };
  // the changes are all of the text as it was (it does not change), one after the other
  tr.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
    const r = suggestEdit(result.own, editOf(tr, fromA, toA, inserted.toString()), text, result.nextKey);
    result = { ...r, touched: r.touched ?? result.touched };
  });
  return {
    effects: editOwn.of({ result, kind: tr.isUserEvent("delete") ? "delete" : "input" }),
    // drawn after what is inserted there (the inserted text sits before its position)
    selection: EditorSelection.cursor(result.cursor, 1),
    scrollIntoView: true,
    userEvent: "review.suggest",
    annotations: Transaction.time.of(tr.annotation(Transaction.time) ?? Date.now()),
  };
});

/** Undo (-1) or redo (1) in suggest mode: true there, whether or not there was a step; false in edit mode, for the
 *  history of the text. */
export function stepSuggestions(view: EditorView, dir: -1 | 1): boolean {
  if (!suggesting(view.state)) return false;
  const spec = stepSpec(view.state, dir);
  if (spec) view.dispatch(spec);
  return true;
}

const keys = Prec.high(
  keymap.of([
    { key: "Mod-z", run: (v) => stepSuggestions(v, -1), preventDefault: true },
    { key: "Mod-Shift-z", run: (v) => stepSuggestions(v, 1), preventDefault: true },
    { key: "Mod-y", run: (v) => stepSuggestions(v, 1), preventDefault: true },
  ]),
);

/** Each suggestion 400 ms after its last change. */
const DELAY = 400;

/** The keys of the suggestions whose app's copy differs from the field's (not while asked). */
function unsent(st: OwnState): number[] {
  const keys = new Set([...st.own.map((o) => o.key), ...st.ids.keys()]);
  return [...keys].filter((k) => {
    if (st.busy.has(k)) return false;
    const want = st.own.find((o) => o.key === k);
    return want ? !same(want, st.sent.get(k)) : st.ids.has(k);
  });
}

/** The suggestions sent to the app as they change: `suggest` (with the id once known; the answer gives the id), or
 *  `withdraw` for one typed away. One request at a time for each; what changes meanwhile goes after the answer. Apart
 *  from the view (`state` and `dispatch` are the editor's) to be tested. */
export class Sender {
  timers = new Map<number, ReturnType<typeof setTimeout>>();
  /** those the app failed to take, sent again only once changed */
  failed = new Set<number>();
  gone = false;
  readonly state: () => EditorState;
  readonly send: (spec: TransactionSpec) => void;
  constructor(state: () => EditorState, send: (spec: TransactionSpec) => void) {
    [this.state, this.send] = [state, send];
  }
  /** The editor's state changed from `before`. */
  update(before: EditorState) {
    const a = before.field(ownField, false);
    const b = this.state().field(ownField, false);
    if (!a || !b || a === b) return;
    for (const k of unsent(b)) {
      const changed = !same(a.own.find((o) => o.key === k), b.own.find((o) => o.key === k));
      if (changed) this.failed.delete(k);
      if (changed || (!this.timers.has(k) && !this.failed.has(k))) this.schedule(k);
    }
  }
  schedule(k: number) {
    clearTimeout(this.timers.get(k));
    this.timers.set(k, setTimeout(() => this.flush(k), DELAY));
  }
  dispatch(spec: TransactionSpec) {
    if (!this.gone) this.send(spec);
  }
  flush(k: number) {
    this.timers.delete(k);
    const state = this.state();
    const handlers = state.facet(handlersFacet);
    const st = state.field(ownField, false);
    if (!handlers || !st || st.busy.has(k)) return;
    const want = st.own.find((o) => o.key === k);
    const id = st.ids.get(k);
    const fail = (e: unknown) => {
      console.error("review:", e);
      this.failed.add(k);
      this.dispatch({ effects: answered.of({ key: k, failed: true }) });
    };
    if (!want) {
      if (!id) return;
      this.dispatch({ effects: asking.of(k) });
      handlers.withdraw(id).then(() => this.dispatch({ effects: forget.of({ key: k, id }) }), fail);
      return;
    }
    if (same(want, st.sent.get(k))) return;
    this.dispatch({ effects: asking.of(k) });
    handlers
      .suggest({ ...(id ? { id } : {}), from: want.from, to: want.to, insert: want.insert })
      .then((newId) => this.dispatch({ effects: answered.of({ key: k, id: newId, sent: want }) }), fail);
  }
  /** What is typed goes out before the editor does (the answers come to no editor). */
  destroy() {
    this.gone = true;
    for (const k of [...this.timers.keys()]) {
      clearTimeout(this.timers.get(k));
      this.flush(k);
    }
  }
}

const sync = ViewPlugin.define((view) => {
  const sender = new Sender(() => view.state, (spec) => view.dispatch(spec));
  return {
    update: (u: ViewUpdate) => sender.update(u.startState),
    destroy: () => sender.destroy(),
  };
});

/** The user's suggestions as drawn: the app's item of each, where it has one, for the tooltip. */
const shown = shownFacet.of((state) => {
  const st = state.field(ownField, false);
  const me = state.facet(handlersFacet)?.me ?? "";
  if (!st) return [];
  const items = new Map(state.field(itemsField, false)?.map((i) => [i.id, i]));
  return st.own.map((o): Shown => {
    const id = st.ids.get(o.key) ?? "";
    return { id, kind: "suggestion", slug: me, from: o.from, to: o.to, insert: o.insert, dim: false, item: items.get(id) ?? null };
  });
});

/** The ids the field draws (and those done with): the overlay leaves the app's items of them out. */
const claimed = claimedFacet.of((state) => {
  const st = state.field(ownField, false);
  return new Set([...(st?.ids.values() ?? []), ...(st?.retired ?? [])]);
});

/** Suggest mode and the user's suggestions. */
export const suggestMode: Extension = [modeField, ownField, filter, keys, sync, shown, claimed];
