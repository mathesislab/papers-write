// The change from one text to another, as one replacement: what an editor applies when the file is read again.

/** The part of `before` that differs from `after`: from its start and end, the text the two share left out. */
export function difference(before: string, after: string): { from: number; to: number; insert: string } {
  let start = 0;
  const most = Math.min(before.length, after.length);
  while (start < most && before.charCodeAt(start) === after.charCodeAt(start)) start++;
  let end = 0;
  while (end < most - start && before.charCodeAt(before.length - 1 - end) === after.charCodeAt(after.length - 1 - end)) end++;
  return { from: start, to: before.length - end, insert: after.slice(start, after.length - end) };
}
