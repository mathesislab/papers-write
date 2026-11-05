// The small tokenizer shared by math, LaTeX environments and commands in the text: it gives the elements (nodes with
// absolute positions) of one line of text, in math or as LaTeX.

import type { Element } from "@lezer/markdown";

/** Makes an element: `cx.elt` of a block or an inline context. */
export type Elt = (type: string, from: number, to: number, children?: readonly Element[]) => Element;

export type Mode = "math" | "latex";

/** The environments pandoc passes raw whose content is math. */
export const MATH_ENVS = /^(?:equation|align|alignat|aligned|gather|gathered|multline|flalign|eqnarray|split|math|displaymath)\*?$/;

/** A command at the start of the text: `\\begin{x}`/`\\end{x}`, one with a key or keys as its argument (`\\cite{a,b}`,
 *  `\\label{x}`, `\\ref{x}`), `\\name` or `\\name*`. */
const COMMAND = /\\(?:(begin|end)\{([^}\n]*)\}|(cite[A-Za-z]*|label|ref|eqref|autoref|pageref|cref)(\*?)((?:\[[^\]\n]*\]){0,2})\{([^}\n]*)\}|[A-Za-z]+\*?)/y;
const TOKEN = /\\[^A-Za-z\s]|[\^_]|[{}]|\d+(?:\.\d+)?/y;

/** The length of the command at the start of `text` (0 if there is none). */
export function commandLength(text: string): number {
  COMMAND.lastIndex = 0;
  return COMMAND.exec(text)?.[0].length ?? 0;
}

const NAMES = {
  math: { command: "MathCommand", brace: "MathBrace" },
  latex: { command: "LatexCommand", brace: "LatexBrace" },
};

/** The elements of `text`, which starts at `at` in the document. */
export function tokens(text: string, at: number, elt: Elt, mode: Mode): Element[] {
  const out: Element[] = [];
  const names = NAMES[mode];
  for (let i = 0; i < text.length; i++) {
    const from = at + i;
    COMMAND.lastIndex = i;
    const c = text[i] === "\\" ? COMMAND.exec(text) : null;
    if (c) {
      const [s, env, envName, keyed, star, optional, keys] = c;
      i += s.length - 1;
      if (env) {
        const word = 1 + env.length;
        out.push(elt(names.command, from, from + word), elt(names.brace, from + word, from + word + 1));
        out.push(elt("LatexEnvName", from + word + 1, from + word + 1 + envName.length), elt(names.brace, from + s.length - 1, from + s.length));
      } else if (keyed) {
        out.push(elt(names.command, from, from + 1 + keyed.length + star.length));
        const open = from + 1 + keyed.length + star.length + optional.length;
        out.push(elt(names.brace, open, open + 1));
        const node = keyed.startsWith("cite") ? "CitationKey" : keyed === "label" ? "LabelName" : "CrossRefKey";
        for (const k of keys.matchAll(/[^,\s][^,]*/g)) out.push(elt(node, open + 1 + k.index, open + 1 + k.index + k[0].trimEnd().length));
        out.push(elt(names.brace, from + s.length - 1, from + s.length));
      } else out.push(elt(names.command, from, from + s.length));
      continue;
    }
    TOKEN.lastIndex = i;
    const m = TOKEN.exec(text);
    if (!m) continue;
    const [s] = m;
    i += s.length - 1;
    if (s[0] === "\\") out.push(elt(names.command, from, from + s.length));
    else if (s === "{" || s === "}") out.push(elt(names.brace, from, from + 1));
    else if (mode === "latex") continue;
    else if (s === "^" || s === "_") out.push(elt("MathOperator", from, from + 1));
    else out.push(elt("MathNumber", from, from + s.length));
  }
  return out;
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
