// The public API of the projects: what an app imports as `papers-write/project` (the project bar, the tree of files,
// the strip of tabs).

export { default as ProjectBar, type Level, type Tile } from "./ProjectBar.svelte";
export { default as FileTree, type Badge, type Extra } from "./FileTree.svelte";
export { default as TabStrip, type TabItem } from "./TabStrip.svelte";
export { default as Tabs } from "./Tabs.svelte";
export { SHORT, initials, label } from "./bar";
export { COLOURS, cssOf } from "./colour";
export type { Colour, Kind, Node } from "./types";
export { ProjectSidebar, type SidebarState } from "./sidebar.svelte";
