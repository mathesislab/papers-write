// What the pointer is over and what a label stands for (src/lib/editor/hover.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { markdown } from "@codemirror/lang-markdown";
import { EditorState } from "@codemirror/state";
import { ensureSyntaxTree } from "@codemirror/language";
import { environmentTex, labelTarget, targetAt } from "../src/lib/editor/hover.ts";
import { paperMarkdown, tex } from "../src/lib/editor/lang/index.ts";

const md = (doc: string) => {
  const state = EditorState.create({ doc, extensions: [markdown({ extensions: paperMarkdown })] });
  ensureSyntaxTree(state, doc.length, 1000);
  return state;
};
const latex = (doc: string) => {
  const state = EditorState.create({ doc, extensions: [tex] });
  ensureSyntaxTree(state, doc.length, 1000);
  return state;
};

/** The target at the `|` of the marked text. */
function over(marked: string, make = md) {
  const pos = marked.indexOf("|");
  return targetAt(make(marked.replace("|", "")), pos);
}

test("inline and display math, and math environments", () => {
  assert.deepEqual(over("so $x|^2$ here"), { kind: "math", tex: "x^2", display: false, from: 3, to: 8 });
  const display = over("$$\na |= b\n$$ {#eq:a}");
  assert.ok(display?.kind === "math" && display.tex === "a = b" && display.display);
  const env = over("\\begin{align}\n a &= b |\\label{eq:a} \\\\\n c &= d\n\\end{align}");
  assert.ok(env?.kind === "math" && env.display && env.tex.startsWith("\\begin{align}") && !env.tex.includes("\\label"), JSON.stringify(env));
  assert.equal(over("no $x$ |math here"), null);
});

test("the equation wrapper is left off", () => {
  assert.equal(environmentTex("\\begin{equation}\n E = mc^2 \\label{eq:e}\n\\end{equation}"), "E = mc^2");
  assert.equal(environmentTex("\\begin{align*}\na&=b\n\\end{align*}"), "\\begin{align*}\na&=b\n\\end{align*}");
});

test("citation keys and cross-reference keys", () => {
  const cite = over("as [@sch|oof2007; @b] says");
  assert.ok(cite?.kind === "citation" && cite.key === "schoof2007");
  const bare = over("see @feld|mann2015 here");
  assert.ok(bare?.kind === "citation" && bare.key === "feldmann2015");
  const ref = over("see @fig:lo|op here");
  assert.ok(ref?.kind === "ref" && ref.key === "fig:loop");
  const tcite = over("text \\cite{a,b|c} text", latex);
  assert.ok(tcite?.kind === "citation" && tcite.key === "bc");
  const tref = over("\\ref{fig:|x}", latex);
  assert.ok(tref?.kind === "ref" && tref.key === "fig:x");
  assert.equal(over("plain |words"), null);
});

test("images", () => {
  const md1 = over("![A loop](figures/lo|op.png){#fig:loop}");
  assert.ok(md1?.kind === "image" && md1.path === "figures/loop.png");
  assert.equal(over("![A loop](https://x.org/lo|op.png)"), null);
  const spaced = over("![x](<my fig|ure.png>)");
  assert.ok(spaced?.kind === "image" && spaced.path === "my figure.png");
  const inc = over("\\includegraphics[width=\\textwidth]{figs/a|.pdf}", latex);
  assert.ok(inc?.kind === "image" && inc.path === "figs/a.pdf");
  // raw LaTeX in Markdown
  const raw = over("\\includegraphics{figs/b|.png}");
  assert.ok(raw?.kind === "image" && raw.path === "figs/b.png");
});

const DOC = `# Introduction {#sec:intro}

![The hysteresis loop](figures/loop.png){#fig:loop}

| a | b |
|---|---|
| 1 | 2 |

: Parameters of the model {#tbl:params}

$$
\\tau_b = C |u_b|^{m-1} u_b
$$ {#eq:friction}

\\begin{equation}
  E = mc^2 \\label{eq:energy}
\\end{equation}
`;

test("what a label stands for, Markdown", () => {
  const state = md(DOC);
  assert.equal(labelTarget(state, "sec:intro"), "Section: Introduction");
  assert.equal(labelTarget(state, "fig:loop"), "Figure: The hysteresis loop");
  assert.equal(labelTarget(state, "tbl:params"), "Table: Parameters of the model");
  assert.equal(labelTarget(state, "eq:friction"), "Equation: \\tau_b = C |u_b|^{m-1} u_b");
  assert.equal(labelTarget(state, "eq:energy"), "Equation: E = mc^2");
  assert.equal(labelTarget(state, "fig:none"), null);
});

const TEX = `\\section{Method}\\label{sec:method}

\\begin{figure}
  \\includegraphics{a.png}
  \\caption{A {nested} caption}
  \\label{fig:a}
\\end{figure}

% \\label{fig:commented}
\\begin{align}
  a &= b \\label{eq:a}
\\end{align}
`;

test("what a label stands for, LaTeX", () => {
  const state = latex(TEX);
  assert.equal(labelTarget(state, "sec:method"), "Section: Method");
  assert.equal(labelTarget(state, "fig:a"), "Figure: A {nested} caption");
  assert.equal(labelTarget(state, "eq:a"), "Equation: a &= b");
});
