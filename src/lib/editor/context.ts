// Where the cursor is, as far as the symbols typed and the completion care: pure functions of the syntax tree and of the
// text of the line before the cursor (`before`), so that they are tested without an editor.

import type { syntaxTree } from "@codemirror/language";

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
