// The public API of the project bar: what an app imports as `papers-write/project`.

export { default as ProjectBar, type Level, type Tile } from "./ProjectBar.svelte";
export { SHORT, initials, label } from "./bar";
export { COLOURS, cssOf } from "./colour";
export type { Colour } from "./types";
