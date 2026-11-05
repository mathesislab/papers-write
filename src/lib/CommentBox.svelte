<script lang="ts">
  // A small box beside the selection to write a comment in: ⌘↵ posts it, esc closes it.
  let { x, y, onpost, oncancel }: { x: number; y: number; onpost: (body: string) => void; oncancel: () => void } = $props();
  let body = $state("");
  const grab = (node: HTMLTextAreaElement) => node.focus();

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") oncancel();
    else if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && body.trim()) onpost(body);
    else return;
    e.preventDefault();
  }
</script>

<div class="box" role="dialog" aria-label="Add Comment" style:left="{Math.max(8, Math.min(x, window.innerWidth - 308))}px" style:top="{Math.min(y, window.innerHeight - 150)}px">
  <textarea bind:value={body} use:grab onkeydown={key} placeholder="Comment" rows="3"></textarea>
  <div>
    <span>⌘↵ posts</span>
    <button onclick={oncancel}>Cancel</button>
    <button class="post" disabled={!body.trim()} onclick={() => onpost(body)}>Comment</button>
  </div>
</div>

<style>
  .box {
    position: fixed;
    z-index: 8;
    width: 300px;
    box-sizing: border-box;
    padding: 8px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.18);
    font-size: 12.5px;
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
    font: inherit;
    color: var(--fg);
    background: var(--menu-bg);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 5px 7px;
    outline: none;
  }
  textarea:focus {
    border-color: var(--accent);
  }
  div {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
  }
  span {
    flex: 1;
    color: var(--muted);
    font-size: 11.5px;
  }
  button {
    font: inherit;
    color: var(--fg);
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 1px 8px;
  }
  button:hover:not(:disabled) {
    background: var(--chip);
  }
  .post {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--on-accent);
  }
  .post:hover:not(:disabled) {
    background: var(--accent);
    filter: brightness(1.08);
  }
  button:disabled {
    opacity: 0.5;
  }
</style>
