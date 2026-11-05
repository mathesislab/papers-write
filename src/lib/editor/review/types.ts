// The review as the editor sees it: the suggestions and comments of everyone, carried onto the text in the editor
// by the app (papers-write-core's `review`, whose `Item` these types mirror as serde writes it), and what the editor
// asks of the app when the user suggests or comments. Positions are UTF-16 code units of the editor's text, as
// CodeMirror counts them.

export type ReviewKind = "suggestion" | "comment" | "reply" | "decision";
export type ReviewDecision = "accepted" | "rejected" | "resolved" | "reopened" | "withdrawn";
export type ReviewState = "open" | "accepted" | "rejected" | "resolved" | "withdrawn";

/** One record of a person's review file, as written. */
export type ReviewRecord = {
  /** `<slug>-<n>`: the slug is the person's (their file's name) */
  id: string;
  kind: ReviewKind;
  /** RFC 3339, UTC */
  time: string;
  round?: string;
  base?: string;
  start?: number;
  end?: number;
  prefix?: string;
  exact?: string;
  suffix?: string;
  /** a suggestion's replacement (empty: a deletion) */
  insert?: string;
  /** a comment's or reply's body, a suggestion's note */
  text?: string;
  /** the record answered (reply, counter-suggestion) or decided (decision) */
  on?: string;
  decision?: ReviewDecision;
  supersedes?: string;
};

/** How an item sits on the current text. */
export type ReviewStatus = { type: "exact" } | { type: "moved" } | { type: "outdated"; now: string } | { type: "orphaned" };

/** A suggestion or comment as it stands on the current text. */
export type ReviewItem = {
  id: string;
  kind: "suggestion" | "comment";
  /** the person's name */
  author: string;
  /** the suggestion a counter-suggestion answers */
  on?: string | null;
  /** its latest record */
  record: ReviewRecord;
  /** replies and counter-suggestions, in time order */
  thread: ReviewRecord[];
  state: ReviewState;
  /** where it is in the editor's text (a suggestion: the text it replaces; empty for an insertion) */
  at: { from: number; to: number };
  status: ReviewStatus;
  /** a suggestion's text to put at `at` */
  replacement?: string | null;
};

/** Edit: typing changes the text. Suggest: typing makes the user's suggestions; the text does not change. */
export type ReviewMode = "edit" | "suggest";

/** What the editor shows of the suggestions: all markup, as if all were accepted, or as if none were. */
export type ReviewView = "markup" | "accepted" | "original";

/** A suggestion of the user's, made or changed by typing in suggest mode. */
export type Suggested = {
  /** the item it changes, none for a new one */
  id?: string;
  /** the text it replaces, in the editor's text */
  from: number;
  to: number;
  /** what it puts there instead ("" deletes) */
  insert: string;
};

/** What the editor's review asks of the app. */
export type ReviewHandlers = {
  /** the user's slug: their items are those whose id starts with `<slug>-` */
  me: string;
  /** a suggestion typed (new, or one of the user's changed): written by the app, which gives its id */
  suggest: (s: Suggested) => Promise<string>;
  /** a suggestion of the user's typed away to nothing */
  withdraw: (id: string) => Promise<void>;
  /** an item clicked in the text */
  select?: (id: string) => void;
};
