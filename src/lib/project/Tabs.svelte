<script lang="ts">
  // The tabs of this page's set in its window (papers-tabs, crates/tabs), drawn by TabStrip: asked on `tabs-changed`
  // and when the window gets the focus; a tab chosen is `select_tab`, one closed `close_tab` (the app may ask first).
  // An app's control on a tab (`extra`) as TabStrip's; `least` the fewest tabs that show a strip.

  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount, type Snippet } from "svelte";
  import TabStrip, { type TabItem } from "./TabStrip.svelte";

  type Tab = { label: string; title: string; path: string | null };

  let { least = 2, extra }: { least?: number; extra?: Snippet<[TabItem]> } = $props();

  const me = getCurrentWebview().label;
  let tabs = $state.raw<TabItem[]>([]);

  async function refresh() {
    try {
      const got = await invoke<Tab[]>("tab_strip");
      tabs = got.map((t) => ({ id: t.label, title: t.title, tooltip: t.path }));
    } catch {
      tabs = [];
    }
  }

  onMount(() => {
    refresh();
    window.addEventListener("focus", refresh);
    // the events sent to this page alone (not `listen` of @tauri-apps/api/event, which hears those of every page)
    const unlisten = getCurrentWebview().listen("tabs-changed", refresh);
    return () => {
      window.removeEventListener("focus", refresh);
      unlisten.then((f) => f());
    };
  });
</script>

<TabStrip
  {tabs}
  front={me}
  {least}
  {extra}
  onselect={(label) => invoke("select_tab", { label })}
  onclose={(label) => invoke("close_tab", { label })}
/>
