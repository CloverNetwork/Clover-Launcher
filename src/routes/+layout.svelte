<!--
  Every window: the platform's look, and none of the web engine's own behaviour (its
  right-click menu, reload and zoom keys) that would give the app away as a web page.
-->
<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import { language } from "$lib/i18n";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { commandKey, platform } from "$lib/platform";

  let { children } = $props();

  document.documentElement.dataset.platform = platform;

  /** Text people can select and copy keeps the system's Copy menu. */
  function keepsSystemMenu(target: EventTarget | null): boolean {
    if (!(target instanceof Element)) return false;
    if (target.closest("input, textarea, [contenteditable='true']")) return true;
    return !!target.closest("pre, [data-selectable]") && !!window.getSelection()?.toString();
  }

  function onContextMenu(e: MouseEvent) {
    if (!keepsSystemMenu(e.target)) e.preventDefault();
  }

  function onKeydown(e: KeyboardEvent) {
    const key = e.key.toLowerCase();
    // Reload, print and page zoom belong to browsers, not apps.
    const browserOnly =
      e.key === "F5" ||
      (commandKey(e) && (key === "r" || key === "p" || key === "=" || key === "+" || key === "-" || key === "0"));
    if (browserOnly) e.preventDefault();
  }

  // Windows start hidden and appear once drawn, so there's no blank flash first.
  onMount(() => {
    requestAnimationFrame(() => getCurrentWindow().show());
    document.documentElement.lang = language;
    api.setLanguage(language).catch(() => {});
  });
</script>

<svelte:window oncontextmenu={onContextMenu} onkeydown={onKeydown} />

{@render children()}
