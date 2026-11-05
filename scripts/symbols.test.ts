// The symbols typed and the LaTeX names of the review (src/lib/symbols.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { DEFAULTS, LATEX, find, latex, merged, typed } from "../src/lib/symbols.ts";

const rules = merged(DEFAULTS, []);

/** The line `text` typed one character at a time, each completed sequence replaced as the editor does. */
function type(text: string, r = rules): string {
  let line = "";
  for (const char of text) {
    const hit = typed(line, char, r);
    line = hit ? line.slice(0, line.length - hit.from) + hit.insert : line + char;
  }
  return line;
}

test("arrows, relations, dashes, dots", () => {
  assert.equal(type("a -> b"), "a → b");
  assert.equal(type("a <- b"), "a ← b");
  assert.equal(type("a <-> b"), "a ↔ b");
  assert.equal(type("a => b"), "a ⇒ b");
  assert.equal(type("a <=> b"), "a ⇔ b");
  assert.equal(type("a >= b, c <= d"), "a ≥ b, c ≤ d");
  assert.equal(type("a != b, c ~= d, e +- f"), "a ≠ b, c ≈ d, e ± f");
  assert.equal(type("2010--2020"), "2010–2020");
  assert.equal(type("well --- yes"), "well — yes");
  assert.equal(type("wait..."), "wait…");
});

test("the replacement reaches back as far as the sequence typed", () => {
  assert.deepEqual(typed("a -", ">", rules), { from: 1, insert: "→" });
  assert.deepEqual(typed("a ←", ">", rules), { from: 1, insert: "↔" });
  assert.deepEqual(typed("m s^-", "1", rules), { from: 2, insert: "⁻¹" });
  assert.equal(typed("a ", "b", rules), null);
});

test("superscripts after a letter, a digit or a bracket, subscripts after a letter", () => {
  assert.equal(type("m^2 and 10^3 and (a+b)^2"), "m² and 10³ and (a+b)²");
  assert.equal(type("m s^-1, W m^-2, kg^-3"), "m s⁻¹, W m⁻², kg⁻³");
  assert.equal(type("CO_2, CH_4, O_3, N_2O"), "CO₂, CH₄, O₃, N₂O");
  // after nothing, a space, or a digit (a subscript): left as typed
  assert.equal(type("^2 and a ^2 and _2 and 5_2"), "^2 and a ^2 and _2 and 5_2");
});

test("the dashes of Markdown stay", () => {
  assert.equal(type("---"), "---");
  assert.equal(type("| a | b |"), "| a | b |");
  assert.equal(type("|---|:--:|"), "|---|:--:|");
  assert.equal(type("| --- | --- |"), "| --- | --- |");
  assert.equal(type("- item -- text"), "- item – text");
});

test("a sequence the rules lack stays as typed", () => {
  assert.equal(type("a - b > c = d"), "a - b > c = d");
  assert.equal(type('say "hi" and \'ho\''), 'say "hi" and \'ho\'');
});

test("the user's pairs: another replacement, off, a new sequence, the longest first", () => {
  const r = merged(DEFAULTS, [["->", "⟶"], ["~=", ""], ["(c)", "©"], ["x", "y"], ["", "z"], ["(r", "®"]]);
  assert.equal(type("a -> b", r), "a ⟶ b");
  assert.equal(type("a ~= b", r), "a ~= b");
  assert.equal(type("(c) x", r), "© x");
  assert.equal(type("(r)", r), "®)");
  // the context of a sequence stays with it when its replacement changes
  assert.equal(type("m^2 ^2", merged(DEFAULTS, [["^2", "<sup>2</sup>"]])), "m<sup>2</sup> ^2");
  // the last pair of a sequence counts
  assert.equal(type("->", merged(DEFAULTS, [["->", ""], ["->", "→→"]])), "→→");
  assert.ok(r.every((x, i) => i === 0 || r[i - 1].keys.length >= x.keys.length));
});

test("the longest sequence wins", () => {
  const r = merged(DEFAULTS, [["a->", "!"]]);
  assert.equal(type("a->", r), "!");
  assert.equal(type("b->", r), "b→");
});

test("the LaTeX name before the cursor", () => {
  assert.deepEqual(latex("the \\alpha"), { length: 6, insert: "α" });
  assert.deepEqual(latex("\\Delta"), { length: 6, insert: "Δ" });
  assert.deepEqual(latex("T in \\degC"), { length: 5, insert: "°C" });
  assert.deepEqual(latex("x\\^2"), { length: 3, insert: "²" });
  assert.deepEqual(latex("x\\^-"), { length: 3, insert: "⁻" });
  assert.deepEqual(latex("CO\\_2"), { length: 3, insert: "₂" });
  assert.deepEqual(latex("\\to"), { length: 3, insert: "→" });
});

test("no known name before the cursor", () => {
  assert.equal(latex("alpha"), null);
  assert.equal(latex("\\nonsense"), null);
  assert.equal(latex("\\alpha "), null);
  assert.equal(latex(""), null);
  assert.equal(latex("\\alpha", new Map()), null);
  assert.deepEqual(latex("\\ab", new Map([["\\ab", "X"]])), { length: 3, insert: "X" });
});

test("the table has the Greek letters, units and arrows", () => {
  const greek = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega";
  for (const name of greek.split(" ")) {
    assert.ok(LATEX.has(`\\${name}`), name);
    assert.ok(LATEX.has(`\\${name[0].toUpperCase()}${name.slice(1)}`), name);
  }
  for (const name of "varepsilon vartheta varphi deg degC pm times cdot approx leq geq infty partial nabla to rightarrow Rightarrow Leftrightarrow uparrow permil micro angstrom".split(" ")) {
    assert.ok(LATEX.has(`\\${name}`), name);
  }
  assert.equal(LATEX.get("\\^0"), "⁰");
  assert.equal(LATEX.get("\\^9"), "⁹");
  assert.equal(LATEX.get("\\_0"), "₀");
  assert.equal(LATEX.get("\\_9"), "₉");
  assert.equal(LATEX.get("\\mu"), "μ");
  assert.equal(LATEX.get("\\micro"), "µ");
});

test("every name in the table can be completed", () => {
  for (const [name, symbol] of LATEX) assert.deepEqual(latex(`x ${name}`), { length: name.length, insert: symbol }, name);
});

test("finding in the keys and the symbols, those starting with the query first", () => {
  const entries = [...DEFAULTS, ...[...LATEX].map(([keys, insert]) => ({ keys, insert }))];
  assert.equal(find(entries, "  ").length, entries.length);
  assert.equal(find(entries, "→")[0].keys, "->");
  const names = find(entries, "alpha").map((e) => e.keys);
  assert.equal(names[0], "\\alpha");
  assert.ok(names.includes("\\Alpha"));
  assert.equal(find(entries, "\\del")[0].keys, "\\delta");
  assert.ok(find(entries, "degc").some((e) => e.keys === "\\degC"));
  assert.deepEqual(find(entries, "zzzz"), []);
  // "pi" starts \pi before it is found inside \propto... or \Pi
  assert.equal(find(entries, "pi")[0].keys, "\\pi");
});
