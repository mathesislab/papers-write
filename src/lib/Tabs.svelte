<script lang="ts">
  // The tabs of this page's window (tabs.rs), drawn by the shared TabStrip: asked on `tabs-changed` and when the
  // window gets the focus; a tab chosen is `select_tab`, one closed `close_tab` (untitled text is asked about first).

  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount } from "svelte";
  import { TabStrip, type TabItem } from "papers-write/project";
  import { listen } from "$lib/events";

  type Tab = { label: string; title: string; path: string | null };

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
    const unlisten = listen("tabs-changed", refresh);
    return () => {
      window.removeEventListener("focus", refresh);
      unlisten.then((f) => f());
    };
  });
</script>

<TabStrip
  {tabs}
  front={me}
  onselect={(label) => invoke("select_tab", { label })}
  onclose={(label) => invoke("close_tab", { label })}
/>
