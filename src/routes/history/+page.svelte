<script lang="ts">
  // The page of a window that shows a document since a commit (Git ▸ Compare with Commit…): the text of the file at
  // that commit, read only, with the changes since drawn as tracked changes by the editor's review: each a suggestion
  // "since <rev>". Nothing is written, and typing is not possible, so no suggestion is made: the handlers of the review
  // do nothing. The items come from the backend with the text (`history`): the file as it is now against the commit.

  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { EditorState } from "@codemirror/state";
  import { FileEditor, PREFS, type EditorPrefs, type ReviewHandlers, type ReviewItem } from "papers-write/editor";
  import { applyEditor, watchSettings } from "$lib/settings";
  import "$lib/theme.css";

  type History = { title: string; text: string; items: ReviewItem[]; markdown: boolean };

  let shown = $state<History | undefined>();
  let editor = $state<FileEditor>();
  let error = $state("");
  let prefs = $state<EditorPrefs>(PREFS);

  /** nothing is suggested or withdrawn in a read-only text */
  const handlers: ReviewHandlers = { me: "history", suggest: async () => "", withdraw: async () => {} };
  const readOnly = [EditorState.readOnly.of(true)];

  async function ask() {
    try {
      shown = await invoke<History>("history");
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => editor?.setReview(shown?.items ?? []));

  onMount(() => {
    const stop = watchSettings((s) => (prefs = applyEditor(s)));
    ask();
    // the document written (or changed by a pull) since the tab was opened: compared again as it gets the keys
    window.addEventListener("focus", ask);
    return () => {
      stop();
      window.removeEventListener("focus", ask);
    };
  });
</script>

<main>
  {#if shown}
    <div class="bar">{shown.title}: {shown.items.length} change{shown.items.length === 1 ? "" : "s"} since, read only</div>
    {#key shown.text}
      <div class="text">
        <FileEditor
          bind:this={editor}
          load={async () => ({ path: "", text: shown?.text ?? "" })}
          save={async () => {}}
          mono={!shown.markdown}
          {prefs}
          extensions={readOnly}
          review={handlers}
        />
      </div>
    {/key}
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
</main>

<style>
  main {
    height: 100vh;
    display: flex;
    flex-direction: column;
    position: relative;
  }
  .bar {
    flex: none;
    padding: 4px 14px;
    font-size: 11.5px;
    color: var(--muted);
    background: var(--chip);
    border-bottom: 1px solid var(--line);
  }
  .text {
    flex: 1;
    min-height: 0;
  }
  .error {
    margin: 0;
    padding: 6px 14px;
    color: var(--error);
    font-size: 12px;
  }
</style>
