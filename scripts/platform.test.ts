// The keys as the platform writes them (src/lib/editor/platform.ts).

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { keys, trashKey } from "../src/lib/editor/platform.ts";

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

test("the Trash's keys as the file manager has them: ⌘⌫ at once on macOS; Delete asked first, ⇧Delete at once elsewhere", () => {
  const press = (key: string, mods: { metaKey?: boolean; shiftKey?: boolean; ctrlKey?: boolean; altKey?: boolean } = {}) => ({
    key,
    metaKey: false,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
    ...mods,
  });
  assert.equal(trashKey(press("Backspace", { metaKey: true }), true), "now");
  assert.equal(trashKey(press("Backspace"), true), "ask");
  assert.equal(trashKey(press("Delete"), true), "ask");
  assert.equal(trashKey(press("Backspace", { metaKey: true, shiftKey: true }), true), null);
  assert.equal(trashKey(press("Delete"), false), "ask");
  assert.equal(trashKey(press("Delete", { shiftKey: true }), false), "now");
  assert.equal(trashKey(press("Delete", { ctrlKey: true }), false), null);
  assert.equal(trashKey(press("Backspace", { metaKey: true }), false), null);
  assert.equal(trashKey(press("a", { metaKey: true }), true), null);
});
