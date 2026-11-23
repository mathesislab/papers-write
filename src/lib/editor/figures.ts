// Figures shown in the text: under each line that has a figure (`![…](path)` in Markdown, `\includegraphics{path}`
// anywhere), the image itself, as wide as the column at most. The images come from the app (`image`, as the
// tooltips have them: a data URI, a PDF's first page drawn as a PNG); each is asked once per path while the editor
// lives, and drawn again only if its path changes.

import { syntaxTree } from "@codemirror/language";
import { RangeSetBuilder, StateField, type EditorState, type Extension } from "@codemirror/state";
import { Decoration, EditorView, WidgetType, type DecorationSet } from "@codemirror/view";
import { INCLUDE, imagePath, isFile } from "./hover.ts";

/** A figure of the text: its path, the end of the line it is on, and its caption (the text of `![caption](path)`). */
export type FigureAt = { path: string; line: number; caption?: string };

/** The figures of the text, in order. */
export function figuresOf(state: EditorState): FigureAt[] {
  const found: FigureAt[] = [];
  syntaxTree(state).iterate({
    enter(node) {
      if (node.name !== "Image") return;
      const url = node.node.getChild("URL");
      const path = url ? imagePath(state.sliceDoc(url.from, url.to)) : "";
      const caption = /^!\[([^]*)\]/.exec(state.sliceDoc(node.from, url ? url.from : node.to))?.[1]?.trim();
      if (isFile(path)) found.push({ path, line: state.doc.lineAt(node.to).to, ...(caption ? { caption } : {}) });
      return false;
    },
  });
  const text = state.doc.toString();
  for (const m of text.matchAll(INCLUDE)) {
    const path = m[1].trim();
    if (isFile(path)) found.push({ path, line: state.doc.lineAt((m.index ?? 0) + m[0].length).to });
  }
  return found.sort((a, b) => a.line - b.line);
}

class Figure extends WidgetType {
  readonly path: string;
  readonly caption: string | undefined;
  readonly image: (path: string) => Promise<string | null>;
  constructor(path: string, caption: string | undefined, image: (path: string) => Promise<string | null>) {
    super();
    [this.path, this.caption, this.image] = [path, caption, image];
  }

  eq(other: Figure) {
    return other.path === this.path && other.caption === this.caption;
  }

  toDOM() {
    const box = document.createElement("div");
    box.className = "cm-figure";
    const img = box.appendChild(document.createElement("img"));
    img.alt = this.path;
    this.image(this.path).then(
      (url) => (url ? (img.src = url) : box.classList.add("cm-figure-missing")),
      () => box.classList.add("cm-figure-missing"),
    );
    if (this.caption) {
      const caption = box.appendChild(document.createElement("div"));
      caption.className = "cm-figure-caption";
      caption.textContent = this.caption;
    }
    return box;
  }

  ignoreEvent() {
    return false;
  }
}

/** The figures of the text under their lines, the images from `image`. */
export function inlineFigures(image: (path: string) => Promise<string | null>): Extension {
  // each path asked once while the editor lives
  const asked = new Map<string, Promise<string | null>>();
  const cached = (path: string) => {
    let got = asked.get(path);
    if (!got) asked.set(path, (got = image(path)));
    return got;
  };
  const decorate = (state: EditorState): DecorationSet => {
    const builder = new RangeSetBuilder<Decoration>();
    for (const f of figuresOf(state)) {
      builder.add(f.line, f.line, Decoration.widget({ widget: new Figure(f.path, f.caption, cached), block: true, side: 1 }));
    }
    return builder.finish();
  };
  const field = StateField.define<DecorationSet>({
    create: decorate,
    // again as the text changes, and as the parser gets further through it
    update: (set, tr) => (tr.docChanged || syntaxTree(tr.state) !== syntaxTree(tr.startState) ? decorate(tr.state) : set),
    provide: (f) => EditorView.decorations.from(f),
  });
  return [field, theme];
}

const theme = EditorView.baseTheme({
  ".cm-figure": { padding: "6px 0 10px", textAlign: "center" },
  ".cm-figure img": { maxWidth: "100%", maxHeight: "60vh", height: "auto", borderRadius: "3px" },
  ".cm-figure-missing": { display: "none" },
  ".cm-figure-caption": { fontSize: "0.85em", color: "var(--muted)", marginTop: "0.4em", textAlign: "left" },
});
