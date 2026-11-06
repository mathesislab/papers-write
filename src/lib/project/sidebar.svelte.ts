// A project's sidebar as its pages show it (papers-tabs keeps it, crates/tabs): shown or not, its width, the tab it
// shows (the app's to name). The same in all the project's pages: one changed here is kept and the others told
// (`sidebar-changed`); the page's project may change (a loose file saved into one), so it is asked again with the tabs.
// So are the folders open in its file tree (`folders-changed`), which are not kept: a project opens with them closed.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";

export type SidebarState = { shown: boolean; width: number; tab: string | null };

export class ProjectSidebar {
  shown = $state(true);
  width = $state(240);
  tab = $state<string | null>(null);
  /** the folders open in the file tree */
  folders = $state<string[]>([]);

  #take(s: SidebarState) {
    this.shown = s.shown;
    this.width = s.width;
    this.tab = s.tab;
  }

  /** What is kept read, and kept up to date; the function that stops it. */
  start(): () => void {
    const load = () => {
      invoke<SidebarState>("sidebar").then((s) => this.#take(s), () => {});
      invoke<string[]>("folders").then((f) => (this.folders = f), () => {});
    };
    load();
    const page = getCurrentWebview();
    const stops = [
      page.listen<SidebarState>("sidebar-changed", (e) => this.#take(e.payload)),
      page.listen<string[]>("folders-changed", (e) => (this.folders = e.payload)),
      page.listen("tabs-changed", load),
    ];
    return () => stops.forEach((s) => s.then((f) => f()));
  }

  /** Changed, and kept unless `keep` is false (a width being dragged: kept when it is let go). */
  set(change: Partial<SidebarState>, keep = true) {
    if (change.shown !== undefined) this.shown = change.shown;
    if (change.width !== undefined) this.width = change.width;
    if (change.tab !== undefined) this.tab = change.tab;
    if (keep) this.keep();
  }

  /** The folders open in the file tree changed. */
  setFolders(folders: string[]) {
    this.folders = folders;
    invoke("set_folders", { folders }).catch(() => {});
  }

  keep() {
    invoke("set_sidebar", { sidebar: { shown: this.shown, width: this.width, tab: this.tab } }).catch(() => {});
  }
}
