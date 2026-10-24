<script lang="ts">
  import { api, errorText, type VersionEntry } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";

  let dialog: HTMLDialogElement | undefined = $state();
  let name = $state("");
  let nameEdited = $state(false);
  let version = $state("");
  let showSnapshots = $state(false);
  let versions = $state<VersionEntry[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let creating = $state(false);

  $effect(() => {
    if (launcher.newInstanceOpen) {
      name = "";
      nameEdited = false;
      error = null;
      dialog?.showModal();
    } else {
      dialog?.close();
    }
  });

  $effect(() => {
    if (!launcher.newInstanceOpen) return;
    const include = showSnapshots;
    loading = true;
    api
      .listVersions(include)
      .then((list) => {
        versions = list;
        if (!list.some((v) => v.id === version)) version = list[0]?.id ?? "";
      })
      .catch((e) => (error = `Couldn't load Minecraft versions. ${errorText(e)}`))
      .finally(() => (loading = false));
  });

  // Suggest a name from the version until the person types their own.
  $effect(() => {
    if (!nameEdited && version) name = `Minecraft ${version}`;
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    creating = true;
    try {
      await launcher.create(name, version);
      launcher.newInstanceOpen = false;
    } catch (err) {
      error = errorText(err);
    } finally {
      creating = false;
    }
  }
</script>

<dialog
  class="sheet"
  bind:this={dialog}
  aria-labelledby="new-instance-title"
  onclose={() => (launcher.newInstanceOpen = false)}
>
  <form onsubmit={submit}>
    <h2 id="new-instance-title">New Instance</h2>
    <p class="secondary">Each instance keeps its own worlds, mods and settings.</p>

    <div class="grid">
      <label for="instance-name">Name</label>
      <input
        id="instance-name"
        class="field"
        bind:value={name}
        oninput={() => (nameEdited = true)}
        autocomplete="off"
        spellcheck="false"
        required
      />

      <label for="instance-version">Version</label>
      <div>
        <select id="instance-version" class="field" bind:value={version} disabled={loading || !versions.length}>
          {#if loading && !versions.length}
            <option value="">Loading versions…</option>
          {/if}
          {#each versions as v (v.id)}
            <option value={v.id}>{v.id}{v.type === "snapshot" ? " (Snapshot)" : ""}</option>
          {/each}
        </select>
        <label class="checkbox">
          <input type="checkbox" bind:checked={showSnapshots} />
          Show snapshots
        </label>
      </div>
    </div>

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    <div class="actions">
      <button type="button" class="button" onclick={() => (launcher.newInstanceOpen = false)}>Cancel</button>
      <button type="submit" class="button default" disabled={!name.trim() || !version || creating}>Create</button>
    </div>
  </form>
</dialog>

<style>
  p {
    margin: 0 0 16px;
  }

  .grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 10px 12px;
    align-items: baseline;
  }

  .grid > label {
    text-align: right;
  }

  .checkbox {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
  }

  .checkbox input {
    margin: 0;
  }

  .error {
    margin: 12px 0 0;
    color: var(--danger);
  }
</style>
