<!-- The settings window (⌘,). Changes apply immediately; there is no Save button. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorText, type Settings } from "$lib/api";

  let settings = $state<Settings | null>(null);
  let error = $state<string | null>(null);
  let saveTimer: ReturnType<typeof setTimeout>;

  onMount(async () => {
    settings = await api.getSettings();
  });

  function save() {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      if (!settings) return;
      try {
        await api.saveSettings($state.snapshot(settings));
        error = null;
      } catch (e) {
        error = errorText(e);
      }
    }, 300);
  }

  const gigabytes = (mb: number) => `${(mb / 1024).toLocaleString(undefined, { maximumFractionDigits: 1 })} GB`;
</script>

{#if settings}
  <form class="settings" onsubmit={(e) => e.preventDefault()}>
    <label for="java">Java:</label>
    <div>
      <input id="java" class="field" bind:value={settings.javaPath} oninput={save} spellcheck="false" autocomplete="off" />
      <p class="hint secondary">Path to a Java executable. Leave as “java” to use the one on your PATH.</p>
    </div>

    <label for="memory">Memory:</label>
    <div>
      <div class="slider">
        <input
          id="memory"
          type="range"
          min="1024"
          max="16384"
          step="512"
          bind:value={settings.memoryMb}
          oninput={save}
          aria-valuetext={gigabytes(settings.memoryMb)}
        />
        <output for="memory">{gigabytes(settings.memoryMb)}</output>
      </div>
      <p class="hint secondary">Maximum memory Minecraft can use. 4 GB suits most worlds; modded packs may need more.</p>
    </div>

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
  </form>
{/if}

<style>
  .settings {
    display: grid;
    grid-template-columns: 96px 1fr;
    gap: 16px 10px;
    align-items: baseline;
    padding: 24px 28px;
  }

  label {
    text-align: right;
  }

  .hint {
    margin: 4px 0 0;
    font-size: 11px;
  }

  .slider {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .slider input {
    flex: 1;
    accent-color: var(--accent);
  }

  output {
    min-width: 48px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .error {
    grid-column: 2;
    margin: 0;
    color: var(--danger);
  }
</style>
