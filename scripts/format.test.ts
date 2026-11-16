// Formatting at the selection: bold, italic, code, a link, a heading, lists and quotations, in Markdown and LaTeX
// (src/lib/editor/format.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { EditorSelection, EditorState } from "@codemirror/state";
import { markdown } from "@codemirror/lang-markdown";
import { formatSpec, type Format } from "../src/lib/editor/format.ts";

/** The text after `what`, the selection written in it: | a cursor, « and » a selection. */
function run(text: string, what: Format, latex = false): string {
  const cursor = text.indexOf("|");
  const selection =
    cursor >= 0
      ? EditorSelection.cursor(cursor)
      : EditorSelection.range(text.indexOf("«"), text.indexOf("»") - 1);
  const doc = text.replace(/[|«»]/g, "");
  const state = EditorState.create({ doc, selection, extensions: latex ? [] : [markdown()] });
  const spec = formatSpec(state, what, latex);
  if (!spec) return text;
  const after = state.update(spec).state;
  const r = after.selection.main;
  const out = after.doc.toString();
  if (r.empty) return out.slice(0, r.head) + "|" + out.slice(r.head);
  return out.slice(0, r.from) + "«" + out.slice(r.from, r.to) + "»" + out.slice(r.to);
}

test("bold around a selection, and off again", () => {
  assert.equal(run("the «grounding» line", "bold"), "the **«grounding»** line");
  assert.equal(run("the **«grounding»** line", "bold"), "the «grounding» line");
  assert.equal(run("the «**grounding**» line", "bold"), "the «grounding» line");
});

test("the spaces at the ends of a selection stay outside the marks", () => {
  assert.equal(run("the« grounding »line", "italic"), "the *«grounding»* line");
});

test("a cursor in a word formats the word; elsewhere the marks to type into", () => {
  assert.equal(run("the groun|ding line", "bold"), "the **groun|ding** line");
  assert.equal(run("the **groun|ding** line", "bold"), "the groun|ding line");
  assert.equal(run("the |line", "bold"), "the **|**line");
  assert.equal(run("the **|**line", "bold"), "the |line");
});

test("italic inside bold is italic, not half the bold", () => {
  assert.equal(run("a **«strong»** word", "italic"), "a ***«strong»*** word");
  assert.equal(run("a **|** word", "italic"), "a ***|*** word");
  assert.equal(run("an *em|ph* word", "italic"), "an em|ph word");
});

test("inline code", () => {
  assert.equal(run("call «make» here", "code"), "call `«make»` here");
  assert.equal(run("call `ma|ke` here", "code"), "call ma|ke here");
});

test("a link: the text selected, a URL selected, nothing selected; and taken off", () => {
  assert.equal(run("see «the paper» now", "link"), "see [the paper](|) now");
  assert.equal(run("see «https://doi.org/x» now", "link"), "see [|](https://doi.org/x) now");
  assert.equal(run("see | now", "link"), "see [|]() now");
  assert.equal(run("see [the pa|per](https://x.org) now", "link"), "see «the paper» now");
});

test("LaTeX: \\textbf, \\emph, \\texttt, on and off", () => {
  assert.equal(run("the «line»", "bold", true), "the \\textbf{«line»}");
  assert.equal(run("the \\textbf{«line»}", "bold", true), "the «line»");
  assert.equal(run("the «\\emph{line}»", "italic", true), "the «line»");
  assert.equal(run("the \\emph{li|ne}", "italic", true), "the li|ne");
  assert.equal(run("the |", "code", true), "the \\texttt{|}");
  assert.equal(run("a \\textbf{\\emph{x} and |y}", "bold", true), "a \\emph{x} and |y");
});

test("LaTeX: \\href", () => {
  assert.equal(run("see «the paper»", "link", true), "see \\href{|}{the paper}");
  assert.equal(run("see «https://x.org»", "link", true), "see \\href{https://x.org}{|}");
  assert.equal(run("see \\href{https://x.org}{the pa|per}", "link", true), "see «the paper»");
});

test("a heading: #, ##, ###, then none", () => {
  assert.equal(run("Intro|duction", "heading"), "# Intro|duction");
  assert.equal(run("# Intro|duction", "heading"), "## Intro|duction");
  assert.equal(run("### Intro|duction {#sec:intro}", "heading"), "Intro|duction {#sec:intro}");
  assert.equal(run("|", "heading"), "# |");
});

test("LaTeX: \\section, \\subsection, \\subsubsection, then the title alone", () => {
  assert.equal(run("Intro|duction", "heading", true), "\\section{Intro|duction}");
  assert.equal(run("\\section{Intro|duction}\\label{s}", "heading", true), "\\subsection{Intro|duction}\\label{s}");
  assert.equal(run("\\section*{A|}", "heading", true), "\\subsection*{A|}");
  assert.equal(run("\\subsubsection{Intro|duction}", "heading", true), "Intro|duction");
  assert.equal(run("|", "heading", true), "\\section{|}");
});

test("lists: bullets and numbers on the lines selected, on and off, one into the other", () => {
  assert.equal(run("«one\ntwo»", "bullet"), "«- one\n- two»");
  assert.equal(run("«- one\n- two»", "bullet"), "«one\ntwo»");
  assert.equal(run("«one\n\ntwo»", "numbered"), "«1. one\n\n2. two»");
  assert.equal(run("«- one\n- two»", "numbered"), "«1. one\n2. two»");
  assert.equal(run("  in|dented", "bullet"), "  - in|dented");
  assert.equal(run("|", "numbered"), "1. |");
});

test("a quotation: every line of the block, the empty ones too", () => {
  assert.equal(run("«one\n\ntwo»", "quote"), "«> one\n>\n> two»");
  assert.equal(run("«> one\n>\n> two»", "quote"), "«one\n\ntwo»");
});

test("LaTeX: itemize, enumerate and quote around the lines, and taken off", () => {
  assert.equal(run("«one\ntwo»", "bullet", true), "\\begin{itemize}\n  \\item one\n  \\item two|\n\\end{itemize}");
  assert.equal(run("\\begin{enumerate}\n  \\item o|ne\n  \\item two\n\\end{enumerate}", "numbered", true), "one\ntwo|");
  assert.equal(run("|", "bullet", true), "\\begin{itemize}\n  \\item |\n\\end{itemize}");
  assert.equal(run("«said»", "quote", true), "\\begin{quote}\n  said|\n\\end{quote}");
});
