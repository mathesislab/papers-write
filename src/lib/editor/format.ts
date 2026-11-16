// Formatting at the selection, as the toolbar and the Format menu ask: bold, italic, code and a link around the text
// selected (or taken off when it is already so), and a heading, a list or a quotation for the lines selected.
// Markdown, or LaTeX in a .tex file: \textbf, \emph, \texttt, \href; \section, \subsection, \subsubsection; itemize,
// enumerate and quote. A cursor inside a word formats the word; elsewhere the marks go in around it, to type into.
// The change goes in as typing does (`input.format`): in suggest mode it is the user's suggestion.

import { ensureSyntaxTree, indentUnit, syntaxTree } from "@codemirror/language";
import { EditorSelection, type ChangeSet, type ChangeSpec, type EditorState, type SelectionRange, type TransactionSpec } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";

/** What the toolbar and the Format menu ask. */
export type Format = "bold" | "italic" | "code" | "link" | "heading" | "bullet" | "numbered" | "quote";

type Inline = "bold" | "italic" | "code";
type Block = "bullet" | "numbered" | "quote";
type Node = ReturnType<typeof syntaxTree>["topNode"];

/** The marks of each inline format, Markdown and LaTeX, and the Markdown node they make. */
const INLINE: Record<Inline, { md: string; tex: string; node: string }> = {
  bold: { md: "**", tex: "\\textbf{", node: "StrongEmphasis" },
  italic: { md: "*", tex: "\\emph{", node: "Emphasis" },
  code: { md: "`", tex: "\\texttt{", node: "InlineCode" },
};

/** The environments of the LaTeX blocks. */
const ENV: Record<Block, string> = { bullet: "itemize", numbered: "enumerate", quote: "quote" };

const URL = /^(?:https?:\/\/|www\.|doi:|mailto:)\S+$/i;
const MD_HEADING = /^(#{1,6})(?=[ \t]|$)[ \t]*/;
const TEX_HEADING = /^([ \t]*)\\((?:sub){0,2})section(\*?)\{/;
const MD_BULLET = /^([ \t]*)[-*+][ \t]+/;
const MD_NUMBER = /^([ \t]*)\d+[.)][ \t]+/;
const MD_QUOTE = /^[ \t]*>[ \t]?/;

/** The change a format makes at the selection; null when there is nothing to do. */
export function formatSpec(state: EditorState, what: Format, latex: boolean): TransactionSpec | null {
  switch (what) {
    case "bold":
    case "italic":
    case "code":
      return byRange(state, (r) => (latex ? texInline(state, r, INLINE[what].tex) : mdInline(state, r, what)));
    case "link":
      return byRange(state, (r) => (latex ? texLink(state, r) : mdLink(state, r)));
    case "heading":
      return latex ? texHeading(state) : mdHeading(state);
    default:
      return latex ? texBlock(state, what) : mdBlock(state, what);
  }
}

/** The format applied in the editor, as typing, and the keys back to it. */
export function format(view: EditorView, what: Format, latex: boolean): boolean {
  const spec = formatSpec(view.state, what, latex);
  if (spec) view.dispatch({ ...spec, scrollIntoView: true, userEvent: "input.format" });
  view.focus();
  return !!spec;
}

/** What a format does for one range: its changes (in the text as it is), and the range after them. */
type Local = { changes: ChangeSpec[]; range: (changes: ChangeSet) => SelectionRange };

function byRange(state: EditorState, f: (r: SelectionRange) => Local): TransactionSpec {
  return state.changeByRange((r) => {
    const local = f(r);
    const changes = state.changes(local.changes);
    return { changes, range: local.range(changes) };
  });
}

/** The range mapped through the changes, as it was turned. */
const mapped = (r: SelectionRange) => (c: ChangeSet) => EditorSelection.range(c.mapPos(r.anchor, 1), c.mapPos(r.head, 1));

/** A range of `from` to `to`, turned as `r` is. */
const like = (r: SelectionRange, from: number, to: number) => (r.anchor <= r.head ? EditorSelection.range(from, to) : EditorSelection.range(to, from));

/** The selection without the spaces at its ends. */
function trimmed(state: EditorState, from: number, to: number): [number, number] {
  const text = state.sliceDoc(from, to);
  const start = text.length - text.trimStart().length;
  const end = text.trimEnd().length;
  return end > start ? [from + start, from + end] : [from, from];
}

/** The word a cursor is inside of (letters on both sides), else null. */
function wordAround(state: EditorState, pos: number): [number, number] | null {
  const word = state.wordAt(pos);
  return word && word.from < pos && pos < word.to ? [word.from, word.to] : null;
}

/** Marks around the selection (trimmed), or the word of a cursor; at a cursor elsewhere, both marks with the cursor
 *  between them. The selection stays on the text, inside the marks. */
function wrap(state: EditorState, r: SelectionRange, open: string, close: string): Local {
  const [a, b] = r.empty ? (wordAround(state, r.from) ?? [r.from, r.from]) : trimmed(state, r.from, r.to);
  const changes = [
    { from: a, insert: open },
    { from: b, insert: close },
  ];
  if (r.empty) return { changes, range: () => EditorSelection.cursor(r.from + open.length) };
  return { changes, range: () => like(r, a + open.length, b + open.length) };
}

function tree(state: EditorState, to: number) {
  return ensureSyntaxTree(state, to, 100) ?? syntaxTree(state);
}

/** The innermost node `name` holding the range: around a cursor strictly inside it, around a selection within it. */
function around(state: EditorState, r: SelectionRange, name: string): Node | null {
  for (let n: Node | null = tree(state, r.to).resolveInner(r.from, 1); n; n = n.parent) {
    if (n.name !== name) continue;
    if (r.empty ? n.from < r.from && r.from < n.to : n.from <= r.from && r.to <= n.to) return n;
  }
  return null;
}

function mdInline(state: EditorState, r: SelectionRange, what: Inline): Local {
  const mark = INLINE[what].md;
  const node = around(state, r, INLINE[what].node);
  const [first, last] = [node?.firstChild, node?.lastChild];
  if (node && first && last && first !== last && /Mark$/.test(first.name) && /Mark$/.test(last.name)) {
    return {
      changes: [
        { from: first.from, to: first.to },
        { from: last.from, to: last.to },
      ],
      range: mapped(r),
    };
  }
  // the marks typed with nothing between them (`**|**`): taken off again
  if (r.empty) {
    const at = r.from;
    const [before, after] = [state.sliceDoc(at - mark.length, at), state.sliceDoc(at, at + mark.length)];
    const [outBefore, outAfter] = [state.sliceDoc(at - mark.length - 1, at - mark.length), state.sliceDoc(at + mark.length, at + mark.length + 1)];
    if (before === mark && after === mark && outBefore !== mark[0] && outAfter !== mark[0]) {
      return { changes: [{ from: at - mark.length, to: at + mark.length }], range: () => EditorSelection.cursor(at - mark.length) };
    }
  }
  return wrap(state, r, mark, mark);
}

/** The `}` that closes the `{` at `brace`; null if none near. A backslash escapes what follows it. */
function closing(state: EditorState, brace: number, limit = 20000): number | null {
  const text = state.sliceDoc(brace, Math.min(state.doc.length, brace + limit));
  if (text[0] !== "{") return null;
  let depth = 0;
  for (let k = 0; k < text.length; k++) {
    const c = text[k];
    if (c === "\\") k++;
    else if (c === "{") depth++;
    else if (c === "}" && --depth === 0) return brace + k;
  }
  return null;
}

/** The nearest command `open` (`\textbf{`) before the range whose argument holds it: where it starts, and its `}`. */
function enclosing(state: EditorState, r: SelectionRange, open: string): { start: number; end: number } | null {
  const lo = Math.max(0, r.from - 4000);
  const before = state.sliceDoc(lo, r.from);
  for (let i = before.lastIndexOf(open); i >= 0; i = i > 0 ? before.lastIndexOf(open, i - 1) : -1) {
    const start = lo + i;
    const end = closing(state, start + open.length - 1);
    if (end !== null && end >= r.to) return { start, end };
  }
  return null;
}

function texInline(state: EditorState, r: SelectionRange, open: string): Local {
  // the selection is the command, or lies in its argument
  const text = state.sliceDoc(r.from, r.to);
  const whole = text.startsWith(open) && closing(state, r.from + open.length - 1) === r.to - 1 ? { start: r.from, end: r.to - 1 } : null;
  const found = whole ?? enclosing(state, r, open);
  if (found) {
    return {
      changes: [
        { from: found.start, to: found.start + open.length },
        { from: found.end, to: found.end + 1 },
      ],
      range: mapped(r),
    };
  }
  return wrap(state, r, open, "}");
}

function mdLink(state: EditorState, r: SelectionRange): Local {
  const node = around(state, r, "Link");
  const marks: Node[] = [];
  for (let c = node?.firstChild; c; c = c.nextSibling) if (c.name === "LinkMark") marks.push(c);
  if (node && marks.length >= 2) {
    const text = state.sliceDoc(marks[0].to, marks[1].from);
    return {
      changes: [{ from: node.from, to: node.to, insert: text }],
      range: () => like(r, node.from, node.from + text.length),
    };
  }
  const [a, b] = trimmed(state, r.from, r.to);
  const text = state.sliceDoc(a, b);
  if (!text) return { changes: [{ from: r.from, insert: "[]()" }], range: () => EditorSelection.cursor(r.from + 1) };
  // a URL selected is the target, its text to type; other text is the text, the target to type
  if (URL.test(text)) return { changes: [{ from: a, to: b, insert: `[](${text})` }], range: () => EditorSelection.cursor(a + 1) };
  return { changes: [{ from: a, to: b, insert: `[${text}]()` }], range: () => EditorSelection.cursor(a + text.length + 3) };
}

function texLink(state: EditorState, r: SelectionRange): Local {
  const open = "\\href{";
  const lo = Math.max(0, r.from - 4000);
  const before = state.sliceDoc(lo, r.to);
  for (let i = before.lastIndexOf(open); i >= 0; i = i > 0 ? before.lastIndexOf(open, i - 1) : -1) {
    const start = lo + i;
    if (start > r.from) continue;
    const first = closing(state, start + open.length - 1);
    const end = first === null ? null : closing(state, first + 1);
    if (first === null || end === null || end < r.to) continue;
    const text = state.sliceDoc(first + 2, end);
    return { changes: [{ from: start, to: end + 1, insert: text }], range: () => like(r, start, start + text.length) };
  }
  const [a, b] = trimmed(state, r.from, r.to);
  const text = state.sliceDoc(a, b);
  if (!text) return { changes: [{ from: r.from, insert: "\\href{}{}" }], range: () => EditorSelection.cursor(r.from + open.length) };
  if (URL.test(text)) return { changes: [{ from: a, to: b, insert: `\\href{${text}}{}` }], range: () => EditorSelection.cursor(a + open.length + text.length + 2) };
  return { changes: [{ from: a, to: b, insert: `\\href{}{${text}}` }], range: () => EditorSelection.cursor(a + open.length) };
}

/** The numbers of the lines the selection touches (a selection ending at the start of a line leaves that line out). */
function touched(state: EditorState): number[] {
  const lines = new Set<number>();
  for (const r of state.selection.ranges) {
    const first = state.doc.lineAt(r.from).number;
    let last = state.doc.lineAt(r.to).number;
    if (!r.empty && last > first && state.doc.line(last).from === r.to) last--;
    for (let n = first; n <= last; n++) lines.add(n);
  }
  return [...lines].sort((a, b) => a - b);
}

/** Changes to lines, the selection carried through them: a selection takes in what goes in at its ends, a cursor
 *  goes after it. */
function lineSpec(state: EditorState, changes: ChangeSpec[]): TransactionSpec | null {
  if (!changes.length) return null;
  const set = state.changes(changes);
  const ranges = state.selection.ranges.map((r) =>
    r.empty ? EditorSelection.cursor(set.mapPos(r.head, 1)) : like(r, set.mapPos(r.from, -1), set.mapPos(r.to, 1)),
  );
  return { changes: set, selection: EditorSelection.create(ranges, state.selection.mainIndex) };
}

/** The level a heading takes next: none, #, ##, ###, then none again. */
const nextLevel = (level: number) => (level >= 3 ? 0 : level + 1);

function mdHeading(state: EditorState): TransactionSpec | null {
  const main = state.doc.lineAt(state.selection.main.head);
  const next = nextLevel(MD_HEADING.exec(main.text)?.[1].length ?? 0);
  const lines = touched(state).map((n) => state.doc.line(n));
  const changes: ChangeSpec[] = [];
  for (const line of lines) {
    if (!line.text.trim() && lines.length > 1) continue;
    const prefix = MD_HEADING.exec(line.text)?.[0].length ?? 0;
    changes.push({ from: line.from, to: line.from + prefix, insert: next ? `${"#".repeat(next)} ` : "" });
  }
  return lineSpec(state, changes);
}

function texHeading(state: EditorState): TransactionSpec | null {
  const level = (text: string) => {
    const m = TEX_HEADING.exec(text);
    return m ? m[2].length / 3 + 1 : 0;
  };
  const main = state.doc.lineAt(state.selection.main.head);
  const next = nextLevel(level(main.text));
  const lines = touched(state).map((n) => state.doc.line(n));
  // an empty line: the command to type the title into
  if (lines.length === 1 && !main.text.trim()) {
    if (!next) return null;
    const insert = "\\section{}";
    return { changes: { from: main.to, insert }, selection: EditorSelection.cursor(main.to + insert.length - 1) };
  }
  const changes: ChangeSpec[] = [];
  for (const line of lines) {
    if (!line.text.trim()) continue;
    const m = TEX_HEADING.exec(line.text);
    if (m && next) {
      const at = line.from + m[1].length + 1;
      changes.push({ from: at, to: at + m[2].length + "section".length, insert: `${"sub".repeat(next - 1)}section` });
    } else if (m) {
      const end = closing(state, line.from + m[0].length - 1);
      if (end === null) continue;
      changes.push({ from: line.from + m[1].length, to: line.from + m[0].length }, { from: end, to: end + 1 });
    } else if (next) {
      const indent = line.text.length - line.text.trimStart().length;
      changes.push({ from: line.from + indent, insert: "\\section{" }, { from: line.from + line.text.trimEnd().length, insert: "}" });
    }
  }
  return lineSpec(state, changes);
}

function mdBlock(state: EditorState, what: Block): TransactionSpec | null {
  const lines = touched(state).map((n) => state.doc.line(n));
  const filled = lines.filter((l) => l.text.trim());
  const own = what === "bullet" ? MD_BULLET : what === "numbered" ? MD_NUMBER : MD_QUOTE;
  const off = filled.length > 0 && filled.every((l) => own.test(l.text));
  const changes: ChangeSpec[] = [];
  // an empty line: the mark to type after
  if (!filled.length) {
    const line = state.doc.lineAt(state.selection.main.head);
    const insert = what === "bullet" ? "- " : what === "numbered" ? "1. " : "> ";
    return { changes: { from: line.from, to: line.to, insert }, selection: EditorSelection.cursor(line.from + insert.length) };
  }
  const [first, last] = [filled[0].number, filled[filled.length - 1].number];
  let n = 0;
  for (const line of lines) {
    if (line.number < first || line.number > last) continue;
    const text = line.text;
    if (what === "quote") {
      if (off) {
        const m = MD_QUOTE.exec(text);
        if (m) changes.push({ from: line.from, to: line.from + m[0].length });
      } else changes.push({ from: line.from, insert: text.trim() ? "> " : ">" });
      continue;
    }
    if (!text.trim()) continue;
    const m = MD_BULLET.exec(text) ?? MD_NUMBER.exec(text);
    const indent = m ? m[1].length : text.length - text.trimStart().length;
    const end = m ? m[0].length : indent;
    const insert = off ? "" : what === "bullet" ? "- " : `${++n}. `;
    changes.push({ from: line.from + indent, to: line.from + end, insert });
  }
  return lineSpec(state, changes);
}

/** The lines `\begin{env}` and `\end{env}` around the lines first to last, if they are in one. */
function environment(state: EditorState, env: string, first: number, last: number): [number, number] | null {
  const begin = new RegExp(`^\\s*\\\\begin\\{${env}\\}`);
  const end = new RegExp(`^\\s*\\\\end\\{${env}\\}`);
  let top = 0;
  for (let n = first; n >= 1; n--) {
    const text = state.doc.line(n).text;
    if (begin.test(text)) {
      top = n;
      break;
    }
    if (end.test(text) && n !== last) return null;
  }
  if (!top) return null;
  for (let n = Math.max(last, top + 1); n <= state.doc.lines; n++) {
    const text = state.doc.line(n).text;
    if (end.test(text)) return [top, n];
    if (begin.test(text)) return null;
  }
  return null;
}

function texBlock(state: EditorState, what: Block): TransactionSpec | null {
  const env = ENV[what];
  const item = what === "quote" ? "" : "\\item ";
  const unit = state.facet(indentUnit);
  const main = state.selection.main;
  const first = state.doc.lineAt(main.from).number;
  let last = state.doc.lineAt(main.to).number;
  if (!main.empty && last > first && state.doc.line(last).from === main.to) last--;

  // in the environment already: taken off, its lines out of it
  const around = environment(state, env, first, last);
  if (around) {
    const [top, bottom] = around;
    const inner: string[] = [];
    for (let n = top + 1; n < bottom; n++) {
      let text = state.doc.line(n).text;
      if (text.startsWith(unit)) text = text.slice(unit.length);
      if (item) text = text.replace(/^(\s*)\\item\s?/, "$1");
      inner.push(text);
    }
    const [from, to] = [state.doc.line(top).from, state.doc.line(bottom).to];
    const insert = inner.join("\n");
    return { changes: { from, to, insert }, selection: EditorSelection.cursor(from + insert.length) };
  }

  const lines: { text: string }[] = [];
  for (let n = first; n <= last; n++) lines.push(state.doc.line(n));
  const filled = lines.filter((l) => l.text.trim());
  const [from, to] = [state.doc.line(first).from, state.doc.line(last).to];
  const indent = /^\s*/.exec(filled[0]?.text ?? "")?.[0] ?? "";
  const body = filled.length ? filled.map((l) => `${indent}${unit}${item}${l.text.trim()}`) : [`${indent}${unit}${item}`];
  const head = `${indent}\\begin{${env}}\n`;
  const insert = `${head}${body.join("\n")}\n${indent}\\end{${env}}`;
  const cursor = from + head.length + body.join("\n").length;
  return { changes: { from, to, insert }, selection: EditorSelection.cursor(cursor) };
}
