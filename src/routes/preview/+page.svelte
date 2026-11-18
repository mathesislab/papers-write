<script lang="ts">
  // The preview of a text in a tab of its own (View ▸ Panes ▸ In Tabs of Their Own: a small screen), beside the text's
  // tab: what pandoc made of the text and the block of its cursor, sent by the text's page as they change (panetabs.rs:
  // `relay`). A click scrolls the text to the block; Go to Source (⌘-click, the menu) brings the text's tab forward
  // at the place; Show PDF its PDF's tab.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { ready } from "papers-write/project";
  import { listen } from "$lib/events";
  import Frame from "$lib/Frame.svelte";
  import Preview from "$lib/Preview.svelte";
  import "$lib/theme.css";

  /** the page of the text this previews */
  const of = new URLSearchParams(location.search).get("of") ?? "";

  let path = $state<string | null | undefined>(undefined);
  let html = $state("");
  let error = $state("");
  let block = $state(0);
  let preview = $state<Preview>();
  let frame = $state<Frame>();

  /** Something told to the text's page; false when it is gone (its tab closed). */
  const tell = (what: Record<string, unknown>) => invoke<boolean>("relay", { to: of, event: "preview-tab", payload: what }).catch(() => false);

  onMount(() => {
    const got = new Promise<void>((done) => {
      const unlisten = [
        listen<{ html: string; error: string; block: number }>("preview-html", (e) => {
          ({ html, error, block } = e.payload);
          done();
        }),
        listen<number>("preview-block", (e) => (block = e.payload)),
        listen("preview-flash", () => queueMicrotask(() => preview?.flash())),
        // in front: the preview's side is the one worked in (kept by the text's page)
        listen<boolean>("tab-shown", (e) => e.payload && void tell({ kind: "shown" })),
      ];
      onDestroy = () => unlisten.forEach((u) => u.then((f) => f()));
    });
    const asked = (async () => {
      path = await invoke<string | null>("page_path", { page: of }).catch(() => null);
      if (!(await tell({ kind: "want" }))) error = "The text of this preview is closed";
    })();
    // drawn once the text's page has sent what pandoc made (or after a while: ready's own wait)
    ready(asked, Promise.race([got, new Promise((r) => setTimeout(r, 1500))]), frame?.loaded() ?? Promise.resolve());
    return () => onDestroy();
  });
  let onDestroy = () => {};
</script>

<Frame bind:this={frame} {path} onsay={(what) => (error = what)}>
  <div class="preview">
    <Preview
      bind:this={preview}
      {html}
      {error}
      {block}
      onsource={(text, word, around) => void tell({ kind: "source", text, word, around })}
      onblock={(index) => void tell({ kind: "block", index })}
      onpdf={(text, word, around) => void tell({ kind: "pdf", text, word, around })}
    />
  </div>
</Frame>

<style>
  .preview {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
</style>
