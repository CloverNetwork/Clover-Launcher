<!--
  Edit Instance: name and group, versions, Java and memory, the game window, and commands
  around a launch. Everything left empty uses the defaults from Settings.
-->
<script lang="ts">
  import { api, errorText, type InstanceOptions } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import VersionPicker from "./VersionPicker.svelte";

  type Window = "default" | "size" | "fullscreen";

  let dialog: HTMLDialogElement | undefined = $state();
  let name = $state("");
  let group = $state("");
  let version = $state("");
  let loader = $state("vanilla");
  let loaderVersion = $state<string | null>(null);
  let ownMemory = $state(false);
  let memoryMb = $state(4096);
  let defaultMemory = $state(4096);
  let javaPath = $state("");
  let jvmArgs = $state("");
  let windowMode = $state<Window>("default");
  let width = $state(1280);
  let height = $state(720);
  let preLaunch = $state("");
  let postExit = $state("");
  let saving = $state(false);
  let error = $state<string | null>(null);

  const instance = $derived(launcher.selected);
  const custom = $derived(instance?.loader === "custom");

  $effect(() => {
    if (launcher.editOpen && instance) {
      load();
      dialog?.showModal();
    } else {
      dialog?.close();
    }
  });

  async function load() {
    const i = launcher.selected!;
    const o = i.options;
    error = null;
    name = i.name;
    group = o.group ?? "";
    version = i.version;
    loader = i.loader;
    loaderVersion = i.loaderVersion;
    defaultMemory = (await api.getSettings()).memoryMb;
    ownMemory = o.memoryMb !== null;
    memoryMb = o.memoryMb ?? defaultMemory;
    javaPath = o.javaPath ?? "";
    jvmArgs = o.jvmArgs;
    windowMode = o.fullscreen ? "fullscreen" : o.width && o.height ? "size" : "default";
    width = o.width ?? 1280;
    height = o.height ?? 720;
    preLaunch = o.preLaunch;
    postExit = o.postExit;
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!instance) return;
    saving = true;
    error = null;
    const options: InstanceOptions = {
      group: group.trim() || null,
      memoryMb: ownMemory ? memoryMb : null,
      javaPath: javaPath.trim() || null,
      jvmArgs,
      width: windowMode === "size" ? width : null,
      height: windowMode === "size" ? height : null,
      fullscreen: windowMode === "fullscreen",
      preLaunch,
      postExit,
    };
    try {
      const updated = await api.updateInstance(instance.id, { name, version, loader, loaderVersion, options });
      launcher.replace(updated);
      launcher.modsRevision++;
      launcher.editOpen = false;
    } catch (err) {
      error = errorText(err);
    } finally {
      saving = false;
    }
  }

  const gigabytes = (mb: number) => `${(mb / 1024).toLocaleString(undefined, { maximumFractionDigits: 1 })} GB`;
</script>

<dialog class="sheet edit" bind:this={dialog} aria-labelledby="edit-title" onclose={() => (launcher.editOpen = false)}>
  <form onsubmit={save}>
    <h2 id="edit-title">Edit Instance</h2>

    <div class="scroll">
      <h3>General</h3>
      <div class="grid">
        <label for="edit-name">Name</label>
        <input id="edit-name" class="field" bind:value={name} autocomplete="off" spellcheck="false" required />
        <label for="edit-group">Group</label>
        <div>
          <input id="edit-group" class="field" list="edit-groups" bind:value={group} placeholder="None" autocomplete="off" />
          <datalist id="edit-groups">
            {#each launcher.groupNames as g (g)}
              <option value={g}></option>
            {/each}
          </datalist>
        </div>
      </div>

      <h3>Version</h3>
      <div class="grid">
        {#if custom}
          <span class="label">Profile</span>
          <p class="secondary">A custom launcher profile ({instance?.loaderVersion}). Its version can't be changed here.</p>
        {:else}
          <VersionPicker bind:version bind:loader bind:loaderVersion />
        {/if}
      </div>
      {#if instance && !custom && (version !== instance.version || loader !== instance.loader || loaderVersion !== instance.loaderVersion)}
        <p class="note secondary">
          Clover installs the new version on the next launch. Mods stay; the Mods section offers updates that match.
        </p>
      {/if}

      <h3>Java and Memory</h3>
      <div class="grid">
        <span class="label">Memory</span>
        <div>
          <label class="checkbox">
            <input type="checkbox" checked={!ownMemory} onchange={(e) => (ownMemory = !e.currentTarget.checked)} />
            Use the default ({gigabytes(defaultMemory)})
          </label>
          {#if ownMemory}
            <div class="slider">
              <input type="range" min="1024" max="32768" step="512" bind:value={memoryMb} aria-label="Memory" aria-valuetext={gigabytes(memoryMb)} />
              <output>{gigabytes(memoryMb)}</output>
            </div>
          {/if}
        </div>
        <label for="edit-java">Java</label>
        <input id="edit-java" class="field" bind:value={javaPath} placeholder="Automatic" spellcheck="false" autocomplete="off" />
        <label for="edit-jvm">JVM Arguments</label>
        <input id="edit-jvm" class="field mono" bind:value={jvmArgs} placeholder="-XX:+UseG1GC" spellcheck="false" autocomplete="off" />
      </div>

      <h3>Game Window</h3>
      <div class="grid">
        <span class="label">Size</span>
        <div class="choices" role="radiogroup" aria-label="Game window size">
          <label class="checkbox"><input type="radio" bind:group={windowMode} value="default" /> Default</label>
          <label class="checkbox">
            <input type="radio" bind:group={windowMode} value="size" /> Custom
            <input class="field small" type="number" min="320" max="7680" bind:value={width} disabled={windowMode !== "size"} aria-label="Width" />
            ×
            <input class="field small" type="number" min="240" max="4320" bind:value={height} disabled={windowMode !== "size"} aria-label="Height" />
          </label>
          <label class="checkbox"><input type="radio" bind:group={windowMode} value="fullscreen" /> Full screen</label>
        </div>
      </div>

      <h3>Commands</h3>
      <div class="grid">
        <label for="edit-pre">Before Launch</label>
        <input id="edit-pre" class="field mono" bind:value={preLaunch} placeholder="None" spellcheck="false" autocomplete="off" />
        <label for="edit-post">After Exit</label>
        <input id="edit-post" class="field mono" bind:value={postExit} placeholder="None" spellcheck="false" autocomplete="off" />
        <span></span>
        <p class="hint secondary">
          Run in the game folder, with $INST_NAME, $INST_DIR, $INST_MC_DIR and $INST_JAVA set. If the command before launch
          fails, the game doesn't start.
        </p>
      </div>
    </div>

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
    <div class="actions">
      <button type="button" class="button" onclick={() => (launcher.editOpen = false)}>Cancel</button>
      <button type="submit" class="button default" disabled={saving || !name.trim()}>Save</button>
    </div>
  </form>
</dialog>

<style>
  dialog.edit {
    width: min(560px, calc(100vw - 32px));
  }

  .scroll {
    max-height: min(560px, 65vh);
    overflow-y: auto;
    margin: 0 -4px;
    padding: 0 4px;
  }

  .scroll:focus-visible {
    box-shadow: none;
  }

  h3 {
    margin: 18px 0 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .grid {
    display: grid;
    grid-template-columns: 120px 1fr;
    gap: 10px 12px;
    align-items: baseline;
  }

  .grid > .label,
  .grid > :global(label) {
    text-align: right;
  }

  .grid p {
    margin: 0;
  }

  .note {
    margin: 8px 0 0 132px;
    font-size: 12px;
  }

  .checkbox {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .checkbox input[type="checkbox"],
  .checkbox input[type="radio"] {
    margin: 0;
  }

  .choices {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .field.small {
    width: 72px;
  }

  .mono {
    font-family: var(--mono);
    font-size: 12px;
  }

  .slider {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 8px;
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

  .hint {
    font-size: 11px;
  }

  .error {
    margin: 12px 0 0;
    color: var(--danger);
  }
</style>
