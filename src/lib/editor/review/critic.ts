// CriticMarkup in the text itself (a project whose suggestions are kept in the text, `review = "in-file"`), as pure
// functions: the marks read from the text, a mark accepted or rejected, and what the user's typing in suggest mode
// writes. The editor (criticMode.ts) draws the marks and runs the typing through `suggestIn`; tested in
// scripts/critic.test.ts.
//
// The marks: `{++new++}` an insertion, `{--old--}` a deletion, `{~~old~>new~~}` a replacement, `{==text==}` a
// highlight, `{>>note<<}` a comment. A comment right after another mark is that mark's note; a note that starts with
// `@XY` (alone, or `@XY: text`) says who made the mark. A highlight with a note is a comment on its text.
//
// Suggest mode, what typing writes:
// - Typing at a point outside the marks: an insertion, `{++typed++}` followed by the user's initials
//   (`{>>@MP<<}`, when set); the cursor stays inside it, so that what is typed next goes on into it.
// - A selection replaced: `{~~old~>typed~~}`, the cursor inside what it puts there.
// - Backspace or Delete outside the marks: `{--character--}`, which grows with each further one; the cursor stays
//   on the side it moves from.
// - Typing or deleting inside what an insertion or replacement puts there edits it, plainly; emptied, an insertion
//   goes and a replacement becomes a deletion.
// - Anything else that touches a mark (its delimiters, the text a deletion takes out, a selection over part of a
//   mark) is not done: the text stays as it is (`blocked`).

/** A range of the text: [from, to). */
export type Span = { from: number; to: number };

export type MarkKind = "insert" | "delete" | "replace" | "highlight" | "comment";

/** A mark of the text. `from`..`to` is all of it, the note after it included. */
export type Mark = {
  kind: MarkKind;
  from: number;
  to: number;
  /** what it takes out (delete, replace) or marks (highlight, a comment on a text) */
  old?: Span;
  /** what it puts in (insert, replace) */
  new?: Span;
  /** the comment: its text, without the author; on its own (`comment` without `old`) or after a mark */
  note?: string;
  /** the initials or name after `@` at the start of the note */
  author?: string;
};

/** A change of the text: [from, to) replaced by `insert`. */
export type Change = { from: number; to: number; insert: string };

const MARK = /\{\+\+([\s\S]*?)\+\+\}|\{--([\s\S]*?)--\}|\{~~([\s\S]*?)~>([\s\S]*?)~~\}|\{==([\s\S]*?)==\}|\{>>([\s\S]*?)<<\}/g;

/** The author and the text of a note: `@MP`, `@MP: text`, `@Jane Doe: text`; else the whole of it is the text. */
export function noteOf(body: string): { author?: string; text: string } {
  const m = /^\s*@([^\s:][^:]*?)\s*(?::\s*([\s\S]*))?$/.exec(body);
  if (!m) return { text: body.trim() };
  // `@MP some text` without a colon: the first word is the author
  if (m[2] === undefined && /\s/.test(m[1])) {
    const [who, ...rest] = m[1].split(/\s+/);
    return { author: who, text: rest.join(" ") };
  }
  return { author: m[1].trim(), text: (m[2] ?? "").trim() };
}

/** The marks of `text`, in order. */
export function parseCritic(text: string): Mark[] {
  const marks: Mark[] = [];
  for (const m of text.matchAll(MARK)) {
    const from = m.index!;
    const to = from + m[0].length;
    if (m[1] !== undefined) marks.push({ kind: "insert", from, to, new: { from: from + 3, to: to - 3 } });
    else if (m[2] !== undefined) marks.push({ kind: "delete", from, to, old: { from: from + 3, to: to - 3 } });
    else if (m[3] !== undefined) {
      const sep = from + 3 + m[3].length;
      marks.push({ kind: "replace", from, to, old: { from: from + 3, to: sep }, new: { from: sep + 2, to: to - 3 } });
    } else if (m[5] !== undefined) marks.push({ kind: "highlight", from, to, old: { from: from + 3, to: to - 3 } });
    else {
      const { author, text: note } = noteOf(m[6]);
      const before = marks[marks.length - 1];
      // a note right after a mark (that has none yet) is that mark's
      if (before && before.to === from && before.kind !== "comment" && before.note === undefined && before.author === undefined) {
        before.to = to;
        before.note = note;
        before.author = author;
        if (before.kind === "highlight") before.kind = "comment";
      } else marks.push({ kind: "comment", from, to, note, author });
    }
  }
  return marks;
}

const slice = (text: string, s: Span | undefined) => (s ? text.slice(s.from, s.to) : "");

/** The mark accepted: what it puts in stays, what it takes out goes, the marks and the note go. A comment resolved
 *  leaves the text it was on. */
export function acceptMark(text: string, mark: Mark): Change {
  const insert = mark.kind === "insert" || mark.kind === "replace" ? slice(text, mark.new) : mark.kind === "delete" ? "" : slice(text, mark.old);
  return { from: mark.from, to: mark.to, insert };
}

/** The mark rejected: the text as it was before it. A comment resolved, as `acceptMark`. */
export function rejectMark(text: string, mark: Mark): Change {
  const insert = mark.kind === "insert" ? "" : slice(text, mark.old);
  return { from: mark.from, to: mark.to, insert };
}

/** The marks whose whole range holds `pos` (its ends included). */
export function marksAt(marks: Mark[], pos: number): Mark[] {
  return marks.filter((m) => m.from <= pos && pos <= m.to);
}

/** What the user did, as suggest mode reads it from a change the editor was asked to make. */
export type Typed =
  /** typing, pasting, or a selection replaced or deleted */
  | { type: "replace"; from: number; to: number; insert: string }
  /** Backspace: [from, to) the characters before the cursor at `to` */
  | { type: "back"; from: number; to: number }
  /** Delete: [from, to) the characters after the cursor at `from` */
  | { type: "forward"; from: number; to: number };

/** What suggest mode writes for what was typed: the change, and where the cursor goes; `blocked` when the typing
 *  touches a mark in a way that cannot be a suggestion (the text stays as it is). */
export type Suggested = { change: Change; cursor: number } | { blocked: true };

/** Whether [from, to] lies in the span (its ends included). */
const within = (s: Span | undefined, from: number, to: number) => !!s && s.from <= from && to <= s.to;
/** Whether [from, to) meets the mark's range (touching its ends is not meeting it). */
const meets = (m: Mark, from: number, to: number) => (from === to ? m.from < from && from < m.to : from < m.to && m.from < to);

/** The suggestion that typing `typed` into `text` (whose marks are `marks`) makes; `initials` (empty: none) put after
 *  each new mark as its note. */
export function suggestIn(text: string, marks: Mark[], typed: Typed, initials = ""): Suggested {
  const tag = initials.trim() ? `{>>@${initials.trim()}<<}` : "";
  const { from, to } = typed;
  const insert = typed.type === "replace" ? typed.insert : "";

  // inside what an insertion or replacement puts in: edited as it is
  const editable = marks.find((m) => within(m.new, from, to));
  if (editable) {
    const now = slice(text, editable.new).slice(0, from - editable.new!.from) + insert + slice(text, editable.new).slice(to - editable.new!.from);
    if (now === "") {
      // emptied: an insertion goes, a replacement becomes a deletion
      if (editable.kind === "insert") return { change: { from: editable.from, to: editable.to, insert: "" }, cursor: editable.from };
      const old = slice(text, editable.old);
      const after = text.slice(editable.new!.to + 3, editable.to);
      const mark = `{--${old}--}${after}`;
      return { change: { from: editable.from, to: editable.to, insert: mark }, cursor: editable.from };
    }
    return { change: { from, to, insert }, cursor: from + insert.length };
  }

  // a deletion grown by Backspace just before it, or Delete just after it
  if (typed.type === "back") {
    const next = marks.find((m) => m.kind === "delete" && m.from === to);
    if (next) {
      const gone = text.slice(from, to);
      return { change: { from, to: next.old!.from, insert: `{--${gone}` }, cursor: from };
    }
  }
  if (typed.type === "forward") {
    const before = marks.find((m) => m.kind === "delete" && m.to === from);
    if (before) {
      const gone = text.slice(from, to);
      const note = text.slice(before.old!.to + 3, before.to);
      const grown = `${slice(text, before.old)}${gone}--}${note}`;
      return { change: { from: before.old!.from, to, insert: grown }, cursor: before.old!.from + grown.length };
    }
  }

  if (marks.some((m) => meets(m, from, to))) return { blocked: true };

  if (from === to) {
    if (!insert) return { blocked: true };
    // typed at a point: an insertion, the cursor inside it
    return { change: { from, to, insert: `{++${insert}++}${tag}` }, cursor: from + 3 + insert.length };
  }
  const old = text.slice(from, to);
  if (!insert) {
    // deleted: Backspace leaves the cursor before the deletion, Delete after it
    const mark = `{--${old}--}${tag}`;
    return { change: { from, to, insert: mark }, cursor: typed.type === "back" ? from : from + mark.length };
  }
  // a selection replaced: the cursor inside what it puts there
  return { change: { from, to, insert: `{~~${old}~>${insert}~~}${tag}` }, cursor: from + 3 + old.length + 2 + insert.length };
}

/** The text with every mark accepted (`accept`) or rejected: what it reads as once the review is done (a text
 *  without marks is itself). */
export function resolveAll(text: string, accept: boolean): string {
  const marks = parseCritic(text);
  let out = "";
  let at = 0;
  for (const m of marks) {
    const change = accept ? acceptMark(text, m) : rejectMark(text, m);
    out += text.slice(at, change.from) + change.insert;
    at = change.to;
  }
  return out + text.slice(at);
}

/** A comment on [from, to) of the text as marks: the text highlighted and the note after it, the initials first
 *  (`{==text==}{>>@MP: body<<}`); on a point, the note alone. */
export function commentOn(from: number, to: number, body: string, initials = ""): Change[] {
  const who = initials.trim() ? `@${initials.trim()}: ` : "";
  const note = `{>>${who}${body.trim()}<<}`;
  if (from === to) return [{ from, to, insert: note }];
  return [
    { from, to: from, insert: "{==" },
    { from: to, to, insert: `==}${note}` },
  ];
}
