// What the editor shows on hovering the text (CodeMirror's `hoverTooltip`): an equation rendered, a citation's reference,
// a figure's thumbnail, the target of a cross-reference. Where the pointer is (`targetAt`) and what a label stands for
// (`labelTarget`) are pure functions of the state, tested without an editor; what is rendered, looked up or read from
// disk is the app's (`HoverSources`). Opt-in: an editor without `hover` shows none.

import { language, syntaxTree } from "@codemirror/language";
import type { EditorState, Extension } from "@codemirror/state";
import { EditorView, hoverTooltip, type Tooltip } from "@codemirror/view";
import { latexMathAt } from "./lang/tex-math.ts";
import { headingTitle } from "./outline.ts";
import type { HoverSources } from "./types.ts";

type SyntaxNode = ReturnType<ReturnType<typeof syntaxTree>["resolve"]>;

/** What the pointer is over. */
export type Target =
  | { kind: "math"; tex: string; display: boolean; from: number; to: number }
  | { kind: "citation"; key: string; from: number; to: number }
  | { kind: "ref"; key: string; from: number; to: number }
  | { kind: "image"; path: string; from: number; to: number };

const MATH_NODES = new Set(["InlineMath", "DisplayMath", "MathEnvironment"]);
const INCLUDE = /\\includegraphics\*?(?:\[[^\]]*\])?\{([^}]*)\}/g;
/** math environments that are the equation itself: their wrapper is left off when rendering (`$$ … $$` is already one) */
const WRAPPERS = /^\\begin\{(?:equation\*?|displaymath)\}([^]*)\\end\{(?:equation\*?|displaymath)\}$/;

/** Whether the file is LaTeX, not Markdown. */
export const isTex = (state: EditorState) => state.facet(language)?.name === "latex";

/** The formula of a math environment as pandoc renders it: no `\label` or `\nonumber`, and no `equation` around it. */
export function environmentTex(text: string): string {
  const clean = text.replace(/\\label\{[^}]*\}/g, "").replace(/\\nonumber\b/g, "").trim();
  return (WRAPPERS.exec(clean)?.[1] ?? clean).trim();
}

/** The formula of a math node and whether it is displayed. */
function mathOf(state: EditorState, node: SyntaxNode): { tex: string; display: boolean } | null {
  if (node.name === "MathEnvironment") return { tex: environmentTex(state.sliceDoc(node.from, node.to)), display: true };
  const marks = node.getChildren("MathMark");
  if (marks.length < 2) return null;
  return { tex: state.sliceDoc(marks[0].to, marks[1].from).trim(), display: node.name === "DisplayMath" };
}

/** The path of an image in `![…](path)`: without the brackets of a path with spaces (`<a b.png>`) or a title. */
const imagePath = (raw: string) => raw.trim().replace(/^<(.*)>$/, "$1");

/** Whether a path is a file (not `https://…`). */
const isFile = (path: string) => !!path && !/^[A-Za-z][A-Za-z0-9+.-]*:/.test(path) && !path.startsWith("//");

/** What `pos` is over: a formula, a key of a citation or a cross-reference, an image (`![…](path)`,
 *  `\includegraphics{path}`); null over anything else. `side` is the side of `pos` the pointer is on. */
export function targetAt(state: EditorState, pos: number, side: -1 | 1 = 1): Target | null {
  for (let node: SyntaxNode | null = syntaxTree(state).resolve(pos, side); node; node = node.parent) {
    const { name, from, to } = node;
    if (name === "CitationKey") return { kind: "citation", key: state.sliceDoc(from, to), from, to };
    if (name === "CrossRefKey") return { kind: "ref", key: state.sliceDoc(from, to), from, to };
    if (MATH_NODES.has(name)) {
      const math = mathOf(state, node);
      return math?.tex ? { kind: "math", ...math, from, to } : null;
    }
    if (name === "Image") {
      const url = node.getChild("URL");
      const path = url ? imagePath(state.sliceDoc(url.from, url.to)) : "";
      return isFile(path) ? { kind: "image", path, from, to } : null;
    }
  }
  // a LaTeX file has no math nodes: its formulas are found in the text
  if (isTex(state)) {
    const m = latexMathAt(state.doc.toString(), pos);
    if (m) {
      const tex = m.tex.startsWith("\\begin{") ? environmentTex(m.tex) : m.tex;
      if (tex) return { kind: "math", tex, display: m.display, from: m.from, to: m.to };
    }
  }
  // `\includegraphics{path}` is a command whatever the language
  const line = state.doc.lineAt(pos);
  for (const m of line.text.matchAll(INCLUDE)) {
    const from = line.from + (m.index ?? 0);
    const to = from + m[0].length;
    if (pos >= from && pos <= to && isFile(m[1].trim())) return { kind: "image", path: m[1].trim(), from, to };
  }
  return null;
}

const KINDS: Record<string, string> = { fig: "Figure", eq: "Equation", tbl: "Table", sec: "Section", lst: "Listing" };
const SHORT = 220;
const short = (text: string) => {
  const one = text.replace(/\s+/g, " ").trim();
  return one.length > SHORT ? `${one.slice(0, SHORT - 1)}…` : one;
};

/** What the label at `node` (a LabelName of a Markdown file) stands for: the kind it is by the node it labels, and its text. */
function markdownTarget(state: EditorState, node: SyntaxNode): { kind?: string; text: string } {
  const attributes = node.parent;
  let holder = attributes?.name === "Attributes" ? attributes.parent : attributes;
  // `\label{x}` in a math environment
  for (let n: SyntaxNode | null = node; n && !holder; n = n.parent) if (n.name === "MathEnvironment") holder = n;
  if (!holder) return { text: state.doc.lineAt(node.from).text };
  const text = state.sliceDoc(holder.from, holder.to);
  if (/^(?:ATX|Setext)Heading/.test(holder.name)) return { kind: "Section", text: headingTitle(text) };
  if (holder.name === "MathEnvironment" || holder.name === "DisplayMath") {
    const math = mathOf(state, holder);
    return { kind: "Equation", text: math?.tex ?? text };
  }
  if (holder.name === "Paragraph") {
    const image = attributes?.prevSibling;
    if (image?.name === "Image") return { kind: "Figure", text: /^!\[([^\]]*)\]/.exec(state.sliceDoc(image.from, image.to))?.[1] ?? "" };
    if (text.startsWith(":")) return { kind: "Table", text: text.slice(1).replace(/\{[^}]*\}\s*$/, "") };
  }
  return { text: state.doc.lineAt(node.from).text.replace(/\{[^}]*\}\s*$/, "") };
}

/** The `{…}` argument of `\caption` in `text`, braces balanced. */
function caption(text: string): string | null {
  const at = text.search(/\\caption\*?(?:\[[^\]]*\])?\{/);
  if (at < 0) return null;
  let depth = 0;
  const start = text.indexOf("{", at);
  for (let i = start; i < text.length; i++) {
    if (text[i] === "{") depth++;
    else if (text[i] === "}" && --depth === 0) return text.slice(start + 1, i);
  }
  return null;
}

/** What the label at `at` of a LaTeX file stands for: the environment around it (an equation, a figure's or table's
 *  caption), or the section on its line. */
function texTarget(state: EditorState, at: number): { kind?: string; text: string } {
  const before = state.sliceDoc(0, at).replace(/(^|[^\\])%.*$/gm, "$1");
  const open: { name: string; at: number }[] = [];
  for (const m of before.matchAll(/\\(begin|end)\{([^}]*)\}/g)) {
    if (m[1] === "begin") open.push({ name: m[2], at: m.index ?? 0 });
    else if (open.at(-1)?.name === m[2]) open.pop();
  }
  const env = open.at(-1);
  if (env) {
    const rest = state.sliceDoc(env.at, state.doc.length);
    const end = new RegExp(`\\\\end\\{${env.name.replace("*", "\\*")}\\}`).exec(rest);
    const whole = end ? rest.slice(0, end.index + end[0].length) : rest.slice(0, 2000);
    if (/^(?:equation|align|gather|multline|eqnarray|flalign|alignat|displaymath)\*?$/.test(env.name)) {
      const tex = environmentTex(whole).replace(/^\\begin\{[^}]*\}|\\end\{[^}]*\}$/g, "").trim();
      return { kind: "Equation", text: tex };
    }
    const cap = caption(whole);
    if (cap !== null) return { kind: env.name === "table" || env.name === "table*" ? "Table" : "Figure", text: cap };
  }
  const line = state.doc.lineAt(at).text;
  const section = /\\(?:sub)*(?:section|paragraph)\*?(?:\[[^\]]*\])?\{([^}]*)\}/.exec(line);
  if (section) return { kind: "Section", text: section[1] };
  return { text: line.replace(/\\label\{[^}]*\}/g, "") };
}

/** What the label `key` stands for, as the line of the document it labels: "Figure: The hysteresis loop", "Equation:
 *  \tau_b = …", "Section: Introduction". Null when no label of that name is defined. */
export function labelTarget(state: EditorState, key: string): string | null {
  let node = null as SyntaxNode | null;
  syntaxTree(state).iterate({
    enter(n) {
      if (!node && n.name === "LabelName" && state.sliceDoc(n.from, n.to) === key) node = n.node;
      return !node;
    },
  });
  if (!node) return null;
  const at = isTex(state) ? texTarget(state, node.from) : markdownTarget(state, node);
  const kind = at.kind ?? KINDS[key.split(":")[0]];
  const text = short(at.text);
  return kind ? `${kind}: ${text}` : text;
}

const THUMBNAIL = 240;

/** The tooltip of what `source` made of `target`, or null. */
async function contentOf(view: EditorView, target: Target, sources: HoverSources): Promise<HTMLElement | null> {
  const dom = document.createElement("div");
  switch (target.kind) {
    case "math": {
      if (!sources.math) return null;
      dom.className = "cm-hover-math";
      // the app renders the formula itself (pandoc's MathML of the text typed): the text is escaped there
      dom.innerHTML = await sources.math(target.tex, target.display);
      return dom.querySelector("math") ? dom : null;
    }
    case "citation": {
      const found = await sources.citation?.(target.key);
      if (!found) return null;
      const head = document.createElement("div");
      head.className = "cm-hover-head";
      head.textContent = found.label;
      dom.append(head);
      if (found.detail) {
        const detail = document.createElement("div");
        detail.textContent = found.detail;
        dom.append(detail);
      }
      return dom;
    }
    case "image": {
      const url = await sources.image?.(target.path);
      if (!url) return null;
      const img = document.createElement("img");
      img.src = url;
      img.alt = target.path;
      img.style.maxWidth = `${THUMBNAIL}px`;
      dom.className = "cm-hover-image";
      dom.append(img);
      return dom;
    }
    case "ref": {
      const text = labelTarget(view.state, target.key);
      if (text === null) return null;
      dom.textContent = text;
      return dom;
    }
  }
}

const theme = EditorView.theme({
  // the box is the editor's (editor.ts: every tooltip of the editor); this is what is in it
  ".cm-hover": { padding: "6px 10px", maxWidth: "min(32em, 90vw)", lineHeight: "1.4" },
  ".cm-hover-head": { fontWeight: "650" },
  ".cm-hover-math": { fontSize: "1.15em", overflowX: "auto" },
  ".cm-hover-image img": { display: "block", height: "auto", maxHeight: "240px" },
});

/** The tooltips of the editor: 300 ms after the pointer stops over a formula, a citation key, an image or a
 *  cross-reference; none while a button is down (a selection being made). */
export function hover(sources: HoverSources): Extension {
  let down = false;
  const release = () => {
    down = false;
    document.removeEventListener("mouseup", release, true);
  };
  return [
    EditorView.domEventHandlers({
      mousedown() {
        down = true;
        document.addEventListener("mouseup", release, true);
        return false;
      },
    }),
    hoverTooltip(
      async (view, pos, side): Promise<Tooltip | null> => {
        if (down) return null;
        const target = targetAt(view.state, pos, side);
        if (!target) return null;
        let dom: HTMLElement | null;
        try {
          dom = await contentOf(view, target, sources);
        } catch {
          return null;
        }
        if (!dom || down) return null;
        dom.classList.add("cm-hover");
        return { pos: target.from, end: target.to, above: true, create: () => ({ dom }) };
      },
      { hoverTime: 300 },
    ),
    theme,
  ];
}
