// A LaTeX file (.tex, .sty, .cls) as a stream language: the commands, the names of environments, math (`$ $`, `$$ $$`,
// `\( \)`, `\[ \]` and the environments `equation`, `align` ...), comments, braces, and the keys of `\cite{}`, `\ref{}`
// and `\label{}`. The tokens are named as the nodes of the Markdown mode are, with the same tags (see tags.ts), so
// that the editor colours both the same way.

import { StreamLanguage, type StreamParser } from "@codemirror/language";
import { tags } from "@lezer/highlight";
import { MATH_ENVS } from "./tokens.ts";
import * as t from "./tags.ts";

/** Where the tokenizer is: in the text or in a kind of math, and what it expects after a command. */
export type TexState = {
  /** the math it is in: its closing delimiter (`$`, `$$`, `\)`, `\]`), `env:name` for an environment, or none */
  math: string | null;
  /** after `\begin` / `\end` (`\`'s command), the brace (`open`) and then the name (`name`) are next; after a command
   *  with keys, the optional arguments and the brace; in the keys, the keys */
  next: null | { kind: "env"; end: boolean; at: "brace" | "name" } | { kind: "keys"; node: string; at: "brace" | "list" };
  /** inside a verbatim environment: the name to close it with */
  verbatim: string | null;
};

const VERBATIM = /^(?:verbatim|lstlisting|minted|comment)\*?$/;
const KEYED = /^\\(cite[A-Za-z]*|[A-Za-z]*cite[s]?|label|ref|eqref|autoref|pageref|cref|Cref|nameref|vref)\*?/;
const keyNode = (name: string) => (/cite/i.test(name) ? "CitationKey" : name === "label" ? "LabelName" : "CrossRefKey");

/** The tokenizer: `token` returns the name of a token (see `tokenTable`) for the text it consumed from the stream. */
export const texParser: StreamParser<TexState> = {
  name: "latex",
  startState: () => ({ math: null, next: null, verbatim: null }),
  copyState: (s) => ({ ...s, next: s.next && { ...s.next } }),
  languageData: { commentTokens: { line: "%" } },
  tokenTable: {
    Comment: tags.comment,
    LatexCommand: t.latexCommand,
    LatexBrace: t.latexBrace,
    LatexEnvName: t.latexEnvName,
    MathMark: t.mathMark,
    MathCommand: t.mathCommand,
    MathOperator: t.mathOperator,
    MathBrace: t.mathBrace,
    MathNumber: t.mathNumber,
    CitationKey: t.citationKey,
    CrossRefKey: t.crossRefKey,
    LabelName: t.labelName,
  },
  token(stream, state) {
    const math = state.math !== null;
    const names = math ? { command: "MathCommand", brace: "MathBrace" } : { command: "LatexCommand", brace: "LatexBrace" };

    if (state.verbatim) {
      const end = new RegExp(`\\\\end\\{${state.verbatim.replace(/[*]/g, "\\*")}\\}`);
      const hit = end.exec(stream.string.slice(stream.pos));
      if (!hit) return stream.skipToEnd(), null;
      if (hit.index > 0) return stream.pos += hit.index, null;
      state.verbatim = null;
      stream.pos += hit[0].length;
      return "LatexCommand";
    }
    if (stream.eatSpace()) return null;

    // what the last command asked for: the brace and the name of an environment, the brace and the keys of a citation
    const next = state.next;
    if (next?.kind === "env") {
      if (next.at === "brace") {
        state.next = null;
        if (stream.eat("{")) return (state.next = { ...next, at: "name" }), names.brace;
      } else {
        const name = stream.match(/^[^}\n]*/) as RegExpMatchArray;
        const word = name[0];
        state.next = null;
        stream.eat("}");
        if (word) {
          if (next.end && state.math === `env:${word}`) state.math = null;
          else if (!next.end && !math && MATH_ENVS.test(word)) state.math = `env:${word}`;
          else if (!next.end && !math && VERBATIM.test(word)) state.verbatim = word;
          // the name; the closing brace comes as a brace of its own on the next call
          return stream.backUp(stream.string[stream.pos - 1] === "}" ? 1 : 0), "LatexEnvName";
        }
        return names.brace;
      }
    }
    if (next?.kind === "keys") {
      if (next.at === "brace") {
        if (stream.match(/^\[[^\]\n]*\]/)) return null;
        state.next = null;
        if (stream.eat("{")) return (state.next = { ...next, at: "list" }), names.brace;
      } else {
        if (stream.eat(",")) return names.brace;
        if (stream.eat("}")) return (state.next = null), names.brace;
        if (stream.match(/^[^,}\s%]+/)) return next.node;
        if (stream.peek() === "%" || stream.eol()) return stream.skipToEnd(), "Comment";
        stream.next();
        return null;
      }
    }

    const c = stream.next() as string;
    if (c === "%") return stream.skipToEnd(), "Comment";

    if (c === "\\") {
      // the end of a math delimited by `\)` or `\]`
      const close = stream.peek();
      if (close === ")" || close === "]") {
        if (state.math === `\\${close}`) return stream.next(), (state.math = null), "MathMark";
      }
      if ((close === "(" || close === "[") && !math) return stream.next(), (state.math = close === "(" ? "\\)" : "\\]"), "MathMark";
      const word = stream.match(/^[A-Za-z@]+\*?/) as RegExpMatchArray | null;
      if (!word) return stream.next(), names.command;
      const command = `\\${word[0]}`;
      if (/^\\(?:begin|end)$/.test(command)) state.next = { kind: "env", end: command === "\\end", at: "brace" };
      else {
        const keyed = KEYED.exec(command);
        if (keyed && keyed[0] === command) state.next = { kind: "keys", node: keyNode(keyed[1] ?? command.slice(1)), at: "brace" };
      }
      return names.command;
    }

    if (c === "$") {
      if (state.math === "$$" && stream.eat("$")) return (state.math = null), "MathMark";
      if (state.math === "$") return (state.math = null), "MathMark";
      if (math) return "MathMark";
      state.math = stream.eat("$") ? "$$" : "$";
      return "MathMark";
    }
    if (c === "{" || c === "}") return names.brace;
    if (math) {
      if (c === "^" || c === "_") return "MathOperator";
      if (/\d/.test(c)) return stream.match(/^\d*(?:\.\d+)?/), "MathNumber";
      stream.match(/^[^\\${}^_%\d]*/);
      return null;
    }
    if (c === "[" || c === "]") return "LatexBrace";
    stream.match(/^[^\\${}[\]%]*/);
    return null;
  },
  blankLine(state) {
    // a paragraph ends math opened with `$` (TeX would complain); the display and environment ones go on
    if (state.math === "$") state.math = null;
    state.next = null;
  },
};

/** The language of LaTeX files. */
export const tex = StreamLanguage.define(texParser);
