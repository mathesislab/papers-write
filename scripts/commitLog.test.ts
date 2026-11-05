// The message of the next commit from what the window did (src/lib/commitLog.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { decided, message } from "../src/lib/commitLog.ts";

test("nothing logged: no message", () => {
  assert.equal(message([]), "");
});

test("decisions are counted by whose, the parts joined in the order first done", () => {
  const log = [
    ...decided("accept", [{ author: "Jane Doe" }]),
    { did: "review of jane/edits as suggestions" },
    ...decided("accept", [{ author: "Jane Doe" }, { author: "Bob" }, { author: "Jane Doe" }]),
    ...decided("reject", [{ author: "Bob" }]),
  ];
  assert.equal(message(log), "Accept 3 suggestions of Jane Doe and 1 of Bob; review of jane/edits as suggestions; reject 1 suggestion of Bob");
});

test("the same thing done twice is said once; three people are listed", () => {
  const log = [
    { did: "clean up the review" },
    { did: "clean up the review" },
    ...decided("reject", [{ author: "A" }, { author: "B" }, { author: "C" }, { author: "C" }]),
  ];
  assert.equal(message(log), "Clean up the review; reject 1 suggestion of A, 1 of B and 2 of C");
});
