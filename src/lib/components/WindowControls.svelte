<!--
  Full Screen and Quit (or Close) buttons for windows without a title bar, which is how
  Clover runs on Hyprland. Shown only when turned on in Settings. There's no minimize
  button: Hyprland has no minimized windows.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { api, events, type WindowChrome } from "$lib/api";
  import Icon from "./Icon.svelte";

  /** The main window quits the app; other windows just close. */
  let { quits = false }: { quits?: boolean } = $props();

  let chrome = $state<WindowChrome | null>(null);

  onMount(() => {
    api.windowChrome().then((c) => (chrome = c));
    const unlisten = events.windowChrome((c) => (chrome = c));
    return () => unlisten.then((fn) => fn());
  });

  async function toggleFullscreen() {
    const window = getCurrentWindow();
    await window.setFullscreen(!(await window.isFullscreen()));
  }

  const close = () => (quits ? api.quit() : getCurrentWindow().close());
</script>

{#if chrome?.buttons}
  <div class="controls">
    <button class="icon-button" aria-label="Full Screen" title="Full Screen" onclick={toggleFullscreen}>
      <Icon name="fullscreen" size={14} />
    </button>
    <button class="icon-button" aria-label={quits ? "Quit" : "Close"} title={quits ? "Quit" : "Close"} onclick={close}>
      <Icon name="close" size={14} />
    </button>
  </div>
{/if}

<style>
  .controls {
    display: inline-flex;
    gap: 2px;
    margin-left: 4px;
    padding-left: 6px;
    border-left: 1px solid var(--separator);
  }
</style>
