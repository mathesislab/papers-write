// Markdown as it is written for pandoc and texmark: YAML front matter, math and LaTeX (and, see cite.ts, citations and
// cross-references), as Lezer Markdown extensions. `paperMarkdown` is the list to give `markdown({ extensions })`.

import type { MarkdownConfig, MarkdownExtension } from "@lezer/markdown";
import { frontMatterBlock } from "./frontmatter.ts";
import { latexBlock, latexInline } from "./latex.ts";
import { mathBlock, mathInline } from "./math.ts";
import * as t from "./tags.ts";

export * from "./tags.ts";

const frontMatter: MarkdownConfig = {
  defineNodes: [
    { name: "FrontMatter", block: true },
    { name: "FrontMatterMark", style: t.frontMatterMark },
    { name: "FrontMatterKey", style: t.frontMatterKey },
    { name: "FrontMatterValue", style: t.frontMatterValue },
  ],
  parseBlock: [frontMatterBlock],
};

const math: MarkdownConfig = {
  defineNodes: [
    "InlineMath",
    { name: "DisplayMath", block: true },
    { name: "MathMark", style: t.mathMark },
    { name: "MathCommand", style: t.mathCommand },
    { name: "MathOperator", style: t.mathOperator },
    { name: "MathBrace", style: t.mathBrace },
    { name: "MathNumber", style: t.mathNumber },
  ],
  parseBlock: [mathBlock],
  parseInline: [mathInline],
};

const latex: MarkdownConfig = {
  defineNodes: [
    { name: "MathEnvironment", block: true },
    { name: "LatexEnvironment", block: true },
    { name: "LatexCommand", style: t.latexCommand },
    { name: "LatexBrace", style: t.latexBrace },
    { name: "LatexEnvName", style: t.latexEnvName },
  ],
  parseBlock: [latexBlock],
  parseInline: [latexInline],
};

/** The extensions of Markdown for academic text, to give to `markdown({ extensions: paperMarkdown })`. */
export const paperMarkdown: MarkdownExtension[] = [frontMatter, math, latex];
