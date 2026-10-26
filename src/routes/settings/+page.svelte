<!-- The settings window (⌘,). Changes apply immediately; there is no Save button. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, CURSEFORGE_KEY_FORM, errorText, type LoaderSource, type Settings, type WindowChrome } from "$lib/api";
  import WindowControls from "$lib/components/WindowControls.svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { updates } from "$lib/updates.svelte";

  let settings = $state<Settings | null>(null);
  let error = $state<string | null>(null);
  let saveTimer: ReturnType<typeof setTimeout>;

  let sourceName = $state("");
  let sourceUrl = $state("");
  let sourcePath = $state("");
  let sourceError = $state<string | null>(null);
  let keyCheck = $state<{ ok: boolean; text: string } | null>(null);
  let checkingKey = $state(false);

  async function checkKey() {
    checkingKey = true;
    try {
      await api.checkCurseforgeKey(settings!.curseforgeApiKey);
      keyCheck = { ok: true, text: "CurseForge accepted this key." };
    } catch (e) {
      keyCheck = { ok: false, text: errorText(e) };
    } finally {
      checkingKey = false;
    }
  }

  let chrome = $state<WindowChrome | null>(null);
  let version = $state("");

  onMount(async () => {
    chrome = await api.windowChrome();
    version = await getVersion();
    const loaded = await api.getSettings();
    // "java" meant "whatever is on the PATH" before automatic Java existed.
    if (loaded.javaPath.trim() === "java") loaded.javaPath = "";
    settings = loaded;
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

  async function addSource(e: SubmitEvent) {
    e.preventDefault();
    try {
      const source = await api.addLoaderSource(sourceName, sourceUrl, sourcePath);
      settings!.loaderSources = [...settings!.loaderSources, source];
      sourceName = sourceUrl = sourcePath = "";
      sourceError = null;
    } catch (err) {
      sourceError = errorText(err);
    }
  }

  async function removeSource(source: LoaderSource) {
    await api.removeLoaderSource(source.id);
    settings!.loaderSources = settings!.loaderSources.filter((s) => s.id !== source.id);
  }

  const gigabytes = (mb: number) => `${(mb / 1024).toLocaleString(undefined, { maximumFractionDigits: 1 })} GB`;
</script>

{#if settings}
  <div class="window">
    <div class="window-controls"><WindowControls /></div>
    <form class="settings" onsubmit={(e) => e.preventDefault()}>
      <label for="java">Java:</label>
      <div>
        <input
          id="java"
          class="field"
          bind:value={settings.javaPath}
          oninput={save}
          placeholder="Automatic"
          spellcheck="false"
          autocomplete="off"
        />
        <p class="hint secondary">
          Leave empty to use the Java each Minecraft version needs. Clover downloads it if this computer doesn't
          have it. Enter a path to use one Java for everything.
        </p>
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

      <label for="curseforge">CurseForge:</label>
      <div>
        <div class="key-row">
          <input
            id="curseforge"
            class="field"
            type="password"
            bind:value={settings.curseforgeApiKey}
            oninput={() => {
              keyCheck = null;
              save();
            }}
            placeholder="API key"
            spellcheck="false"
            autocomplete="off"
          />
          <button type="button" class="button" disabled={checkingKey || !settings.curseforgeApiKey.trim()} onclick={checkKey}>
            {checkingKey ? "Checking…" : "Check Key"}
          </button>
        </div>
        {#if keyCheck}
          <p class="hint" class:ok={keyCheck.ok} class:bad={!keyCheck.ok} role="status">{keyCheck.text}</p>
        {/if}
        <p class="hint secondary">
          Needed only for CurseForge mods; Modrinth works without one. CurseForge gives launchers access through a key
          approved for third-party apps, which you
          <button type="button" class="inline-link" onclick={() => openUrl(CURSEFORGE_KEY_FORM)}>apply for here</button>.
          Personal keys from the CurseForge console usually aren't accepted.
        </p>
      </div>

      {#if chrome?.hyprland}
        <span class="label">Window:</span>
        <div>
          <label class="checkbox">
            <input type="checkbox" bind:checked={settings.showWindowButtons} onchange={save} />
            Show Full Screen and Quit buttons
          </label>
          <p class="hint secondary">
            Hyprland arranges windows with your keybindings, so Clover's windows have no title bar. Turn this on to add
            buttons for full screen and quitting. There's no minimize button, since Hyprland doesn't minimize windows.
          </p>
        </div>
      {/if}

      <span class="label">Version:</span>
      <div class="key-row">
        <span>Clover {version}</span>
        <button type="button" class="button" onclick={() => updates.check(true)}>Check for Updates</button>
      </div>

      {#if error}
        <p class="error" role="alert">{error}</p>
      {/if}
    </form>

    <section class="sources" aria-labelledby="sources-title">
      <h2 id="sources-title">Extra Mod Loaders</h2>
      <p class="hint secondary">
        Add any loader with a Fabric-compatible meta server, like a fork of Fabric or Quilt. Loaders with their own
        installer can be added to an instance with Import Launcher Profile instead.
      </p>

      {#if settings.loaderSources.length}
        <ul>
          {#each settings.loaderSources as source (source.id)}
            <li>
              <div>
                <strong>{source.name}</strong>
                <span class="secondary">{source.url}/versions/{source.loaderPath}</span>
              </div>
              <button type="button" class="button destructive" onclick={() => removeSource(source)}>Remove</button>
            </li>
          {/each}
        </ul>
      {/if}

      <form class="add" onsubmit={addSource}>
        <label for="source-name">Name:</label>
        <input id="source-name" class="field" bind:value={sourceName} placeholder="My Loader" autocomplete="off" />
        <label for="source-url">Meta Server:</label>
        <input
          id="source-url"
          class="field"
          bind:value={sourceUrl}
          placeholder="https://meta.example.net/v2"
          spellcheck="false"
          autocomplete="off"
        />
        <label for="source-path">Loader Path:</label>
        <input
          id="source-path"
          class="field"
          bind:value={sourcePath}
          placeholder="loader"
          spellcheck="false"
          autocomplete="off"
        />
        <span></span>
        <div class="add-actions">
          {#if sourceError}
            <p class="error" role="alert">{sourceError}</p>
          {/if}
          <button type="submit" class="button" disabled={!sourceName.trim() || !sourceUrl.trim()}>Add Loader</button>
        </div>
      </form>
    </section>
  </div>
{/if}

<style>
  .window {
    position: relative;
    height: 100vh;
    overflow-y: auto;
  }

  .window-controls {
    position: absolute;
    top: 8px;
    right: 10px;
  }

  .label {
    text-align: right;
  }

  .checkbox {
    display: flex;
    align-items: center;
    gap: 6px;
    text-align: left;
  }

  .checkbox input {
    margin: 0;
  }

  .settings,
  .add {
    display: grid;
    grid-template-columns: 96px 1fr;
    gap: 16px 10px;
    align-items: baseline;
  }

  .settings {
    padding: 24px 28px 20px;
  }

  label {
    text-align: right;
  }

  .hint {
    margin: 4px 0 0;
    font-size: 11px;
  }

  .key-row {
    display: flex;
    gap: 8px;
  }

  .key-row .field {
    flex: 1;
  }

  .hint.ok {
    color: var(--play);
  }

  .hint.bad {
    color: var(--danger);
  }

  .inline-link {
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    font-size: inherit;
  }

  .inline-link:hover {
    text-decoration: underline;
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

  .sources {
    margin: 0 28px 24px;
    padding-top: 20px;
    border-top: 1px solid var(--separator);
  }

  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }

  ul {
    margin: 12px 0 0;
    padding: 0;
    list-style: none;
    border-radius: var(--radius-panel);
    background: var(--bg-control);
  }

  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 12px;
  }

  li + li {
    border-top: 1px solid var(--separator);
  }

  li div {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  li span {
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .add {
    gap: 10px;
    margin-top: 16px;
  }

  .add-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
  }

  .add-actions .error {
    flex: 1;
  }
</style>
