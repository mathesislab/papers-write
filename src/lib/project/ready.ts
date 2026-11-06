// A tab's page says it is drawn (papers-tabs `page_ready`): its tab is shown only then, the page shown before staying
// in view till then, so a tab never shows an empty page (papers-tabs shows it after a while all the same).

import { invoke } from "@tauri-apps/api/core";
import { tick } from "svelte";

/** The page drawn once `parts` are in (or failed: what went wrong is drawn too). */
export async function ready(...parts: Promise<unknown>[]) {
  await Promise.allSettled(parts);
  await tick();
  await new Promise((r) => setTimeout(r, 0));
  invoke("page_ready").catch(() => {});
}

/** A promise kept by calling `done` (once a part of the page is in). */
export function part(): { done: () => void; promise: Promise<void> } {
  let done = () => {};
  const promise = new Promise<void>((r) => (done = r));
  return { done, promise };
}
