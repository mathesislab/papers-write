// Show in PDF by words: the source line as the PDF reads it, and the phrases tried (src/lib/pdfSearch.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { phrasesAt, plainOf } from "../src/lib/pdfSearch.ts";

test("markup that does not show in the PDF is taken out", () => {
  assert.equal(plainOf("Sea level rises, as **Kopp et al.** [@kopp2014] showed in Fig. @fig:x."), "Sea level rises, as Kopp et al. showed in Fig.");
  assert.equal(plainOf("The \\emph{semi-empirical} model $a=b$ of \\cite{x} sea level."), "The semi-empirical model of sea level.");
  assert.equal(plainOf("# A [linked](http://x) heading"), "A linked heading");
  assert.equal(plainOf("keep {++new++}{>>@MP<<} and {--old--} and {~~a~>b~~}"), "keep new and and b");
});

test("phrases around the cursor, the longest first, none shorter than three words", () => {
  const line = "the grounding line of the ice sheet retreats rapidly in the Amundsen sea";
  const at = line.indexOf("retreats");
  assert.deepEqual(phrasesAt(line, at), ["the ice sheet retreats rapidly in", "ice sheet retreats rapidly", "the ice sheet", "retreats rapidly in"]);
  assert.deepEqual(phrasesAt("two words", 3), []);
});
