<script lang="ts">
  import { onMount } from "svelte";
  import { makeEditor } from "$lib/editor";
  import "$lib/theme.css";

  let element: HTMLElement;

  onMount(() => {
    makeEditor(element, "", () => {});
  });
</script>

<main bind:this={element}></main>

<style>
  main {
    height: 100vh;
  }
</style>
