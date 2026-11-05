// The outline of a document (src/lib/editor/outline.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { markdown } from "@codemirror/lang-markdown";
import { ensureSyntaxTree } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { currentHeading, outline } from "../src/lib/editor/outline.ts";
import { paperMarkdown, tex } from "../src/lib/editor/lang/index.ts";

const make = (doc: string, latex = false) => {
  const state = EditorState.create({ doc, extensions: [latex ? tex : markdown({ extensions: paperMarkdown })] });
  ensureSyntaxTree(state, doc.length, 1000);
  return state;
};

test("Markdown headings by level, none in code", () => {
  const doc = "# Title {#sec:title}\n\ntext\n\n## Methods ##\n\n```\n# not a heading\n```\n\nSetext\n------\n\n### Deep\n";
  const found = outline(make(doc));
  assert.deepEqual(
    found.map((h) => [h.level, h.title]),
    [[1, "Title"], [2, "Methods"], [2, "Setext"], [3, "Deep"]],
  );
  assert.equal(found[1].from, doc.indexOf("## Methods"));
  assert.deepEqual(outline(make("no headings here\n")), []);
});

test("LaTeX sectioning commands, not in comments", () => {
  const doc = "\\section{Intro}\\label{sec:intro}\n% \\section{Gone}\n\\subsection*{A \\emph{nested} {title}}\ntext \\section{mid-line}\n\\subsubsection[short]{Deeper}\n";
  const found = outline(make(doc, true));
  assert.deepEqual(
    found.map((h) => [h.level, h.title]),
    [[3, "Intro"], [4, "A nested {title}"], [5, "Deeper"]],
  );
  assert.equal(found[1].from, doc.indexOf("\\subsection*"));
});

test("the heading the cursor is under", () => {
  const h = outline(make("intro\n\n# A\n\ntext\n\n# B\n\nmore\n"));
  assert.equal(currentHeading(h, 0), -1);
  assert.equal(currentHeading(h, 7), 0);
  assert.equal(currentHeading(h, 17), 0);
  assert.equal(currentHeading(h, 18), 1);
  assert.equal(currentHeading(h, 100), 1);
});

const numbers = (doc: string, latex = false) => outline(make(doc, latex)).map((h) => h.number);

test("Markdown sections are numbered once any heading has a sec label, unnumbered ones skipped", () => {
  const doc = "# A {#sec:a}\n\n## A1\n\n## A2 {.unnumbered}\n\n## A3\n\n# B {-}\n\n# C\n\n## C1\n";
  assert.deepEqual(numbers(doc), ["1", "1.1", undefined, "1.2", undefined, "2", "2.1"]);
  assert.deepEqual(numbers("# A\n\n## B\n"), [undefined, undefined]);
  // counted from the shallowest level that has counted, as the preview does
  assert.deepEqual(numbers("## A {#sec:a}\n\n### B\n\n## C\n"), ["1", "1.1", "2"]);
});

test("LaTeX sections are numbered unless starred", () => {
  const doc = "\\section{A}\n\\subsection{A1}\n\\subsection*{No}\n\\subsection{A2}\n\\section*{Free}\n\\section{B}\n\\subsubsection{Deep}\n\\paragraph{p}\n";
  assert.deepEqual(numbers(doc, true), ["1", "1.1", undefined, "1.2", undefined, "2", "2.0.1", undefined]);
  assert.deepEqual(numbers("\\chapter{C}\n\\section{S}\n\\subsection{T}\n\\subsubsection{U}\n", true), ["1", "1.1", "1.1.1", undefined]);
});
