// The colour of a project: a bar at the top of its windows, to tell them apart among others (Mission Control,
// small windows). The colours of Papers Desktop's window (its src/lib/windowColour.ts), pastel: a bar to tell the
// window apart, not to catch the eye.

import type { Colour } from "./types";

/** The colours, in the order of the menus: their names and how they are drawn; none: no bar. */
export const COLOURS: { colour: Colour; name: string; css: string | null }[] = [
  { colour: "blue", name: "Blue", css: "#a8c8f0" },
  { colour: "teal", name: "Teal", css: "#9fdcd3" },
  { colour: "green", name: "Green", css: "#b5e0b0" },
  { colour: "yellow", name: "Yellow", css: "#f3e19a" },
  { colour: "orange", name: "Orange", css: "#f7c79a" },
  { colour: "red", name: "Red", css: "#f2aaa6" },
  { colour: "pink", name: "Pink", css: "#f2b5d4" },
  { colour: "purple", name: "Purple", css: "#cdb9f0" },
  { colour: "graphite", name: "Graphite", css: "#c3c6cc" },
  { colour: "none", name: "None", css: null },
];

/** How a colour is drawn; none: no bar. */
export const cssOf = (colour: Colour) => COLOURS.find((c) => c.colour === colour)?.css ?? null;
