// The text of a reference put into the review (src/lib/refs.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { citation, familyName, fullReference, initials, latex, pandoc, splitName } from "../src/lib/refs.ts";
import type { Item } from "../src/lib/types.ts";

const item = (over: Partial<Item> = {}): Item => ({
  id: "feldmann15",
  key: "feldmann15",
  kind: "article",
  title: "Collapse of the West Antarctic Ice Sheet after local destabilization of the Amundsen Basin",
  authors: ["Johannes Feldmann", "Anders Levermann"],
  year: 2015,
  container: "Proceedings of the National Academy of Sciences",
  server: null,
  doi: "10.1073/pnas.1512482112",
  url: null,
  keywords: [],
  files: [],
  bib: "refs.bib",
  subtitle: "",
  ...over,
});

test("the family name keeps its particle", () => {
  assert.equal(familyName("Johannes Feldmann"), "Feldmann");
  assert.equal(familyName("Michiel van den Broeke"), "van den Broeke");
  assert.equal(familyName("Bas de Boer"), "de Boer");
  assert.equal(familyName("Ludwig van Beethoven"), "van Beethoven");
  assert.equal(familyName("Madonna"), "Madonna");
  assert.equal(familyName("Martin Luther King Jr."), "King");
});

test("the given names", () => {
  assert.deepEqual(splitName("Michiel R. van den Broeke"), { given: "Michiel R.", family: "van den Broeke" });
  assert.deepEqual(splitName("Madonna"), { given: "", family: "Madonna" });
});

test("initials", () => {
  assert.equal(initials("Johannes"), "J.");
  assert.equal(initials("Jan-Hendrik"), "J.-H.");
  assert.equal(initials("Anne Marie"), "A. M.");
  assert.equal(initials("J.H."), "J. H.");
  assert.equal(initials("J.-H."), "J.-H.");
  assert.equal(initials("Émile"), "É.");
  assert.equal(initials(""), "");
});

test("a citation of two authors, of three, of one", () => {
  assert.equal(citation(item()), "Feldmann and Levermann (2015)");
  assert.equal(citation(item({ authors: ["Alexander Robinson", "Reinhard Calov", "Andrey Ganopolski"], year: 2012 })), "Robinson et al. (2012)");
  assert.equal(citation(item({ authors: ["Alexander Robinson"], year: 2012 })), "Robinson (2012)");
});

test("a citation without a year or authors", () => {
  assert.equal(citation(item({ year: null })), "Feldmann and Levermann (n.d.)");
  assert.equal(citation(item({ authors: [], title: "A report" })), "A report (2015)");
});

test("a full reference as Copernicus sets it", () => {
  assert.equal(
    fullReference(item()),
    "Feldmann, J. and Levermann, A.: Collapse of the West Antarctic Ice Sheet after local destabilization of the Amundsen Basin, Proceedings of the National Academy of Sciences, https://doi.org/10.1073/pnas.1512482112, 2015.",
  );
});

test("a full reference of three authors, a particle and a hyphenated name", () => {
  const authors = ["Jan-Hendrik Malles", "Michiel R. van den Broeke", "Anders Levermann"];
  assert.equal(
    fullReference(item({ authors, title: "A title.", container: "The Cryosphere", doi: null, year: 2020 })),
    "Malles, J.-H., van den Broeke, M. R., and Levermann, A.: A title, The Cryosphere, 2020.",
  );
});

test("a full reference leaves out the parts it lacks", () => {
  assert.equal(fullReference(item({ authors: [], container: null, doi: null, year: null, title: "Notes" })), "Notes, n.d.");
  assert.equal(fullReference(item({ container: null, server: "EGUsphere", doi: "https://doi.org/10.5194/x", year: 2024, title: "T" })), "Feldmann, J. and Levermann, A.: T, EGUsphere, https://doi.org/10.5194/x, 2024.");
});

test("the citation keys", () => {
  assert.equal(pandoc(item()), "[@feldmann15]");
  assert.equal(latex(item()), "\\cite{feldmann15}");
});
