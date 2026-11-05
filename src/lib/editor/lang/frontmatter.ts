// YAML front matter: a `---` line at the very start of the document, up to the next `---` or `...` line. A line
// tokenizer is enough: `key:` is a property name, the rest of the line its value.

import type { BlockParser, Element } from "@lezer/markdown";
import { readRest } from "./tokens.ts";
import { frontMatterKey, frontMatterMark, frontMatterValue } from "./tags.ts";

export { frontMatterKey, frontMatterMark, frontMatterValue };

const FENCE = /^(?:---|\.\.\.)\s*$/;
const KEY = /^(\s*(?:-\s+)?)([^\s:#'"{}[\],&*!|>%@`-][^:#]*?|"[^"]*"|'[^']*')(\s*:)(?=\s|$)/;

export const frontMatterBlock: BlockParser = {
  name: "FrontMatter",
  before: "HorizontalRule",
  parse(cx, line) {
    if (cx.lineStart !== 0 || !/^---\s*$/.test(line.text)) return false;
    // closed by a `---` or `...` line, or it is no front matter
    const rest = readRest(cx, 0);
    if (!/\n(?:---|\.\.\.)[ \t]*(?:\n|$)/.test(rest)) return false;
    const children: Element[] = [cx.elt("FrontMatterMark", 0, 3)];
    let end = 3;
    while (cx.nextLine()) {
      const text = line.text;
      const at = cx.lineStart;
      if (FENCE.test(text)) {
        end = at + 3;
        children.push(cx.elt("FrontMatterMark", at, end));
        cx.nextLine();
        break;
      }
      end = at + text.length;
      const key = KEY.exec(text);
      if (key) {
        const from = at + key[1].length;
        children.push(cx.elt("FrontMatterKey", from, from + key[2].length));
        const value = text.slice(key[0].length);
        const lead = value.length - value.trimStart().length;
        if (value.trim()) children.push(cx.elt("FrontMatterValue", at + key[0].length + lead, at + text.trimEnd().length));
      }
    }
    cx.addElement(cx.elt("FrontMatter", 0, end, children));
    return true;
  },
};
