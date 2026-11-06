<script lang="ts">
  // What every tab of Papers Write shows around its content (ProjectFrame of papers-write/project): the project bar
  // and the sidebar, so a text, a PDF and a history of the same project look alike and nothing moves between them.
  //
  // The project bar (ProjectBar: a tile per project, the main document's words and open suggestions) is that of the
  // tab's project (`project_bar`); a click on a tile switches the window to its project, or for the current one shows
  // or hides the sidebar; a right-click its menu. It is asked again on `projects-changed`, when the window gets the
  // focus, when the tab shows another file and when the page says (`loadBarLater`). When Settings ▸ Window says so, a
  // thin bar in the project's colour is at the top. Project Info… (⌘I, `project-info`) is a card over the window
  // (ProjectCard); so is the card of a new project, asked for when File ▸ Open Project… chose a folder without
  // write.toml (`asking-project`).
  //
  // The sidebar (Sidebar: the files of the project or of the file's folder, the outline, the search of the project) is
  // shown and hidden by a click on the project's tile and View ▸ Show Sidebar (⌥⌘S, `sidebar`); View ▸ Show Outline
  // (⌥⌘O, `outline`) shows it on the outline. Shown or not, its tab, its width and its open folders are the same in
  // all the project's tabs (ProjectSidebar).

  import { invoke } from "@tauri-apps/api/core";
  import { Menu, MenuItem } from "@tauri-apps/api/menu";
  import { onMount, type Snippet } from "svelte";
  import { ProjectBar, ProjectFrame, ProjectSidebar, cssOf, part } from "papers-write/project";
  import type { Heading } from "papers-write/editor";
  import ProjectCard from "$lib/ProjectCard.svelte";
  import Sidebar, { type Tab } from "$lib/Sidebar.svelte";
  import type { Bar } from "$lib/projects";
  import { listen } from "$lib/events";
  import { watchSettings } from "$lib/settings";

  let {
    path,
    headings = [],
    here = -1,
    outlined = false,
    offset = 0,
    covering = $bindable(false),
    onheading,
    onsay,
    onproblem = onsay,
    onfocus,
    top,
    children,
  }: {
    /** the tab's file (none: untitled, or not a file); undefined until known */
    path: string | null | undefined;
    /** the outline of the tab's text, and the heading the cursor is under */
    headings?: readonly Heading[];
    here?: number;
    /** the tab's file has headings to outline */
    outlined?: boolean;
    /** the height of what the page draws above the tabs (ProjectFrame) */
    offset?: number;
    /** a card is over the page (what is drawn above the page, a PDF, makes way) */
    covering?: boolean;
    onheading?: (heading: Heading) => void;
    /** what was done, or went wrong, for the page to say */
    onsay: (what: string) => void;
    /** what went wrong and stays said till dismissed */
    onproblem?: (what: string) => void;
    /** the keys given back to the page's content (a card closed, the sidebar hidden) */
    onfocus?: () => void;
    /** at the right of the strip of tabs */
    top?: Snippet;
    children: Snippet;
  } = $props();

  const side = new ProjectSidebar();
  const sideTab = $derived((side.tab ?? "files") as Tab);
  let list = $state<Sidebar>();

  /** the project bar: the tab's project (none: no bar) and its tiles */
  let projects = $state<Bar>({ current: null, tiles: [] });
  let barTimer: ReturnType<typeof setTimeout> | undefined;
  /** the folder of the project whose info card is shown */
  let carding = $state<string | null>(null);
  /** the folder opened as a project that has no write.toml yet, whose card asks what to write in one */
  let asking = $state<string | null>(null);
  /** the thin bar in the project's colour at the top (Settings ▸ Window) */
  let colourBar = $state(false);
  /** the folder the sidebar shows: the project's, else the file's */
  const sideRoot = $derived(projects.current?.root ?? (path ? path.slice(0, path.lastIndexOf("/")) : null));
  const barColour = $derived(projects.current && colourBar ? cssOf(projects.current.colour) : null);
  $effect(() => {
    covering = !!(asking || carding);
  });

  /** the bar read the first time */
  const barIn = part();

  /** The bar read the first time, for the tab to be shown (`ready`). */
  export function loaded(): Promise<void> {
    return barIn.promise;
  }

  /** The bar asked again. */
  export async function loadBar() {
    clearTimeout(barTimer);
    try {
      projects = await invoke<Bar>("project_bar");
    } catch (e) {
      onproblem(String(e));
    } finally {
      barIn.done();
    }
  }

  /** The bar asked again a moment later (the words and suggestions of a file being written). */
  export function loadBarLater(ms: number) {
    clearTimeout(barTimer);
    barTimer = setTimeout(loadBar, ms);
  }

  export function toggleSidebar() {
    if (!sideRoot) return onsay("Save the document first, to see its folder");
    side.set({ shown: !side.shown });
    if (!side.shown) onfocus?.();
  }

  /** View ▸ Show Outline: the sidebar on its outline, or hidden when it shows it. */
  export function toggleOutline() {
    if (side.shown && sideTab === "outline") return toggleSidebar();
    side.set({ shown: true, tab: "outline" });
  }

  /** Edit ▸ Find ▸ Find in Project…: the sidebar on its search. */
  export function findInProject() {
    if (!sideRoot) return onsay("Save the document first, to search its folder");
    side.set({ shown: true });
    queueMicrotask(() => list?.find());
  }

  /** A click on a tile: the current project's shows or hides the sidebar, another's switches the window to it (or
   *  brings forward the window that shows it). */
  function switchProject(root: string) {
    if (root === projects.current?.root) return toggleSidebar();
    invoke("switch_project", { root }).catch((e) => onproblem(String(e)));
  }

  /** A right-click on a tile. */
  async function tileMenu(root: string) {
    const items = await Promise.all([
      MenuItem.new({ text: "Open in New Window", action: () => void invoke("open_project_in_new_window", { root }).catch((e) => onproblem(String(e))) }),
      MenuItem.new({ text: "Open Project Folder in Finder", action: () => void invoke("reveal", { path: root }).catch((e) => onproblem(String(e))) }),
      MenuItem.new({ text: "Project Info…", action: () => (carding = root) }),
      MenuItem.new({ text: "Remove from Bar", action: () => void invoke("remove_from_bar", { root }) }),
    ]);
    await (await Menu.new({ items })).popup();
  }

  function projectInfo() {
    if (projects.current) carding = projects.current.root;
    else onsay("Not in a project: File ▸ Open Project… makes a folder one");
  }

  async function askProject() {
    asking = await invoke<string | null>("asking_project");
  }

  function closeCard() {
    carding = null;
    asking = null;
    onfocus?.();
  }

  // the tab shows another file (an untitled one saved): its project's bar
  $effect(() => {
    if (path !== undefined) loadBar();
  });

  onMount(() => {
    const stops = [side.start(), watchSettings((s) => (colourBar = s.project_colour_bar))];
    askProject();
    window.addEventListener("focus", loadBar);
    const unlisten = [
      listen("projects-changed", loadBar),
      listen("asking-project", askProject),
      listen("project-info", projectInfo),
      listen("outline", toggleOutline),
      listen("sidebar", toggleSidebar),
    ];
    return () => {
      stops.forEach((stop) => stop());
      clearTimeout(barTimer);
      window.removeEventListener("focus", loadBar);
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });
</script>

{#snippet tiles()}
  {#if projects.current && projects.tiles.length}
    <ProjectBar items={projects.tiles} onswitch={switchProject} onmenu={tileMenu} onadd={() => invoke("choose_project")} addTitle="Open Project… (⇧⌘O)" />
  {/if}
{/snippet}
{#snippet files(root: string)}
  <Sidebar
    bind:this={list}
    bind:tab={() => sideTab, (tab) => side.set({ tab })}
    bind:folders={() => side.folders, (folders) => side.setFolders(folders)}
    {root}
    front={path ?? null}
    {headings}
    {here}
    {outlined}
    onheading={(h) => onheading?.(h)}
    {onsay}
  />
{/snippet}
{#snippet folder()}{#if sideRoot}{@render files(sideRoot)}{/if}{/snippet}

<div class="write">
  <ProjectFrame {side} colour={barColour} {offset} {top} bar={tiles} sidebar={sideRoot ? folder : undefined}>
    {@render children()}
  </ProjectFrame>
</div>
{#if asking}
  {#key asking}<ProjectCard root={asking} onclose={closeCard} />{/key}
{:else if carding}
  {#key carding}<ProjectCard root={carding} onclose={closeCard} />{/key}
{/if}

<style>
  .write {
    display: contents;
    /* the tiles' detail: words and open suggestions, on two lines */
    --tile-detail-size: 10.5px;
  }
</style>
