// Suggest mode, as pure functions: what the user's typing does to their own suggestions while the text stays as it
// is. The editor (own.ts) reads each change of a transaction as an `Edit` and keeps the result; tested in
// scripts/suggest.test.ts.
//
// - Typing at a point: a new insertion, or more of one's own suggestion there (at its end, or anywhere in the text
//   it replaces: a deletion typed into becomes a replacement). The cursor stays at the suggestion's end, drawn after
//   what it inserts.
// - A selection replaced (or deleted): one suggestion over it, taking in one's own suggestions it covers.
// - Backspace in or at the end of one's own suggestion takes from what it inserts; with nothing inserted, or at its
//   start, the suggestion's range grows left (the cursor at its start). Elsewhere it starts a deletion of the
//   character before.
// - Delete makes the deletion grow right (the cursor at its end), or starts one of the character after.
// - One's own suggestions that come to touch are one. A suggestion emptied (nothing replaced, nothing inserted) or
//   that would leave the text as it is goes.
// - Someone else's suggestions are not the user's to change: typing in their range suggests on the text beneath.

import { findClusterBreak } from "@codemirror/state";

/** A suggestion of the user's as the editor holds it: `key` is the editor's own (the app's id may not be known yet);
 *  the range is of the editor's text, `insert` what goes there instead. */
export type Own = { key: number; from: number; to: number; insert: string };

/** What the user did, read from a change of the text the editor was asked to make. */
export type Edit =
  /** typing, pasting, dropping, or a selection replaced or deleted: [from, to] of the text replaced by `insert` */
  | { type: "replace"; from: number; to: number; insert: string }
  /** Backspace with the cursor at `at`: CodeMirror would delete `size` code units before it (more than a character:
   *  a word or to the start of the line, ⌥⌫ ⌘⌫) */
  | { type: "back"; at: number; size: number }
  /** Delete with the cursor at `at`, `size` code units after it */
  | { type: "forward"; at: number; size: number };

/** The editor's text, as much of it as is needed (a string does). */
export type Text = { slice(from: number, to?: number): string; readonly length: number };

export type Result = {
  own: Own[];
  /** where the cursor goes */
  cursor: number;
  /** the next free key */
  nextKey: number;
  /** the key of the suggestion changed (null: none, e.g. one emptied) */
  touched: number | null;
};

const byPlace = (a: Own, b: Own) => a.from - b.from || a.to - b.to || a.key - b.key;

/** The length of the character (grapheme) before `pos`, or a word with the space before it when `word`. */
function sizeBefore(text: Text, pos: number, word: boolean): number {
  const s = text.slice(Math.max(0, pos - 200), pos);
  if (!s) return 0;
  if (word) {
    const m = /(?:[\p{L}\p{N}_]+|[^\s\p{L}\p{N}_]+)?\s*$/u.exec(s);
    if (m && m[0].length) return m[0].length;
  }
  return s.length - findClusterBreak(s, s.length, false);
}

/** The length of the character after `pos`, or a word with the space after it when `word`. */
function sizeAfter(text: Text, pos: number, word: boolean): number {
  const s = text.slice(pos, Math.min(text.length, pos + 200));
  if (!s) return 0;
  if (word) {
    const m = /^\s*(?:[\p{L}\p{N}_]+|[^\s\p{L}\p{N}_]+)?/u.exec(s);
    if (m && m[0].length) return m[0].length;
  }
  return findClusterBreak(s, 0, true);
}

/** `insert` with its last character taken off, or its last word with `word`. */
function chop(insert: string, word: boolean): string {
  return insert.slice(0, insert.length - sizeBefore(insert, insert.length, word));
}

/** The user's suggestion whose range holds `pos`, ends included; the one ending there first (where the cursor is
 *  drawn after its inserted text), then the one starting there with `start`. */
function ownAt(own: Own[], pos: number, start = false): Own | undefined {
  const at = own.filter((o) => o.from <= pos && pos <= o.to);
  const ending = at.find((o) => o.to === pos);
  const starting = at.find((o) => o.from === pos);
  return (start ? (starting ?? ending) : (ending ?? starting)) ?? at[0];
}

/** `changed` put in the list in place of those it takes in (`gone`), then merged with the user's suggestions it
 *  touches or overlaps: the ranges joined, the inserted texts in the order they are drawn (by the end of each range),
 *  under the oldest key (the one the app most likely knows). Emptied suggestions and those that change nothing go. */
function settle(own: Own[], changed: Own, gone: Set<number>, text: Text): { own: Own[]; merged: Own } {
  let rest = own.filter((o) => o.key !== changed.key && !gone.has(o.key));
  let merged = changed;
  for (;;) {
    const touching = rest.filter((o) => o.to >= merged.from && o.from <= merged.to);
    if (!touching.length) break;
    const all = [merged, ...touching].sort((a, b) => a.to - b.to || a.from - b.from);
    merged = {
      key: Math.min(...all.map((o) => o.key)),
      from: Math.min(...all.map((o) => o.from)),
      to: Math.max(...all.map((o) => o.to)),
      insert: all.map((o) => o.insert).join(""),
    };
    rest = rest.filter((o) => !touching.includes(o));
  }
  const empty = merged.from === merged.to ? !merged.insert : text.slice(merged.from, merged.to) === merged.insert;
  return { own: (empty ? rest : [...rest, merged]).sort(byPlace), merged };
}

/** What `edit` does to the user's suggestions `own` over `text` (which it leaves as it is). */
export function suggestEdit(own: Own[], edit: Edit, text: Text, nextKey: number): Result {
  const done = (changed: Own, cursor: (merged: Own) => number, gone = new Set<number>()): Result => {
    const { own: after, merged } = settle(own, changed, gone, text);
    const kept = after.some((o) => o.key === merged.key);
    return { own: after, cursor: cursor(merged), nextKey, touched: kept ? merged.key : null };
  };
  const fresh = (from: number, to: number, insert: string): Own => ({ key: nextKey++, from, to, insert });

  switch (edit.type) {
    case "replace": {
      const { from, to, insert } = edit;
      if (from === to) {
        const cur = ownAt(own, from);
        return done(cur ? { ...cur, insert: cur.insert + insert } : fresh(from, from, insert), (m) => m.to);
      }
      // the user's suggestions the selection reaches: those whose inserted text is drawn inside it (at its end, or
      // past its start) are replaced; those drawn before or after it keep their text, before or after what is typed
      const reached = own.filter((o) => o.to >= from && o.from <= to);
      const before = reached.filter((o) => o.to === from);
      const after = reached.filter((o) => o.to > to);
      const inside = reached.filter((o) => !before.includes(o) && !after.includes(o));
      const all = [...before, ...inside, ...after];
      const changed: Own = {
        key: all[0]?.key ?? nextKey++,
        from: Math.min(from, ...all.map((o) => o.from)),
        to: Math.max(to, ...all.map((o) => o.to)),
        insert: [...before.map((o) => o.insert), insert, ...after.map((o) => o.insert)].join(""),
      };
      return done(changed, (m) => m.to, new Set(all.map((o) => o.key)));
    }
    case "back": {
      const { at, size } = edit;
      const word = size > sizeBefore(text, at, false);
      const cur = ownAt(own, at);
      // what it inserts is drawn before the cursor unless the cursor is at the start of the text it replaces
      if (cur?.insert && (at > cur.from || cur.from === cur.to)) return done({ ...cur, insert: chop(cur.insert, word) }, (m) => m.to);
      if (cur) {
        const grow = sizeBefore(text, cur.from, word);
        if (!grow) return { own, cursor: cur.from, nextKey, touched: cur.key };
        return done({ ...cur, from: cur.from - grow }, (m) => m.from);
      }
      if (!size) return { own, cursor: at, nextKey, touched: null };
      return done(fresh(at - size, at, ""), (m) => m.from);
    }
    case "forward": {
      const { at, size } = edit;
      const word = size > sizeAfter(text, at, false);
      const cur = ownAt(own, at, true);
      if (cur) {
        const grow = sizeAfter(text, cur.to, word);
        if (!grow) return { own, cursor: cur.to, nextKey, touched: cur.key };
        return done({ ...cur, to: cur.to + grow }, (m) => m.to);
      }
      if (!size) return { own, cursor: at, nextKey, touched: null };
      return done(fresh(at, at + size, ""), (m) => m.to);
    }
  }
}

/** The user's suggestions carried through a change of the text (`map` as CodeMirror's `mapPos`): the text inserted
 *  at either end of a range stays out of it. */
export function mapOwn(own: Own[], map: (pos: number, assoc: number) => number): Own[] {
  return own.map((o) => {
    const from = map(o.from, 1);
    const to = map(o.to, -1);
    return to < from ? { ...o, from: to, to } : { ...o, from, to };
  });
}

/** What changed between two lists of the user's suggestions: those new or changed, to send to the app, and the keys
 *  of those gone, to withdraw. */
export function diffOwn(before: Own[], after: Own[]): { suggest: Own[]; withdraw: number[] } {
  const was = new Map(before.map((o) => [o.key, o]));
  const now = new Set(after.map((o) => o.key));
  return {
    suggest: after.filter((o) => {
      const w = was.get(o.key);
      return !w || w.from !== o.from || w.to !== o.to || w.insert !== o.insert;
    }),
    withdraw: before.filter((o) => !now.has(o.key)).map((o) => o.key),
  };
}
