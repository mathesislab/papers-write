<script lang="ts">
  // The tabs of this page's set in its window (papers-tabs, crates/tabs), drawn by TabStrip: asked on `tabs-changed`
  // and when the window gets the focus; a tab chosen is `select_tab`, one closed `close_tab` (the app may ask first),
  // one dragged to another place `move_tab`.
  // An app's control on a tab (`extra`) as TabStrip's; `least` the fewest tabs that show a strip.

  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount, type Snippet } from "svelte";
  import TabStrip, { type TabItem } from "./TabStrip.svelte";

  type Tab = { label: string; title: string; path: string | null; edited: boolean; front: boolean };

  let { least = 2, extra }: { least?: number; extra?: Snippet<[TabItem]> } = $props();

  const me = getCurrentWebview().label;
  let tabs = $state.raw<TabItem[]>([]);
  /** the tab in front: this page's, or one chosen whose page is not drawn yet (this one shown meanwhile, dimmed: the
   *  class `switching` on the document, theme.css) */
  let front = $state(me);

  async function refresh() {
    try {
      const got = await invoke<Tab[]>("tab_strip");
      tabs = got.map((t) => ({ id: t.label, title: t.title, tooltip: t.path, edited: t.edited }));
      front = got.find((t) => t.front)?.label ?? me;
    } catch {
      tabs = [];
      front = me;
    }
    document.documentElement.classList.toggle("switching", front !== me);
  }

  /** A tab dragged to another place: shown there at once, then the app told (which tells every page of the window). */
  function moved(label: string, to: number) {
    const from = tabs.findIndex((t) => t.id === label);
    if (from < 0) return;
    const now = [...tabs];
    now.splice(to, 0, ...now.splice(from, 1));
    tabs = now;
    invoke("move_tab", { label, to }).catch(refresh);
  }

  onMount(() => {
    refresh();
    window.addEventListener("focus", refresh);
    // the events sent to this page alone (not `listen` of @tauri-apps/api/event, which hears those of every page)
    const unlisten = [
      getCurrentWebview().listen("tabs-changed", refresh),
      // shown: in front, not dimmed (before the strip is asked again)
      getCurrentWebview().listen<boolean>("tab-shown", (e) => {
        if (e.payload) document.documentElement.classList.remove("switching");
      }),
    ];
    return () => {
      window.removeEventListener("focus", refresh);
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });
</script>

<TabStrip
  {tabs}
  {front}
  {least}
  {extra}
  onselect={(label) => invoke("select_tab", { label })}
  onclose={(label) => invoke("close_tab", { label })}
  onmove={moved}
/>
