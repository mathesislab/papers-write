// Where the cursor is, as far as the symbols typed and the completion care: pure functions of the syntax tree and of the
// text of the line before the cursor (`before`), so that they are tested without an editor.

import type { syntaxTree } from "@codemirror/language";
import { MATH_NAMES, TEXT_NAMES, TEX_NAMES, type LatexName } from "./lang/latex-names.ts";

type Tree = ReturnType<typeof syntaxTree>;

/** What the cursor is in, where what is typed stays as typed (code apart, see `inCode`): math, a LaTeX environment, the
 *  front matter, or a key (of a citation, a label, a file). */
export type Literal = "math" | "latex" | "front" | "keys";

const MATH_NODES = new Set(["InlineMath", "DisplayMath", "MathEnvironment"]);
const KEY_NODES = new Set(["CitationKey", "CrossRefKey", "LabelName", "URL"]);

const WORD = "\\p{L}\\p{N}_";

/** The math not closed yet on the line: the tree has no node for `$x^` until its second `$` is typed. After
 *  pandoc: an opening `$` is followed by a character that is no space (here, no digit either: it is a price). */
function openMath(before: string): boolean {
  let open: string | null = null;
  const text = before.replace(/\\./g, "xx");
  for (const m of text.matchAll(/\$\$|\$/g)) {
    if (open === m[0]) open = null;
    else if (open === null) {
      const next = text[m.index + m[0].length];
      if (next === undefined || !/[\s\d]/.test(next)) open = m[0];
    }
  }
  return open !== null;
}

/** A command with keys or a path whose braces are not closed yet, or a link not closed, or a `@key` being typed. */
const OPEN_ARGUMENT = new RegExp(
  [
    String.raw`\\(?:cite[A-Za-z]*|label|ref|eqref|autoref|pageref|cref|includegraphics|input|include)\*?(?:\[[^\]]*\]){0,2}\{[^}]*$`,
    String.raw`\]\([^)\s]*$`,
    String.raw`(?:^|[^${WORD}])-?@[^\s\[\]]*$`,
  ].join("|"),
  "u",
);

/** What `pos` is in that keeps the symbols typed out: in math (also one not closed yet), in a LaTeX environment, in the
 *  front matter, in a key or an argument that is a file. `before` is the text of the line up to `pos`. */
export function literalAt(tree: Tree, pos: number, before: string): Literal | null {
  for (let node: ReturnType<Tree["resolve"]> | null = tree.resolve(pos, -1); node; node = node.parent) {
    const inside = node.from < pos && pos < node.to;
    if (MATH_NODES.has(node.name) && inside) return "math";
    if (node.name === "LatexEnvironment" && inside) return "latex";
    if (node.name === "FrontMatter" && inside) return "front";
    if (KEY_NODES.has(node.name) && node.from < pos && pos <= node.to) return "keys";
  }
  if (openMath(before)) return "math";
  return OPEN_ARGUMENT.test(before) ? "keys" : null;
}

/** What the completion offers at the cursor, and what of the text it replaces (from `from` to the cursor). */
export type Context =
  | { kind: "citation"; from: number; prefix: string }
  | { kind: "ref"; from: number; prefix: string; /** `fig`, `eq`... of `@fig:` */ refKind?: string }
  | { kind: "latex"; from: number; prefix: string; names: LatexName[] }
  | { kind: "path"; from: number; prefix: string };

const KINDS = "fig|eq|tbl|sec|lst";
const KEYCHARS = "[\\p{L}\\p{N}_:.#$%&\\-+?<>~/]*";
/** The key being typed in `\cite{a, sc`, after the keys before it. */
const CITE_COMMAND = new RegExp(String.raw`\\cite[A-Za-z]*\*?(?:\[[^\]]*\]){0,2}\{(?:[^}]*,\s*)?([^},\s]*)$`);
const REF_COMMAND = new RegExp(String.raw`\\(?:eqref|ref|autoref|pageref|cref)\{(?:[^}]*,\s*)?([^},\s]*)$`);
const PATH_COMMAND = /\\(?:includegraphics|input|include)\*?(?:\[[^\]]*\])?\{([^}]*)$/;
const PATH_IMAGE = /!\[[^\]]*\]\(([^)\s]*)$/;
const REF_PANDOC = new RegExp(`(?:^|[^${WORD}])-?@(${KINDS}):([^\\s\\[\\]{};,@]*)$`, "u");
const CITE_PANDOC = new RegExp(`(?:^|[^${WORD}])@(${KEYCHARS})$`, "u");
const NAME = /(?:^|[^\\])\\([A-Za-z]*)$/;

/** What is to be completed at `pos` (`before`: the text of its line before it), or null in prose, code and the rest. */
export function contextAt(tree: Tree, pos: number, before: string, code = false): Context | null {
  if (code) return null;
  const literal = literalAt(tree, pos, before);
  if (literal === "front") return null;
  const hit = (re: RegExp) => re.exec(before);
  let m: RegExpExecArray | null;
  if ((m = hit(PATH_IMAGE) ?? hit(PATH_COMMAND))) return { kind: "path", from: pos - m[1].length, prefix: m[1] };
  if ((m = hit(CITE_COMMAND))) return { kind: "citation", from: pos - m[1].length, prefix: m[1] };
  if ((m = hit(REF_COMMAND))) return { kind: "ref", from: pos - m[1].length, prefix: m[1] };
  // `@a` in math is no citation
  if (literal === "math" || literal === "latex") {
    if ((m = hit(NAME))) return { kind: "latex", from: pos - m[1].length - 1, prefix: m[1], names: MATH_NAMES };
    return null;
  }
  if ((m = hit(REF_PANDOC))) return { kind: "ref", from: pos - m[2].length, prefix: m[2], refKind: m[1] };
  if ((m = hit(CITE_PANDOC))) return { kind: "citation", from: pos - m[1].length, prefix: m[1] };
  if ((m = hit(NAME))) return { kind: "latex", from: pos - m[1].length - 1, prefix: m[1], names: TEXT_NAMES };
  return null;
}

/** The labels defined in the text (`{#fig:x}`, `\label{x}`), in order, each once. */
export function labelsOf(tree: Tree, text: string): string[] {
  const labels = new Set<string>();
  tree.iterate({
    enter(n) {
      if (n.name === "LabelName") labels.add(text.slice(n.from, n.to));
    },
  });
  return [...labels];
}

/** The labels that `context` (a reference) can be: for `@fig:` those of the kind, with what follows the colon as the
 *  text to insert; the others whole. */
export function refOptions(labels: string[], context: { prefix: string; refKind?: string }): { label: string; insert: string }[] {
  const { prefix, refKind } = context;
  return labels
    .filter((l) => (refKind ? l.startsWith(`${refKind}:`) : true))
    .map((l) => ({ label: l, insert: refKind ? l.slice(refKind.length + 1) : l }))
    .filter((o) => o.insert.startsWith(prefix));
}

/** The names that `context` (a LaTeX command) can be, those that begin as typed. */
export function nameOptions(context: { prefix: string; names: LatexName[] }): LatexName[] {
  return context.names.filter((n) => n.name.slice(1).startsWith(context.prefix));
}

/** The text of a line without its comment (the first `%` that is no `\%`). */
const uncommented = (line: string) => /^(?:[^%\\]|\\.)*/.exec(line)?.[0] ?? line;

/** What is to be completed at the end of `before` (the line before the cursor at `pos`) in a LaTeX file: the keys of
 *  citations and references, paths, and the names of commands (all of them, in math and out of it); none in a comment. */
export function texContextAt(pos: number, before: string): Context | null {
  if (uncommented(before).length < before.length) return null;
  let m: RegExpExecArray | null;
  if ((m = PATH_COMMAND.exec(before))) return { kind: "path", from: pos - m[1].length, prefix: m[1] };
  if ((m = CITE_COMMAND.exec(before))) return { kind: "citation", from: pos - m[1].length, prefix: m[1] };
  if ((m = REF_COMMAND.exec(before))) return { kind: "ref", from: pos - m[1].length, prefix: m[1] };
  if ((m = NAME.exec(before))) return { kind: "latex", from: pos - m[1].length - 1, prefix: m[1], names: TEX_NAMES };
  return null;
}

/** The labels defined in a LaTeX text (`\label{x}`), in order, each once; comments left out. */
export function texLabelsOf(text: string): string[] {
  const labels = new Set<string>();
  for (const line of text.split("\n")) for (const m of uncommented(line).matchAll(/\\label\{([^}\s]+)\}/g)) labels.add(m[1]);
  return [...labels];
}
