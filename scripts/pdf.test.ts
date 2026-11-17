// What the views of a PDF share (src/lib/pdf.ts): the words around a click, the line clicked, the place of a word
// Show in PDF marks, and the section a page is under.

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { bestPlace, better, lineOfText, sectionAt, spoken, withNext, wordAround } from "../src/lib/pdf.ts";

test("the word at a place, and the six words before and after it, lower case, of two characters or more", () => {
  const text = "One two three four five six seven Antarctic ice-sheet melt rates are a key unknown in the projections";
  const at = text.indexOf("ice-sheet") + 3;
  assert.deepEqual(wordAround(text, at), {
    word: "ice-sheet",
    before: ["three", "four", "five", "six", "seven", "antarctic"],
    after: ["melt", "rates", "are", "key", "unknown", "in"],
  });
  // at the end of a word, as a caret just after it is
  assert.equal(wordAround("melt rates", 4).word, "melt");
  assert.equal(wordAround("", 0).word, "");
});

test("a short line takes the next one with it, enough words to find", () => {
  assert.equal(withNext("Results", "The ice sheet  retreats"), "Results\nThe ice sheet retreats");
  assert.equal(withNext("one two three four five six", "next"), "one two three four five six");
  assert.equal(withNext("short", ""), "short");
  const text = "Results\r\nThe ice sheet retreats\nmore";
  assert.equal(lineOfText(text, 0, 7), "Results\nThe ice sheet retreats");
});

test("text read aloud: hyphenated words joined, one space", () => {
  assert.equal(spoken("the ice-\nsheet and the melt-\r\nwater   run  off\n"), "the icesheet and the meltwater run off");
  // a hyphen before a capital stays (a name, a compound)
  assert.equal(spoken("North-\nAtlantic"), "North- Atlantic");
});

test("the word where its neighbours are most like the source's; the nearest among equals", () => {
  const text = "the ice melts fast here. Later the ice sheet melts slowly. Then ice again";
  const near = (from: number) => Math.abs(from - 60);
  const found = bestPlace(text, "ice", ["later", "the"], ["sheet", "melts"], near);
  assert.equal(found?.from, text.indexOf("ice sheet"));
  assert.equal(found?.score, 4);
  // no neighbours alike: the nearest
  assert.equal(bestPlace(text, "ice", [], [], near)?.from, text.lastIndexOf("ice"));
  // a whole word, any case; none where it is not
  assert.equal(bestPlace("Price ICE", "ice", [], [], () => 0)?.from, 6);
  assert.equal(bestPlace(text, "glacier", [], [], near), null);
  assert.equal(bestPlace(text, "", [], [], near), null);
  // the better of two: higher score, then nearer, then the first
  const a = { from: 0, to: 1, score: 1, distance: 5 };
  const b = { from: 2, to: 3, score: 1, distance: 2 };
  assert.equal(better(a, b), b);
  assert.equal(better(b, { ...a, distance: 2 }), b);
  assert.equal(better(null, a), a);
});

test("the section a page is under: the last item beginning on it or before", () => {
  const items = [
    { level: 1, title: "Intro", page: 1 },
    { level: 1, title: "Methods", page: 3 },
    { level: 2, title: "Unknown", page: null },
    { level: 1, title: "Results", page: 5 },
  ];
  assert.equal(sectionAt(items, 4), 1);
  assert.equal(sectionAt(items, 5), 3);
  assert.equal(sectionAt([], 2), -1);
  assert.equal(sectionAt([{ level: 1, title: "Late", page: 9 }], 2), -1);
});
