<script lang="ts">
  // What every tab of Papers Write shows around its content (ProjectFrame of papers-write/project): the project bar
  // and the sidebar, so a text, a PDF and a history of the same project look alike and nothing moves between them.
  //
  // The project bar (ProjectBar: a tile per project, the main document's words and open suggestions) is that of the
  // tab's project (`project_bar`); a click on a tile switches the window to its project, or for the current one shows
  // or hides the sidebar; a right-click its menu. It is asked again on `projects-changed`, when the window gets the
  // focus, when the tab shows another file and when the page says (`loadBarLater`). When Settings ▸ Window says so, a
  // thin bar in the project's colour is at the top. Project Info… (⇧⌘I, `project-info`) is a card over the window
  // (ProjectCard); so is the card of a new project, asked for when File ▸ Open Project… chose a folder without
  // write.toml (`asking-project`).
  //
  // The sidebar (Sidebar: the files of the project or of the file's folder, the outline, the search of the project) is
  // shown and hidden by a click on the project's tile and View ▸ Show Sidebar (⌥⌘S, `sidebar`); View ▸ Show Outline
  // (⌥⌘O, `outline`) shows it on the outline. Shown or not, its tab, its width and its open folders are the same in
  // all the project's tabs (ProjectSidebar).

  import { invoke } from "@tauri-apps/api/core";
  import { OPEN_FOLDER, keys } from "papers-write/editor";
  import { Menu, MenuItem } from "@tauri-apps/api/menu";
  import { onMount, type Snippet } from "svelte";
  import { ProjectBar, ProjectFrame, ProjectSidebar, cssOf, part, popupReleased } from "papers-write/project";
  import type { Heading } from "papers-write/editor";
  import SwitchProject from "$lib/SwitchProject.svelte";
  import ProjectCard from "$lib/ProjectCard.svelte";
  import Sidebar, { type Tab } from "$lib/Sidebar.svelte";
  import type { Bar } from "$lib/projects";
  import { listen } from "$lib/events";
  import { watchSettings } from "$lib/settings";
  import { over } from "$lib/overlay.svelte";

  /** Switch Project (⌃⌘P) shown */
  let switching = $state(false);

  let {
    path,
    headings = [],
    here = -1,
    outlined = false,
    offset = 0,
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
  /** the project bar's mode (Settings ▸ Window), and whether it is slid in (on hover) */
  let barMode = $state<"shown" | "hover" | "hidden">("shown");
  let barOpen = $state(false);
  let slideTimer: ReturnType<typeof setTimeout> | undefined;
  /** the strip and the bar that slides in from it */
  let hoverBar = $state<HTMLDivElement>();
  let slide = $state<HTMLDivElement>();
  // the part of the window the bar covers while slid in, which PDFKit's view keeps clear of (`over`)
  $effect(() => {
    over.left = barMode === "hover" && barOpen && hoverBar && slide ? hoverBar.getBoundingClientRect().left + slide.offsetWidth : 0;
    return () => (over.left = 0);
  });
  /** The pointer at the left edge: the bar slides in after a moment (a pointer only passing by does not open it). */
  function barNear() {
    clearTimeout(slideTimer);
    slideTimer = setTimeout(() => (barOpen = true), 250);
  }
  /** The pointer gone from the bar: it slides away, after a moment (a pointer straying out and back keeps it). */
  function barAway() {
    clearTimeout(slideTimer);
    slideTimer = setTimeout(() => (barOpen = false), 350);
  }
  const barColour = $derived(projects.current && colourBar ? cssOf(projects.current.colour) : null);
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

  // View ▸ Show Sidebar and Show Outline say Hide while they are shown: told as they change, and as the window comes
  // to the front
  $effect(() => {
    // each item says Hide while its own tab is the one shown
    const [shown, outline] = [side.shown && sideTab === "files", side.shown && sideTab === "outline"];
    const tell = () => invoke("sidebar_shown", { shown, outline }).catch(() => {});
    tell();
    window.addEventListener("focus", tell);
    return () => window.removeEventListener("focus", tell);
  });

  export function toggleSidebar() {
    if (!sideRoot) return onsay("Save the document first, to see its folder");
    side.set({ shown: !side.shown });
    if (!side.shown) onfocus?.();
  }

  /** The sidebar hidden, the keys back to the page's content. */
  function hideSidebar() {
    side.set({ shown: false });
    onfocus?.();
  }

  /** View ▸ Show Files: the sidebar on its files, or hidden when it shows them (each tab of the sidebar has its key,
   *  which hides it when that tab is the one shown). */
  export function toggleFiles() {
    if (!sideRoot) return onsay("Save the document first, to see its folder");
    if (side.shown && sideTab === "files") return hideSidebar();
    side.set({ shown: true, tab: "files" });
  }

  /** View ▸ Show Outline: the sidebar on its outline, or hidden when it shows it. */
  export function toggleOutline() {
    if (side.shown && sideTab === "outline") return toggleSidebar();
    side.set({ shown: true, tab: "outline" });
  }

  /** Edit ▸ Find ▸ Find in Project…: the sidebar on its search. */
  export function findInProject() {
    if (!sideRoot) return onsay("Save the document first, to search its folder");
    // asked again with the search's field in hand: the sidebar hidden, as the other tabs' keys do
    if (side.shown && sideTab === "search" && document.activeElement?.closest(".side")) return hideSidebar();
    side.set({ shown: true });
    queueMicrotask(() => list?.find());
  }

  /** A click on a tile: the current project's shows or hides the sidebar, another's switches the window to it (or
   *  brings forward the window that shows it). */
  function switchProject(root: string) {
    barOpen = false;
    if (root === projects.current?.root) return toggleSidebar();
    invoke("switch_project", { root }).catch((e) => onproblem(String(e)));
  }

  /** A right-click on a tile. */
  async function tileMenu(root: string) {
    const items = await Promise.all([
      MenuItem.new({ text: "Open in New Window", action: () => void invoke("open_project_in_new_window", { root }).catch((e) => onproblem(String(e))) }),
      MenuItem.new({ text: OPEN_FOLDER, action: () => void invoke("reveal", { path: root }).catch((e) => onproblem(String(e))) }),
      MenuItem.new({ text: "Project Info…", action: () => (carding = root) }),
      MenuItem.new({ text: "Close All Tabs", action: () => void invoke("close_project_tabs", { root }) }),
      MenuItem.new({ text: "Remove from Bar", action: () => void invoke("remove_from_bar", { root }) }),
    ]);
    await popupReleased(await Menu.new({ items }));
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
    const stops = [side.start(), watchSettings((s) => ([colourBar, barMode] = [s.project_colour_bar, s.project_bar ?? "shown"]))];
    askProject();
    window.addEventListener("focus", loadBar);
    // ⌃+ where + is a key of its own (Italian, German): the menu's Zoom In is ⌃= (⌃⇧0 there), so said here; where
    // the menu took the key the page does not see it
    const plus = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key === "+") {
        e.preventDefault();
        void invoke("zoom_key", { step: 1 }).catch(() => {});
      }
    };
    window.addEventListener("keydown", plus);
    const unlisten = [
      listen("projects-changed", loadBar),
      listen("asking-project", askProject),
      listen("project-info", projectInfo),
      listen("switch-project", () => (switching = true)),
      listen("outline", toggleOutline),
      listen("sidebar", toggleFiles),
    ];
    return () => {
      stops.forEach((stop) => stop());
      clearTimeout(barTimer);
      clearTimeout(slideTimer);
      window.removeEventListener("focus", loadBar);
      window.removeEventListener("keydown", plus);
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });
</script>

{#snippet projectBar()}
  <ProjectBar
    items={projects.tiles}
    onswitch={switchProject}
    onmenu={tileMenu}
    onmove={(root, to) => void invoke("move_in_bar", { root, to }).then(loadBar).catch((e) => onproblem(String(e)))}
    onadd={() => invoke("choose_project")} addTitle={keys("Open Project… (⇧⌘O)")} />
{/snippet}
{#snippet pin()}
  <!-- the bar pinned (always shown: filled) or sliding in on hover (outlined); View ▸ Pin the Project Bar -->
  <button class="pin" class:on={barMode === "shown"} onclick={() => void invoke("pin_project_bar")} title={barMode === "shown" ? "Unpin: the bar slides in when the pointer is at the left edge" : "Pin the project bar"} aria-label="Pin the project bar">
    <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill={barMode === "shown" ? "currentColor" : "none"}>
      <path d="M6 1.8h4l-.6 4.2 2.4 2.2v1.2H4.2V8.2L6.6 6z" /><path d="M8 9.4v4.8" fill="none" />
    </svg>
  </button>
{/snippet}
{#snippet tiles()}
  {#if projects.current && projects.tiles.length}
    {#if barMode === "shown"}
      <div class="pinned">{@render projectBar()}{@render pin()}</div>
    {:else if barMode === "hover"}
      <!-- a strip at the left edge in the project's colour; the bar slides in over the page while the pointer is on
           it (as the Ubuntu dock), and away when it leaves or a project is chosen. PDFKit's view, above the page,
           makes way for it (`over`) -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="hover-bar" class:open={barOpen} bind:this={hoverBar} style:--strip={cssOf(projects.current.colour) ?? "var(--line)"} onmouseenter={barNear} onmouseleave={barAway}>
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="strip" onclick={() => (barOpen = true)} title="Projects"></div>
        <div class="slide" bind:this={slide}><div class="pinned">{@render projectBar()}{@render pin()}</div></div>
      </div>
    {/if}
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
    onclose={hideSidebar}
  />
{/snippet}
{#snippet folder()}{#if sideRoot}{@render files(sideRoot)}{/if}{/snippet}

<div class="write">
  <ProjectFrame {side} colour={barColour} {offset} {top} bar={tiles} sidebar={sideRoot ? folder : undefined}>
    {@render children()}
  </ProjectFrame>
</div>
{#if switching}<SwitchProject onclose={() => ((switching = false), onfocus?.())} />{/if}
{#if asking}
  {#key asking}<ProjectCard root={asking} onclose={closeCard} />{/key}
{:else if carding}
  {#key carding}<ProjectCard root={carding} onclose={closeCard} />{/key}
{/if}

<style>
  /* the bar, its pin at its foot */
  .pinned {
    position: relative;
    height: 100%;
  }
  .pin {
    position: absolute;
    left: 50%;
    bottom: 8px;
    transform: translateX(-50%);
    padding: 3px;
    line-height: 0;
    color: var(--muted);
    background: transparent;
    border: 0;
    border-radius: 5px;
    opacity: 0.55;
    cursor: pointer;
  }
  .pin:hover,
  .pin.on {
    opacity: 1;
  }
  .pin:hover {
    background: var(--chip, rgba(128, 128, 128, 0.15));
  }
  .hover-bar {
    position: relative;
    flex: none;
    width: 5px;
    height: 100%;
    z-index: 40;
  }
  .strip {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--strip) 55%, transparent);
    cursor: pointer;
  }
  .slide {
    position: absolute;
    top: 0;
    left: 0;
    height: 100%;
    display: flex;
    transform: translateX(-100%);
    transition: transform 0.16s ease-out;
    box-shadow: 4px 0 18px rgba(0, 0, 0, 0.18);
    background: var(--bg);
  }
  .hover-bar:not(.open) .slide {
    box-shadow: none;
  }
  .hover-bar.open .slide {
    transform: none;
  }
  .write {
    display: contents;
    /* the tiles' detail: words and open suggestions, on two lines */
    --tile-detail-size: 10.5px;
  }
</style>
