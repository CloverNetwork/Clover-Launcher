<script lang="ts">
  import { ask } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, errorText } from "$lib/api";
  import Icon from "./Icon.svelte";

  let {
    instanceId,
    lines,
    onPopOut,
    standalone = false,
  }: {
    instanceId: string;
    lines: string[];
    /** Shows the button that moves the log into its own window. */
    onPopOut?: () => void;
    /** Fills its own window instead of sitting under the instance page. */
    standalone?: boolean;
  } = $props();

  let scroller: HTMLDivElement | undefined = $state();
  let pinned = true;
  let uploading = $state(false);
  let shared = $state<string | null>(null);
  let copied = $state(false);
  let error = $state<string | null>(null);

  // Follow new output unless the person has scrolled up to read.
  $effect.pre(() => {
    lines.length;
    if (!scroller) return;
    pinned = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 24;
  });

  $effect(() => {
    lines.length;
    if (pinned && scroller) scroller.scrollTop = scroller.scrollHeight;
  });

  // A fresh launch starts a fresh log, so an earlier link no longer matches it.
  $effect(() => {
    if (lines.length === 0) shared = error = null;
  });

  async function upload() {
    const confirmed = await ask(
      "Anyone with the link can read it. Clover removes your access token and home folder first, and mclo.gs also hides IP addresses.",
      { title: "Upload this log to mclo.gs?", okLabel: "Upload", cancelLabel: "Cancel" },
    );
    if (!confirmed) return;
    uploading = true;
    error = null;
    try {
      shared = (await api.uploadLog(instanceId)).url;
    } catch (e) {
      error = errorText(e);
    } finally {
      uploading = false;
    }
    // Copying without a click can be refused after the dialog; the link stays visible either way.
    await copy().catch(() => {});
  }

  async function copy() {
    if (!shared) return;
    await navigator.clipboard.writeText(shared);
    copied = true;
    setTimeout(() => (copied = false), 2000);
  }
</script>

<section class="log" class:standalone aria-label="Game Log">
  <header>
    <h2>Game Log</h2>
    <p class="status" role="status" aria-live="polite">
      {#if uploading}
        <span class="secondary">Uploading…</span>
      {:else if error}
        <span class="error">{error}</span>
      {:else if shared}
        <button class="link" onclick={() => openUrl(shared!)}>{shared.replace(/^https?:\/\//, "")}</button>
        <button class="link" onclick={copy}>{copied ? "Copied" : "Copy Link"}</button>
      {/if}
    </p>
    <button
      class="icon-button"
      aria-label="Upload to mclo.gs"
      title="Upload to mclo.gs"
      disabled={uploading || !lines.length}
      onclick={upload}
    >
      <Icon name="upload" size={14} />
    </button>
    {#if onPopOut}
      <button class="icon-button" aria-label="Open in Separate Window" title="Open in Separate Window" onclick={onPopOut}>
        <Icon name="popout" size={14} />
      </button>
    {/if}
  </header>
  <!-- Focusable so the log can be scrolled from the keyboard. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="scroller" bind:this={scroller} tabindex="0" role="log">
    {#if lines.length}
      <pre>{lines.join("\n")}</pre>
    {:else}
      <p class="secondary">Output from the game appears here while it runs.</p>
    {/if}
  </div>
</section>

<style>
  .log {
    display: flex;
    flex-direction: column;
    flex: 0 0 38%;
    min-height: 120px;
    border-top: 1px solid var(--separator);
    background: var(--bg-log);
  }

  .log.standalone {
    flex: 1;
    height: 100vh;
    border-top: none;
  }

  header {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: none;
    height: 30px;
    padding: 0 6px 0 16px;
    border-bottom: 1px solid var(--separator);
  }

  h2 {
    margin: 0;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .status {
    flex: 1;
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    min-width: 0;
    margin: 0 8px;
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
  }

  .status .error {
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--danger);
  }

  .link {
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    font-size: 11px;
  }

  .link:hover {
    text-decoration: underline;
  }

  header .icon-button {
    width: 24px;
    height: 24px;
  }

  header .icon-button:disabled {
    opacity: 0.4;
  }

  .scroller {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 10px 16px;
  }

  pre {
    margin: 0;
    font: 11px/1.5 var(--mono);
    white-space: pre-wrap;
    word-break: break-word;
    -webkit-user-select: text;
    user-select: text;
  }

  .scroller > p {
    margin: 0;
  }
</style>
