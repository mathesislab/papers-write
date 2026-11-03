// The symbols of the review: sequences typed that become one (`->` →, `CO_2` CO₂, `m s^-1` m s⁻¹), and LaTeX names
// completed with Tab as in the Julia REPL (`\alpha` α). Pure: the editor (editor.ts) applies them.

/** A sequence typed (`keys`) and what it becomes (`insert`), only where `when` allows it: given the text of the line
 *  before the sequence. */
export type Rule = { keys: string; insert: string; when?: (head: string) => boolean };

/** After a letter, a digit or `)`: a superscript is the power of something. */
const power = (head: string) => /[\p{L}\p{Nd})]$/u.test(head);
/** After a letter: a subscript is the index of something (CO_2). */
const index = (head: string) => /\p{L}$/u.test(head);
/** Not in a line of nothing but dashes, bars and colons (`---`, `|---|---|`, `|:--:|`): Markdown's own. */
const dash = (head: string) => !/^[\s|:-]*$/.test(head);

const rule = (keys: string, insert: string, when?: Rule["when"]): Rule => ({ keys, insert, when });

/** The sequences replaced as the last character is typed. Where a sequence ends in a symbol, it is the one an earlier
 *  sequence made: `<-` ← and then `>` ↔, `<=` ≤ and then `>` ⇔, `--` – and then `-` —. */
export const DEFAULTS: readonly Rule[] = [
  rule("->", "→"),
  rule("<-", "←"),
  rule("←>", "↔"),
  rule("=>", "⇒"),
  rule("≤>", "⇔"),
  rule(">=", "≥"),
  rule("<=", "≤"),
  rule("!=", "≠"),
  rule("~=", "≈"),
  rule("+-", "±"),
  rule("--", "–", dash),
  rule("–-", "—"),
  rule("...", "…"),
  rule("^2", "²", power),
  rule("^3", "³", power),
  rule("^-1", "⁻¹", power),
  rule("^-2", "⁻²", power),
  rule("^-3", "⁻³", power),
  rule("_2", "₂", index),
  rule("_3", "₃", index),
  rule("_4", "₄", index),
];

/** The rules with the user's own pairs (sequence, replacement) on them: a pair of a sequence the rules have gives it
 *  another replacement, an empty replacement turns it off, any other adds one. A sequence of less than two
 *  characters is ignored. The longest sequence comes first. */
export function merged(defaults: readonly Rule[], overrides: readonly [string, string][]): Rule[] {
  const rules = new Map(defaults.map((r) => [r.keys, r]));
  for (const [keys, insert] of overrides) {
    if (keys.length < 2) continue;
    const was = rules.get(keys);
    if (!insert) rules.delete(keys);
    else rules.set(keys, { ...was, keys, insert });
  }
  return [...rules.values()].sort((a, b) => b.keys.length - a.keys.length);
}

/** The replacement made by typing `char` after `before`, the text of the line before the cursor: `from` characters of
 *  `before`, back from the cursor, with the character typed (not yet in the text) are replaced by `insert`. Null when
 *  no sequence of `rules` is completed. */
export function typed(before: string, char: string, rules: readonly Rule[]): { from: number; insert: string } | null {
  const text = before + char;
  let found: Rule | undefined;
  for (const r of rules) {
    if (found && r.keys.length <= found.keys.length) continue;
    if (!text.endsWith(r.keys) || (r.when && !r.when(text.slice(0, text.length - r.keys.length)))) continue;
    found = r;
  }
  return found ? { from: found.keys.length - char.length, insert: found.insert } : null;
}

const SUPER = "⁰¹²³⁴⁵⁶⁷⁸⁹";
const SUB = "₀₁₂₃₄₅₆₇₈₉";
const digits = (marks: string, prefix: string): [string, string][] => [...marks].map((m, i) => [prefix + i, m]);

/** The LaTeX names (with their backslash) and what they stand for, the Greek letters as the Julia REPL has them. */
export const LATEX: ReadonlyMap<string, string> = new Map<string, string>([
  ...Object.entries({
    alpha: "α", beta: "β", gamma: "γ", delta: "δ", epsilon: "ϵ", varepsilon: "ε", zeta: "ζ", eta: "η", theta: "θ",
    vartheta: "ϑ", iota: "ι", kappa: "κ", lambda: "λ", mu: "μ", nu: "ν", xi: "ξ", omicron: "ο", pi: "π", rho: "ρ",
    varrho: "ϱ", sigma: "σ", varsigma: "ς", tau: "τ", upsilon: "υ", phi: "ϕ", varphi: "φ", chi: "χ", psi: "ψ",
    omega: "ω",
    Alpha: "Α", Beta: "Β", Gamma: "Γ", Delta: "Δ", Epsilon: "Ε", Zeta: "Ζ", Eta: "Η", Theta: "Θ", Iota: "Ι",
    Kappa: "Κ", Lambda: "Λ", Mu: "Μ", Nu: "Ν", Xi: "Ξ", Omicron: "Ο", Pi: "Π", Rho: "Ρ", Sigma: "Σ", Tau: "Τ",
    Upsilon: "Υ", Phi: "Φ", Chi: "Χ", Psi: "Ψ", Omega: "Ω",
    // units
    deg: "°", degC: "°C", permil: "‰", micro: "µ", angstrom: "Å",
    // arithmetic and relations
    pm: "±", mp: "∓", times: "×", cdot: "·", div: "÷", sqrt: "√", cbrt: "∛", half: "½",
    approx: "≈", sim: "∼", simeq: "≃", propto: "∝", equiv: "≡", neq: "≠", ne: "≠", leq: "≤", le: "≤", geq: "≥",
    ge: "≥", ll: "≪", gg: "≫", lesssim: "≲", gtrsim: "≳",
    // calculus and sets
    infty: "∞", partial: "∂", nabla: "∇", sum: "∑", prod: "∏", int: "∫", prime: "′", ell: "ℓ", hbar: "ħ",
    in: "∈", notin: "∉", subset: "⊂", subseteq: "⊆", supset: "⊃", supseteq: "⊇", cup: "∪", cap: "∩",
    emptyset: "∅", forall: "∀", exists: "∃", perp: "⊥", parallel: "∥",
    // arrows
    to: "→", gets: "←", rightarrow: "→", leftarrow: "←", leftrightarrow: "↔", Rightarrow: "⇒", Leftarrow: "⇐",
    Leftrightarrow: "⇔", uparrow: "↑", downarrow: "↓", up: "↑", down: "↓", updownarrow: "↕", nearrow: "↗",
    searrow: "↘", mapsto: "↦", rightleftharpoons: "⇌",
    // marks
    circ: "∘", bullet: "•", dagger: "†", star: "⋆", check: "✓", cross: "✗",
  }).map(([name, symbol]): [string, string] => [`\\${name}`, symbol]),
  ...digits(SUPER, "\\^"),
  ["\\^-", "⁻"],
  ["\\^+", "⁺"],
  ...digits(SUB, "\\_"),
  ["\\_-", "₋"],
  ["\\_+", "₊"],
]);

/** The replacement of the LaTeX name at the end of `before`, the text of the line before the cursor: `length`
 *  characters back from the cursor are replaced by `insert`. Null when it ends in no name of `table`. */
export function latex(before: string, table: ReadonlyMap<string, string> = LATEX): { length: number; insert: string } | null {
  const name = /\\(?:[A-Za-z]+|[\^_][0-9+-])$/.exec(before)?.[0];
  const insert = name === undefined ? undefined : table.get(name);
  return name !== undefined && insert !== undefined ? { length: name.length, insert } : null;
}

/** A sequence or name, and the symbol it gives. */
export type Entry = { keys: string; insert: string };

/** The entries of `entries` that `query` finds, in the keys or in the symbol, those starting with it first. */
export function find(entries: readonly Entry[], query: string): Entry[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...entries];
  const found = entries.filter((e) => e.keys.toLowerCase().includes(q) || e.insert.includes(q));
  const starts = (e: Entry) => e.keys.toLowerCase().replace(/^\\/, "").startsWith(q.replace(/^\\/, ""));
  return [...found.filter(starts), ...found.filter((e) => !starts(e))];
}
