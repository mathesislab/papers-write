// The review of the document in a window: who the user is, the items loaded from the backend (`review_*`, the
// commands of papers-write-core's review) carried onto the text as typed, the mode (editing or suggesting), and what
// the user does with them. The editor is told the items (`setReview`) and the mode by the page, which reads them here.

import { invoke } from "@tauri-apps/api/core";
import type { FileEditor, ReviewHandlers, ReviewItem, ReviewMode } from "papers-write/editor";

export type Me = { name: string; email: string; slug: string };
type Loaded = { items: ReviewItem[]; errors: string[]; people: Record<string, string> };
type Decision = "accepted" | "rejected" | "resolved" | "reopened" | "withdrawn";

export type Deps = {
  /** the text as typed */
  text: () => string;
  editor: () => FileEditor | undefined;
  /** told for a moment (a note at the foot of the window) */
  say: (what: string) => void;
  /** suggestions accepted or rejected, the decisions in the files of the review: told with a message for a commit */
  decided?: (message: string) => void;
};

export class ReviewSession {
  /** the user, once the document is a file (none while untitled) */
  me = $state<Me | null>(null);
  items = $state.raw<ReviewItem[]>([]);
  /** the names of the people of the review, by slug */
  people = $state.raw<Record<string, string>>({});
  /** the review files that could not be read, and what went wrong with the last step */
  errors = $state.raw<string[]>([]);
  problem = $state("");
  mode = $state<ReviewMode>("edit");
  /** the item last clicked in the text, to show in the panel */
  selected = $state("");

  #deps: Deps;
  #seq = 0;
  #timer: ReturnType<typeof setTimeout> | undefined;

  constructor(deps: Deps) {
    this.#deps = deps;
  }

  /** What the editor asks of the app, once there is a user. */
  readonly handlers = $derived.by<ReviewHandlers | undefined>(() => {
    const me = this.me;
    if (!me) return undefined;
    return {
      me: me.slug,
      suggest: (s) =>
        this.#write(async () => {
          const id = await invoke<string>("review_suggest", { text: this.#deps.text(), id: s.id ?? null, from: s.from, to: s.to, insert: s.insert });
          this.reload(300);
          return id;
        }),
      withdraw: (id) =>
        this.#write(async () => {
          await invoke("review_withdraw", { id });
          this.reload(300);
        }),
      select: (id) => (this.selected = id),
    };
  });

  /** A step of the user's, its failure kept in `problem` and thrown on. */
  async #write<T>(step: () => Promise<T>): Promise<T> {
    try {
      this.problem = "";
      return await step();
    } catch (e) {
      this.problem = String(e);
      throw e;
    }
  }

  /** The document is (another) file: who the user is for it, and its items once the editor has its text. */
  async start(saved: boolean) {
    this.items = [];
    this.errors = [];
    this.selected = "";
    this.me = null;
    if (!saved) return;
    try {
      this.me = await invoke<Me>("review_me");
    } catch (e) {
      this.problem = String(e);
      return;
    }
    // the editor reads its file after it is made: its text is there a moment later
    for (let i = 0; i < 20; i++) {
      const doc = this.#deps.editor()?.view()?.state.doc;
      if (doc && doc.length > 0) break;
      await new Promise((r) => setTimeout(r, 100));
    }
    this.reload();
  }

  /** The items read again, `delay` ms from the last call (typing, a run of own suggestions and the files of a pull
   *  each ask again before it is time). A reply that is not of the last request is dropped. */
  reload(delay = 0) {
    clearTimeout(this.#timer);
    if (!this.me) return;
    this.#timer = setTimeout(async () => {
      const seq = ++this.#seq;
      try {
        const got = await invoke<Loaded>("review_items", { text: this.#deps.text() });
        if (seq !== this.#seq) return;
        [this.items, this.errors, this.people] = [got.items, got.errors, got.people];
      } catch (e) {
        if (seq === this.#seq) this.problem = String(e);
      }
    }, delay);
  }

  /** Review ▸ Suggest Changes: the other mode (not for untitled text). */
  toggleMode() {
    if (!this.me) return this.#deps.say("Save the document first, to review it");
    this.mode = this.mode === "suggest" ? "edit" : "suggest";
    invoke("review_mode", { suggesting: this.mode === "suggest" });
  }

  async comment(from: number, to: number, body: string) {
    await this.#write(() => invoke("review_comment", { text: this.#deps.text(), from, to, body }));
    this.reload();
  }

  async reply(on: string, body: string) {
    await this.#write(() => invoke("review_reply", { on, body }));
    this.reload();
  }

  async decide(on: string, decision: Decision) {
    await this.#write(() => invoke("review_decide", { on, decision }));
    this.reload();
  }

  /** A counter-suggestion to the suggestion `item`: `insert` in place of all it puts in the text. */
  async counter(item: ReviewItem, insert: string, note?: string) {
    const len = (item.replacement ?? "").length;
    await this.#write(() => invoke("review_counter", { text: this.#deps.text(), on: item.id, from: 0, to: len, insert, note: note || null }));
    this.reload();
  }

  /** The suggestion in the text (the editor writes the file), then decided; a counter-suggestion accepts what it
   *  answers too. */
  async #accept(item: ReviewItem) {
    const editor = this.#deps.editor();
    if (!editor) return;
    await editor.accept(item);
    await invoke("review_decide", { on: item.id, decision: "accepted" });
    if (item.on) await invoke("review_decide", { on: item.on, decision: "accepted" });
  }

  async accept(item: ReviewItem) {
    await this.#write(() => this.#accept(item));
    this.reload();
    this.#decided("Accept", [item]);
  }

  /** The commit offered for what was decided: "Accept 3 suggestions of Jane Doe". */
  #decided(verb: "Accept" | "Reject", items: ReviewItem[]) {
    const names = [...new Set(items.map((i) => i.author))];
    const who = names.length > 1 ? `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}` : names[0];
    this.#deps.decided?.(`${verb} ${items.length} suggestion${items.length === 1 ? "" : "s"} of ${who}`);
  }

  /** Rejected, and with it the counter-suggestions still open that answer it. */
  async reject(item: ReviewItem) {
    await this.#write(async () => {
      await invoke("review_decide", { on: item.id, decision: "rejected" });
      for (const c of this.items.filter((i) => i.on === item.id && i.state === "open")) await invoke("review_decide", { on: c.id, decision: "rejected" });
    });
    this.reload();
  }

  #open = () => this.items.filter((i) => i.kind === "suggestion" && i.state === "open");

  /** Review ▸ Accept All: the open suggestions of everyone, from the end of the text backwards (so those before are
   *  where they were). Not those that are outdated or orphaned, nor one that a counter-suggestion answers (accept
   *  that), nor one that overlaps another taken: they are told. */
  async acceptAll() {
    const open = this.#open();
    const answered = new Set(open.map((i) => i.on).filter(Boolean));
    const ranked = open.filter((i) => !answered.has(i.id) && ["exact", "moved"].includes(i.status.type)).sort((a, b) => b.at.from - a.at.from || b.at.to - a.at.to);
    const take: ReviewItem[] = [];
    let limit = Infinity;
    for (const i of ranked) {
      if (i.at.to > limit) continue;
      take.push(i);
      limit = i.at.from;
    }
    if (!open.length) return this.#deps.say("No suggestions to accept");
    if (!take.length) return this.#deps.say("None of the suggestions can be accepted at once: they are outdated or answered");
    const left = open.length - take.length;
    const ok = await invoke<boolean>("review_confirm", {
      message: `Accept ${take.length} suggestion${take.length === 1 ? "" : "s"} of everyone?${left ? ` ${left} more (outdated, answered by a counter-suggestion or overlapping) stay open.` : ""}`,
      ok: "Accept All",
    });
    if (!ok) return;
    try {
      await this.#write(async () => {
        for (const i of take) await this.#accept(i);
      });
      this.#decided("Accept", take);
    } finally {
      this.reload();
    }
  }

  /** Review ▸ Reject All: the open suggestions of everyone (comments stay). */
  async rejectAll() {
    const open = this.#open();
    if (!open.length) return this.#deps.say("No suggestions to reject");
    const ok = await invoke<boolean>("review_confirm", { message: `Reject ${open.length} suggestion${open.length === 1 ? "" : "s"} of everyone?`, ok: "Reject All" });
    if (!ok) return;
    try {
      await this.#write(async () => {
        for (const i of open) await invoke("review_decide", { on: i.id, decision: "rejected" });
      });
      this.#decided("Reject", open);
    } finally {
      this.reload();
    }
  }

  /** Next or Previous Change: the open item after (before) the cursor, wrapping round, brought into view. */
  step(forward: boolean) {
    const editor = this.#deps.editor();
    const head = editor?.view()?.state.selection.main.head ?? 0;
    const open = this.items.filter((i) => i.state === "open");
    if (!open.length) return this.#deps.say("No changes or comments to go to");
    const item = forward ? (open.find((i) => i.at.from > head) ?? open[0]) : (open.findLast((i) => i.at.from < head) ?? open[open.length - 1]);
    this.show(item.id);
  }

  /** The item brought into view in the text and marked in the panel. */
  show(id: string) {
    this.selected = id;
    this.#deps.editor()?.scrollTo(id);
  }
}
