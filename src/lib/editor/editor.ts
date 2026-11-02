// The editor of the review: CodeMirror 6, Markdown, the markup dimmed (the headings, the emphasis, the quotes drawn
// as they read), in the system font.

import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorState, Transaction, type Extension } from "@codemirror/state";
import { EditorView, drawSelection, keymap, placeholder } from "@codemirror/view";
import { tags } from "@lezer/highlight";

const style = HighlightStyle.define([
  { tag: tags.heading, fontWeight: "650", color: "var(--md-heading)" },
  // the marks: # * > ` and the brackets of links
  { tag: tags.processingInstruction, color: "var(--muted)", fontWeight: "400" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strong, fontWeight: "650" },
  { tag: tags.quote, color: "var(--md-quote)", fontStyle: "italic" },
  { tag: tags.link, color: "var(--accent)" },
  { tag: tags.url, color: "var(--muted)" },
  { tag: tags.monospace, fontFamily: "ui-monospace, Menlo, monospace", fontSize: "12.5px" },
]);

const theme = EditorView.theme({
  "&": { height: "100%", fontSize: "13.5px", color: "var(--fg)", background: "transparent" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { overflow: "auto", fontFamily: "-apple-system, BlinkMacSystemFont, system-ui, sans-serif" },
  ".cm-content": { lineHeight: "1.6", padding: "10px 0 40px", caretColor: "var(--accent)" },
  ".cm-line": { padding: "0 18px" },
  ".cm-cursor": { borderLeftColor: "var(--accent)", borderLeftWidth: "1.5px" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": { background: "var(--selection)" },
  ".cm-placeholder": { color: "var(--muted)" },
});

/** An editor of `text` in `parent`; `changed` with the text after each change typed. */
export function makeEditor(parent: HTMLElement, text: string, changed: (text: string) => void, more: Extension[] = []) {
  return new EditorView({
    parent,
    state: EditorState.create({
      doc: text,
      extensions: [
        history(),
        drawSelection(),
        keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
        markdown(),
        syntaxHighlighting(style),
        theme,
        EditorView.lineWrapping,
        placeholder("Your review, in Markdown: ## for its sections, a section a round, say."),
        EditorView.updateListener.of((update) => {
          if (update.docChanged && update.transactions.some((t) => t.annotation(Transaction.userEvent))) {
            changed(update.state.doc.toString());
          }
        }),
        ...more,
      ],
    }),
  });
}

/** The whole text replaced (read again from the file), not to be undone into. */
export function replaceText(view: EditorView, text: string) {
  view.dispatch({
    changes: { from: 0, to: view.state.doc.length, insert: text },
    annotations: Transaction.addToHistory.of(false),
  });
}

/** The words of a text. */
export const wordsOf = (text: string) => text.split(/\s+/).filter((w) => /\w/.test(w)).length;
