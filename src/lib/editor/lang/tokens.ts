// The small tokenizer shared by math, LaTeX environments and commands in the text: it gives the elements (nodes with
// absolute positions) of one line of text, in math or as LaTeX.

import type { Element } from "@lezer/markdown";

/** Makes an element: `cx.elt` of a block or an inline context. */
export type Elt = (type: string, from: number, to: number, children?: readonly Element[]) => Element;

export type Mode = "math" | "latex";

/** The environments pandoc passes raw whose content is math. */
export const MATH_ENVS = /^(?:equation|align|alignat|aligned|gather|gathered|multline|flalign|eqnarray|split|math|displaymath)\*?$/;

const TOKEN = /\\(?:begin|end)\{[^}\n]*\}|\\[A-Za-z]+\*?|\\[^A-Za-z\s]|[\^_]|[{}]|\d+(?:\.\d+)?/gy;

const NAMES = {
  math: { command: "MathCommand", brace: "MathBrace" },
  latex: { command: "LatexCommand", brace: "LatexBrace" },
};

/** The elements of `text`, which starts at `at` in the document. */
export function tokens(text: string, at: number, elt: Elt, mode: Mode): Element[] {
  const out: Element[] = [];
  const names = NAMES[mode];
  for (let i = 0; i < text.length; i++) {
    TOKEN.lastIndex = i;
    const m = TOKEN.exec(text);
    if (!m) continue;
    const [s] = m;
    const from = at + i;
    i += s.length - 1;
    const env = /^\\(begin|end)\{([^}]*)\}$/.exec(s);
    if (env) {
      const word = 1 + env[1].length;
      out.push(elt(names.command, from, from + word));
      out.push(elt("LatexEnvName", from + word + 1, from + s.length - 1));
      out.push(elt(names.brace, from + word, from + word + 1), elt(names.brace, from + s.length - 1, from + s.length));
    } else if (s[0] === "\\") out.push(elt(names.command, from, from + s.length));
    else if (s === "{" || s === "}") out.push(elt(names.brace, from, from + 1));
    else if (mode === "latex") continue;
    else if (s === "^" || s === "_") out.push(elt("MathOperator", from, from + 1));
    else out.push(elt("MathNumber", from, from + s.length));
  }
  return out.sort((a, b) => a.from - b.from);
}

// Lezer Markdown's BlockContext has an `input` (the whole text) and its Line a `depth` (the containers it continues);
// both are in the code but not in the typings.
type Internals = { input: { length: number; read(from: number, to: number): string } };

/** The text of the document from `from` to its end, to look ahead for a closing line. */
export const readRest = (cx: object, from: number): string => {
  const { input } = cx as Internals;
  return input.read(from, input.length);
};

/** Whether the current line still continues the containers (quote, list) that `cx` is in. */
export const inside = (cx: { depth: number }, line: object): boolean => ((line as { depth: number }).depth ?? 0) >= cx.depth;
