// The keys as the platform writes them (src/lib/editor/platform.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { keys } from "../src/lib/editor/platform.ts";

test("on macOS the symbols stay; elsewhere the keys are written out, Ctrl first", () => {
  assert.equal(keys("Suggest (⇧⌘E)", true), "Suggest (⇧⌘E)");
  assert.equal(keys("Suggest (⇧⌘E)", false), "Suggest (Ctrl+Shift+E)");
  assert.equal(keys("Build and Show PDF (⌥⇧⌘B)", false), "Build and Show PDF (Ctrl+Alt+Shift+B)");
  assert.equal(keys("⌘↵ posts", false), "Ctrl+Enter posts");
  assert.equal(keys("↵ cite · ⌘. [@key] · ⇧↵ reference · ⌘L \\cite", false), "Enter cite · Ctrl+. [@key] · Shift+Enter reference · Ctrl+L \\cite");
  assert.equal(keys("Zoom Out (⌘-) and (⌘−) and ⌘= ⌘0 ⌘[ ⌘1", false), "Zoom Out (Ctrl+-) and (Ctrl+-) and Ctrl+= Ctrl+0 Ctrl+[ Ctrl+1");
  assert.equal(keys("⇧Tab takes it out", false), "Shift+Tab takes it out");
  assert.equal(keys("⌘/ or Edit ▸ Symbols…", false), "Ctrl+/ or Edit ▸ Symbols…");
});
