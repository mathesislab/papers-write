// The LaTeX mode of the editor (src/lib/editor/lang/tex.ts): the tokens found in samples.

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { tex } from "../src/lib/editor/lang/tex.ts";

/** The tokens of a text as "Name:text", in order, for the names asked (all of them if none). */
function tokens(text: string, ...names: string[]): string[] {
  const out: string[] = [];
  tex.parser.parse(text).iterate({
    enter(n) {
      if (n.name !== "Document" && (!names.length || names.includes(n.name))) out.push(`${n.name}:${text.slice(n.from, n.to)}`);
    },
  });
  return out;
}

test("commands and braces", () => {
  assert.deepEqual(tokens("\\section*{Intro} and \\emph{x}", "LatexCommand", "LatexBrace"), [
    "LatexCommand:\\section*", "LatexBrace:{", "LatexBrace:}", "LatexCommand:\\emph", "LatexBrace:{", "LatexBrace:}",
  ]);
});

test("environments: the names, and math ones as math", () => {
  assert.deepEqual(tokens("\\begin{itemize}\n\\item a $x$\n\\end{itemize}", "LatexEnvName", "InlineMath"), ["LatexEnvName:itemize", "LatexEnvName:itemize"]);
  const text = "\\begin{equation}\n\\frac{a}{b}^2\n\\end{equation}\nafter \\alpha";
  assert.deepEqual(tokens(text, "MathCommand", "MathOperator", "MathNumber", "LatexCommand"), [
    "LatexCommand:\\begin", "MathCommand:\\frac", "MathOperator:^", "MathNumber:2", "MathCommand:\\end", "LatexCommand:\\alpha",
  ]);
});

test("inline and display math", () => {
  assert.deepEqual(tokens("a $x^2$ b \\alpha", "MathMark", "MathCommand", "MathOperator", "MathNumber", "LatexCommand"), [
    "MathMark:$", "MathOperator:^", "MathNumber:2", "MathMark:$", "LatexCommand:\\alpha",
  ]);
  assert.deepEqual(tokens("$$\\sum_i$$ \\beta", "MathMark", "MathCommand", "LatexCommand"), ["MathMark:$$", "MathCommand:\\sum", "MathMark:$$", "LatexCommand:\\beta"]);
  assert.deepEqual(tokens("\\(\\gamma\\) \\[\\delta\\] \\eta", "MathMark", "MathCommand", "LatexCommand"), [
    "MathMark:\\(", "MathCommand:\\gamma", "MathMark:\\)", "MathMark:\\[", "MathCommand:\\delta", "MathMark:\\]", "LatexCommand:\\eta",
  ]);
  // over lines
  assert.deepEqual(tokens("\\[\n\\theta\n\\]\n\\iota", "MathCommand", "LatexCommand"), ["MathCommand:\\theta", "LatexCommand:\\iota"]);
});

test("comments, and \\% is no comment", () => {
  assert.deepEqual(tokens("a % \\cite{x} $\nb $y$", "Comment", "CitationKey", "MathMark"), ["Comment:% \\cite{x} $", "MathMark:$", "MathMark:$"]);
  assert.deepEqual(tokens("50\\% of \\emph{x} % rest", "Comment", "LatexCommand"), ["LatexCommand:\\%", "LatexCommand:\\emph", "Comment:% rest"]);
  assert.deepEqual(tokens("a \\$ b $x$", "MathMark"), ["MathMark:$", "MathMark:$"]);
});

test("the keys of citations, references and labels", () => {
  assert.deepEqual(tokens("\\citep[see][p.~2]{a, b2000} \\ref{fig:x} \\label{eq:y} \\eqref{eq:y}", "CitationKey", "CrossRefKey", "LabelName"), [
    "CitationKey:a", "CitationKey:b2000", "CrossRefKey:fig:x", "LabelName:eq:y", "CrossRefKey:eq:y",
  ]);
  assert.deepEqual(tokens("\\textcite{k} \\parencite*{j}", "CitationKey"), ["CitationKey:k", "CitationKey:j"]);
});

test("verbatim is left alone", () => {
  assert.deepEqual(tokens("\\begin{verbatim}\n$ \\cite{x} %\n\\end{verbatim}\n$y$", "MathMark", "CitationKey", "Comment"), ["MathMark:$", "MathMark:$"]);
});

test("completion in a LaTeX file: keys, names, labels, no comments", async () => {
  const { texContextAt, texLabelsOf } = await import("../src/lib/editor/context.ts");
  const at = (before: string) => texContextAt(before.length, before);
  assert.deepEqual(at("see \\citep[p.~2]{a, sc"), { kind: "citation", from: 20, prefix: "sc" });
  assert.deepEqual(at("\\eqref{eq:"), { kind: "ref", from: 7, prefix: "eq:" });
  assert.equal(at("\\ref{x")?.kind, "ref");
  assert.equal(at("in math $\\al")?.kind, "latex");
  assert.equal(at("\\sec")?.kind, "latex");
  assert.equal(at("plain words"), null);
  // `@` is no citation in LaTeX
  assert.equal(at("mail me@here"), null);
  assert.equal(at("% \\cite{a"), null);
  assert.equal(at("50\\% \\ci")?.kind, "latex");
  assert.deepEqual(texLabelsOf("\\label{a}\n% \\label{b}\n\\label{c} \\label{a}"), ["a", "c"]);
});
