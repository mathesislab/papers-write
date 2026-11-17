// The review kept in the text itself (CriticMarkup, critic.ts) in the editor: the marks drawn in their author's colour
// (the delimiters and notes dimmed), a tooltip on each with Accept and Reject (Resolve for a comment), and suggest mode
// writing what is typed as marks. The text is the file's, marks and all: accepting or rejecting is an edit of it,
// undone as any.

import { EditorState, RangeSetBuilder, StateField, type Extension, type Transaction, type TransactionSpec } from "@codemirror/state";
import { Decoration, EditorView, hoverTooltip, type DecorationSet, type Tooltip } from "@codemirror/view";
import { acceptMark, marksAt, parseCritic, rejectMark, suggestIn, type Mark, type Typed } from "./critic.ts";
import { modeField, suggesting } from "./mode.ts";
import { colourOf, theme } from "./overlay.ts";

/** The marks of the text, read again as it changes. */
export const marksField = StateField.define<Mark[]>({
  create: (state) => parseCritic(state.doc.toString()),
  update: (marks, tr) => (tr.docChanged ? parseCritic(tr.newDoc.toString()) : marks),
});

/** The marks of the editor's text. */
export const criticMarks = (state: EditorState): Mark[] => state.field(marksField, false) ?? [];

/** Where a mark's own part ends, before its note. */
function mainEnd(m: Mark): number {
  const last = m.new ?? m.old;
  return last ? last.to + 3 : m.from;
}

const deco = (cls: string) => Decoration.mark({ class: cls });

function decorationsOf(state: EditorState): DecorationSet {
  const ranges: [number, number, Decoration][] = [];
  for (const m of criticMarks(state)) {
    const c = ` cm-review-c${colourOf(m.author ?? "")}`;
    const delim = deco("cm-critic-delim" + c);
    const add = (from: number, to: number, d: Decoration) => from < to && ranges.push([from, to, d]);
    if (m.kind === "insert") {
      add(m.from, m.new!.from, delim);
      add(m.new!.from, m.new!.to, deco("cm-review-ins" + c));
      add(m.new!.to, m.new!.to + 3, delim);
    } else if (m.kind === "delete") {
      add(m.from, m.old!.from, delim);
      add(m.old!.from, m.old!.to, deco("cm-review-del" + c));
      add(m.old!.to, m.old!.to + 3, delim);
    } else if (m.kind === "replace") {
      add(m.from, m.old!.from, delim);
      add(m.old!.from, m.old!.to, deco("cm-review-del" + c));
      add(m.old!.to, m.new!.from, delim);
      add(m.new!.from, m.new!.to, deco("cm-review-ins" + c));
      add(m.new!.to, m.new!.to + 3, delim);
    } else if (m.old) {
      add(m.from, m.old.from, delim);
      add(m.old.from, m.old.to, deco("cm-review-comment" + c));
      add(m.old.to, m.old.to + 3, delim);
    }
    // the note: after the mark, or the whole of a comment on its own
    add(m.kind === "comment" && !m.old ? m.from : mainEnd(m), m.to, deco("cm-critic-note" + c));
  }
  ranges.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  const builder = new RangeSetBuilder<Decoration>();
  for (const [from, to, d] of ranges) builder.add(from, to, d);
  return builder.finish();
}

const drawn = StateField.define<DecorationSet>({
  create: decorationsOf,
  update: (set, tr) => (tr.docChanged ? decorationsOf(tr.state) : set),
  provide: (f) => EditorView.decorations.from(f),
});

/** The mark accepted or rejected (a comment resolved: either), as an edit of the text. */
export function decideMark(view: EditorView, mark: Mark, accept: boolean) {
  const text = view.state.doc.toString();
  const change = accept ? acceptMark(text, mark) : rejectMark(text, mark);
  view.dispatch({ changes: change, userEvent: accept ? "review.accept" : "review.reject" });
}

/** Every mark accepted or rejected (Accept All, Reject All), as one edit. False when there is none. */
export function decideAll(view: EditorView, accept: boolean): boolean {
  const text = view.state.doc.toString();
  const marks = criticMarks(view.state);
  if (!marks.length) return false;
  view.dispatch({ changes: marks.map((m) => (accept ? acceptMark(text, m) : rejectMark(text, m))), userEvent: accept ? "review.accept" : "review.reject" });
  return true;
}

/** The marks at the cursor. */
export const marksHere = (state: EditorState): Mark[] => marksAt(criticMarks(state), state.selection.main.head);

const KIND: Record<Mark["kind"], string> = {
  insert: "Insertion",
  delete: "Deletion",
  replace: "Replacement",
  highlight: "Highlight",
  comment: "Comment",
};

function tipOf(view: EditorView, marks: Mark[]): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "cm-review-tip";
  for (const m of marks) {
    const box = dom.appendChild(document.createElement("div"));
    box.className = "cm-review-tip-item";
    const head = box.appendChild(document.createElement("div"));
    head.className = `cm-review-tip-head cm-review-c${colourOf(m.author ?? "")}`;
    head.appendChild(document.createElement("span")).className = "cm-review-tip-dot";
    head.append(m.author ? `${KIND[m.kind]} · ${m.author}` : KIND[m.kind]);
    if (m.note) box.appendChild(document.createElement("div")).textContent = m.note;
    const row = box.appendChild(document.createElement("div"));
    row.className = "cm-review-tip-actions";
    const button = (label: string, accept: boolean, primary = false) => {
      const b = row.appendChild(document.createElement("button"));
      b.type = "button";
      b.textContent = label;
      if (primary) b.className = "cm-review-tip-primary";
      b.onmousedown = (e) => e.preventDefault();
      b.onclick = () => {
        // the mark as it is now (the text may have changed since the tooltip opened)
        const now = criticMarks(view.state).find((n) => n.from === m.from && n.kind === m.kind);
        if (now) decideMark(view, now, accept);
        view.focus();
      };
    };
    if (m.kind === "comment" || m.kind === "highlight") button("Resolve", true, true);
    else {
      button("Accept", true, true);
      button("Reject", false);
    }
  }
  return dom;
}

const tooltip = hoverTooltip(
  (view, pos): Tooltip | null => {
    const marks = marksAt(criticMarks(view.state), pos);
    if (!marks.length) return null;
    return {
      pos: Math.min(...marks.map((m) => m.from)),
      end: Math.max(...marks.map((m) => m.to)),
      above: true,
      create: () => ({ dom: tipOf(view, marks) }),
    };
  },
  { hoverTime: 350, hideOnChange: true },
);

/** What the user did, read from a transaction; none when it is not typing (an undo, an edit of the app's). */
function typedOf(tr: Transaction): Typed | null {
  if (!tr.isUserEvent("input") && !tr.isUserEvent("delete")) return null;
  const changes: { from: number; to: number; insert: string }[] = [];
  tr.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => changes.push({ from: fromA, to: toA, insert: inserted.toString() }));
  if (changes.length !== 1) return null;
  const [c] = changes;
  const sel = tr.startState.selection.main;
  if (!c.insert && sel.empty && tr.isUserEvent("delete.backward")) return { type: "back", from: c.from, to: c.to };
  if (!c.insert && sel.empty && tr.isUserEvent("delete.forward")) return { type: "forward", from: c.from, to: c.to };
  return { type: "replace", ...c };
}

/** Suggest mode: typing written as marks (critic.ts `suggestIn`); typing that cannot be a suggestion is not done. */
function filter(initials: () => string) {
  return EditorState.transactionFilter.of((tr): TransactionSpec | readonly TransactionSpec[] => {
    if (!tr.docChanged || !suggesting(tr.startState)) return tr;
    const typed = typedOf(tr);
    // typing with several cursors, or more than one change at once: not done
    if (!typed) return tr.isUserEvent("input") || tr.isUserEvent("delete") ? {} : tr;
    const s = suggestIn(tr.startState.doc.toString(), criticMarks(tr.startState), typed, initials());
    if ("blocked" in s) return {};
    return { changes: s.change, selection: { anchor: s.cursor }, userEvent: "input.suggest", scrollIntoView: true };
  });
}

const criticTheme = EditorView.baseTheme({
  ".cm-critic-delim": { opacity: "0.4", fontSize: "0.85em" },
  ".cm-critic-note": { opacity: "0.55", fontSize: "0.85em", fontStyle: "italic", color: "var(--review-color)" },
});

/** The review in the text: the marks drawn, their tooltips, suggest mode writing marks with `initials` (empty: none)
 *  after each new one. */
export function criticReview(initials: () => string): Extension {
  return [modeField, marksField, drawn, tooltip, filter(initials), theme, criticTheme];
}
