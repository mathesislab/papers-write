// The right-click menu of a PDF's text, the same in both views (PdfView, PdfjsView): Read Aloud (the selection, else
// the page from the place clicked), Go to Source (as a ⌘-click), the page's own item, and Copy.

import { LogicalPosition } from "@tauri-apps/api/dpi";
import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { popupReleased } from "$lib/contextMenu";
import { keys } from "papers-write/editor";
import type { PdfViewProps, Spot } from "$lib/pdf";
import { readAloudItems } from "$lib/readAloud";

/** The menu at `x`, `y` (CSS pixels of the page) for a click on `spot` (none: off the text), with the text selected
 *  (`selected`) and the page's text from the place clicked (`fromHere`). */
export async function pdfMenu(
  spot: Spot | null,
  selected: string,
  fromHere: string,
  { onsource, menuItem }: Pick<PdfViewProps, "onsource" | "menuItem">,
  x: number,
  y: number,
) {
  const reading = await readAloudItems(selected || fromHere, selected ? "Read Aloud" : "Read Aloud from Here");
  const items = await Promise.all([
    ...reading,
    ...(spot && onsource ? [MenuItem.new({ text: keys("Go to Source (⌘-click)"), action: () => onsource(spot) })] : []),
    ...(menuItem ? [MenuItem.new(menuItem)] : []),
    PredefinedMenuItem.new({ item: "Separator" }),
    PredefinedMenuItem.new({ item: "Copy" }),
  ]);
  // at the pointer (without a place, it opens elsewhere in the window on Linux)
  await popupReleased(await Menu.new({ items }), new LogicalPosition(x, y));
}
