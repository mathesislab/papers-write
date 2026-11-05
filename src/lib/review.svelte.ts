// The review of the document in a window: who the user is, the items loaded from the backend (`review_*`, the
// commands of papers-write-core's review) carried onto the text as typed, the mode (editing or suggesting), and what
// the user does with them. The editor is told the items (`setReview`) and the mode by the page, which reads them here.

import { invoke } from "@tauri-apps/api/core";
import type { FileEditor, ReviewHandlers, ReviewItem, ReviewMode, ReviewView } from "papers-write/editor";
import { decided, type Entry } from "$lib/commitLog";

export type Me = { name: string; email: string; slug: string };
type Loaded = { items: ReviewItem[]; errors: string[]; people: Record<string, string> };
type Decision = "accepted" | "rejected" | "resolved" | "reopened" | "withdrawn";

/** Which suggestions Accept All and Reject All take: those of a person (their slug) and/or of a round; none: all. */
export type Scope = { person?: string; round?: string };

export type Deps = {
  /** the text as typed */
  text: () => string;
  editor: () => FileEditor | undefined;
  /** told for a moment (a note at the foot of the window) */
  say: (what: string) => void;
  /** what was done to the review (suggestions accepted or rejected, the review cleaned up), for the message of the
   *  next commit */
  log?: (entry: Entry) => void;
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
  /** what is drawn of the suggestions: All Markup, Accepted or Original (suggesting is All Markup only) */
  view = $state<ReviewView>("markup");
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

  /** Who the user is, asked again (the settings changed it); the items read again for the new person. */
  async identity() {
    if (!this.me) return;
    try {
      this.me = await invoke<Me>("review_me");
      this.reload();
    } catch (e) {
      this.problem = String(e);
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
    if (this.mode === "suggest") this.setView("markup");
  }

  /** Review ▸ All Markup, Accepted, Original: choosing another than All Markup leaves suggest mode. */
  setView(view: ReviewView) {
    if (view !== "markup" && this.mode === "suggest") this.toggleMode();
    this.view = view;
    invoke("review_view", { view });
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
    this.#decided("accept", [item]);
  }

  /** Logged for the message of the next commit: "Accept 3 suggestions of Jane Doe". */
  #decided(decision: "accept" | "reject", items: ReviewItem[]) {
    for (const e of decided(decision, items)) this.#deps.log?.(e);
  }

  /** Rejected, and with it the counter-suggestions still open that answer it. */
  async reject(item: ReviewItem) {
    await this.#write(async () => {
      await invoke("review_decide", { on: item.id, decision: "rejected" });
      for (const c of this.items.filter((i) => i.on === item.id && i.state === "open")) await invoke("review_decide", { on: c.id, decision: "rejected" });
    });
    this.reload();
    this.#decided("reject", [item]);
  }

  #open = (scope: Scope = {}) =>
    this.items.filter(
      (i) =>
        i.kind === "suggestion" &&
        i.state === "open" &&
        (!scope.person || i.id.replace(/-\d+$/, "") === scope.person) &&
        (!scope.round || i.record.round === scope.round),
    );

  /** The scope in words: "everyone", "Jane Doe", "round 'Draft of 5 Nov'", "Jane Doe in round 'Draft of 5 Nov'". */
  describe(scope: Scope = {}): string {
    const who = scope.person ? (this.people[scope.person] ?? scope.person) : "";
    const round = scope.round ? `round '${scope.round}'` : "";
    return who && round ? `${who} in ${round}` : who || round || "everyone";
  }

  /** Accept All: the open suggestions of `scope` (everyone by default; the Review menu), from the end of the text backwards (so those before are
   *  where they were). Not those that are outdated or orphaned, nor one that a counter-suggestion answers (accept
   *  that), nor one that overlaps another taken: they are told. */
  async acceptAll(scope: Scope = {}) {
    const open = this.#open(scope);
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
      message: `Accept ${take.length} suggestion${take.length === 1 ? "" : "s"} of ${this.describe(scope)}?${left ? ` ${left} more (outdated, answered by a counter-suggestion or overlapping) stay open.` : ""}`,
      ok: "Accept All",
    });
    if (!ok) return;
    try {
      await this.#write(async () => {
        for (const i of take) await this.#accept(i);
      });
      this.#decided("accept", take);
    } finally {
      this.reload();
    }
  }

  /** Reject All: the open suggestions of `scope` (everyone by default; the Review menu). Comments stay. */
  async rejectAll(scope: Scope = {}) {
    const open = this.#open(scope);
    if (!open.length) return this.#deps.say("No suggestions to reject");
    const ok = await invoke<boolean>("review_confirm", { message: `Reject ${open.length} suggestion${open.length === 1 ? "" : "s"} of ${this.describe(scope)}?`, ok: "Reject All" });
    if (!ok) return;
    try {
      await this.#write(async () => {
        for (const i of open) await invoke("review_decide", { on: i.id, decision: "rejected" });
      });
      this.#decided("reject", open);
    } finally {
      this.reload();
    }
  }

  /** Review ▸ Clean Up Review…: the user's resolved records and the bases nothing needs removed, once the sheet has said
   *  what goes. The files of the others are theirs to clean (papers-write-core's `review::cleanup`). */
  async cleanUp() {
    if (!this.me) return this.#deps.say("Save the document first, to review it");
    try {
      const plan = await this.#write(() => invoke<{ records: string[]; bases: string[]; others: number }>("review_cleanup_plan"));
      if (!plan.records.length && !plan.bases.length) return this.#deps.say("Nothing to clean up");
      const n = (k: number, one: string, many = `${one}s`) => `${k} ${k === 1 ? one : many}`;
      const goes = [plan.records.length ? `${n(plan.records.length, "resolved record")} of yours` : "", plan.bases.length ? n(plan.bases.length, "base") : ""].filter(Boolean).join(", ");
      const others = plan.others ? ` The ${n(plan.others, "resolved item")} of others stay in their files until they clean up.` : "";
      const ok = await invoke<boolean>("review_confirm", {
        message: `Remove ${goes}?${others} Git history keeps everything.`,
        ok: "Clean Up",
      });
      if (!ok) return;
      await this.#write(() => invoke("review_cleanup"));
      this.#deps.say("Review cleaned up");
      this.#deps.log?.({ did: "clean up the review" });
    } catch {
      // `problem` says what went wrong
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
