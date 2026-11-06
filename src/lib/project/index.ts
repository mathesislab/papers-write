// The public API of the projects: what an app imports as `papers-write/project` (the project bar, the tree of files).

export { default as ProjectBar, type Level, type Tile } from "./ProjectBar.svelte";
export { default as FileTree, type Badge, type Extra } from "./FileTree.svelte";
export { SHORT, initials, label } from "./bar";
export { COLOURS, cssOf } from "./colour";
export type { Colour, Kind, Node } from "./types";
