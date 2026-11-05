// The highlight tags of the academic Markdown (front matter, math, LaTeX, citations, cross-references). The editor's
// HighlightStyle gives them their colours; another app may style them its own way.

import { Tag } from "@lezer/highlight";

export const frontMatterMark = Tag.define("frontMatterMark");
export const frontMatterKey = Tag.define("frontMatterKey");
export const frontMatterValue = Tag.define("frontMatterValue");

/** Math: the `$` marks, the commands, `^` and `_`, the braces, the numbers. */
export const mathMark = Tag.define("mathMark");
export const mathCommand = Tag.define("mathCommand");
export const mathOperator = Tag.define("mathOperator");
export const mathBrace = Tag.define("mathBrace");
export const mathNumber = Tag.define("mathNumber");

/** LaTeX in the text: `\name`, the braces, the name of an environment. */
export const latexCommand = Tag.define("latexCommand");
export const latexBrace = Tag.define("latexBrace");
export const latexEnvName = Tag.define("latexEnvName");

/** Citations: `[ ] ; @ -`, the key; cross-references and labels. */
export const citeMark = Tag.define("citeMark");
export const citationKey = Tag.define("citationKey");
export const crossRefKey = Tag.define("crossRefKey");
export const labelName = Tag.define("labelName");
