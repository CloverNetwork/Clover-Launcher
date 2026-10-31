<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, errorText, type LoaderVersion, type VersionEntry } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import { t } from "$lib/i18n";

  /** Pseudo-loader for importing any launcher profile JSON. */
  const IMPORT = "custom";

  let dialog: HTMLDialogElement | undefined = $state();
  let name = $state("");
  let nameEdited = $state(false);
  let version = $state("");
  let showAll = $state(false);
  let versions = $state<VersionEntry[]>([]);
  let loadingVersions = $state(false);

  let loader = $state("vanilla");
  let loaderVersion = $state("");
  let loaderVersions = $state<LoaderVersion[]>([]);
  let loadingLoaderVersions = $state(false);
  let profilePath = $state<string | null>(null);

  let error = $state<string | null>(null);
  let creating = $state(false);

  const customLoaders = $derived(launcher.loaders.filter((l) => l.custom));
  const loaderName = $derived(launcher.loaders.find((l) => l.id === loader)?.name ?? "");
  const importing = $derived(loader === IMPORT);
  const profileFile = $derived(profilePath?.split(/[\\/]/).pop() ?? null);

  const canCreate = $derived.by(() => {
    if (!name.trim() || creating) return false;
    if (importing) return !!profilePath;
    if (!version) return false;
    return loader === "vanilla" || !!loaderVersion;
  });

  $effect(() => {
    if (launcher.newInstanceOpen) {
      name = "";
      nameEdited = false;
      error = null;
      profilePath = null;
      launcher.reloadLoaders();
      dialog?.showModal();
    } else {
      dialog?.close();
    }
  });

  $effect(() => {
    if (!launcher.newInstanceOpen) return;
    const includeAll = showAll;
    loadingVersions = true;
    api
      .listVersions(includeAll)
      .then((list) => {
        versions = list;
        if (!list.some((v) => v.id === version)) version = list[0]?.id ?? "";
      })
      .catch((e) => (error = t("Couldn't load Minecraft versions. {error}", { error: errorText(e) })))
      .finally(() => (loadingVersions = false));
  });

  // Fetch loader versions whenever the loader or game version changes.
  $effect(() => {
    const [id, game] = [loader, version];
    loaderVersions = [];
    loaderVersion = "";
    if (id === "vanilla" || id === IMPORT || !game) return;
    loadingLoaderVersions = true;
    let current = true;
    api
      .listLoaderVersions(id, game)
      .then((list) => {
        if (!current) return;
        loaderVersions = list;
        loaderVersion = (list.find((v) => v.stable) ?? list[0])?.version ?? "";
      })
      .catch((e) => current && (error = errorText(e)))
      .finally(() => current && (loadingLoaderVersions = false));
    return () => (current = false);
  });

  // Suggest a name until the person types their own.
  $effect(() => {
    if (nameEdited) return;
    if (importing) name = profileFile?.replace(/\.json$/i, "") ?? "";
    else if (version) name = loader === "vanilla" ? `Minecraft ${version}` : `${loaderName} ${version}`;
  });

  async function chooseProfile() {
    const path = await open({
      title: t("Choose a Launcher Profile"),
      filters: [{ name: t("Launcher Profile"), extensions: ["json"] }],
    });
    if (typeof path === "string") profilePath = path;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!canCreate) return;
    creating = true;
    error = null;
    try {
      if (importing) await launcher.createFromProfile(name, profilePath!);
      else await launcher.create(name, version, loader, loader === "vanilla" ? null : loaderVersion);
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
    <h2 id="new-instance-title">{t("New Instance")}</h2>
    <p class="intro secondary">{t("Each instance keeps its own worlds, mods and settings.")}</p>

    <div class="grid">
      <label for="instance-name">{t("Name")}</label>
      <input
        id="instance-name"
        class="field"
        bind:value={name}
        oninput={() => (nameEdited = true)}
        autocomplete="off"
        spellcheck="false"
        required
      />

      <label for="instance-loader">{t("Mod Loader")}</label>
      <select id="instance-loader" class="field" bind:value={loader}>
        <option value="vanilla">{t("None")}</option>
        <hr />
        {#each launcher.mainLoaders as source (source.id)}
          <option value={source.id}>{source.name}</option>
        {/each}
        <hr />
        {#each launcher.otherLoaders as source (source.id)}
          <option value={source.id}>{source.name}</option>
        {/each}
        {#if customLoaders.length}
          <optgroup label={t("Added in Settings")}>
            {#each customLoaders as source (source.id)}
              <option value={source.id}>{source.name}</option>
            {/each}
          </optgroup>
        {/if}
        <optgroup label={t("Other")}>
          <option value={IMPORT}>{t("Import Launcher Profile…")}</option>
        </optgroup>
      </select>

      {#if importing}
        <span class="label">{t("Profile")}</span>
        <div>
          <div class="file">
            <button type="button" class="button" onclick={chooseProfile}>{t("Choose File…")}</button>
            <span class="secondary file-name">{profileFile ?? t("No file chosen")}</span>
          </div>
          <p class="hint secondary">
            {t("Any launcher profile JSON works, including ones made by a loader's own installer. Clover downloads the Minecraft version it builds on.")}
          </p>
        </div>
      {:else}
        <label for="instance-version">Minecraft</label>
        <div>
          <select id="instance-version" class="field" bind:value={version} disabled={loadingVersions && !versions.length}>
            {#if loadingVersions && !versions.length}
              <option value="">{t("Loading versions…")}</option>
            {/if}
            {#each versions as v (v.id)}
              <option value={v.id}>{v.id}{v.type === "release" ? "" : ` (${v.type.replace("old_", "").replace("_", " ")})`}</option>
            {/each}
          </select>
          <label class="checkbox">
            <input type="checkbox" bind:checked={showAll} />
            {t("Show snapshots and old versions")}
          </label>
        </div>

        {#if loader !== "vanilla"}
          <label for="loader-version">{t("{loader} Version", { loader: loaderName })}</label>
          <div>
            {#if loadingLoaderVersions}
              <select id="loader-version" class="field" disabled><option>{t("Loading versions…")}</option></select>
            {:else if loaderVersions.length}
              <select id="loader-version" class="field" bind:value={loaderVersion}>
                {#each loaderVersions as v (v.version)}
                  <option value={v.version}>{v.label}{v.stable ? "" : " (Beta)"}</option>
                {/each}
              </select>
            {:else}
              <p class="unavailable secondary" role="status">
                {t("{loader} isn't available for Minecraft {version}. Try another version, or turn on old versions for legacy loaders.", { loader: loaderName, version })}
              </p>
            {/if}
          </div>
        {/if}
      {/if}
    </div>

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    <div class="actions">
      <button
        type="button"
        class="button import"
        onclick={() => {
          launcher.newInstanceOpen = false;
          launcher.importOpen = true;
        }}
      >
        {t("Import Modpack…")}
      </button>
      <button type="button" class="button" onclick={() => (launcher.newInstanceOpen = false)}>{t("Cancel")}</button>
      <button type="submit" class="button default" disabled={!canCreate}>{t("Create")}</button>
    </div>
  </form>
</dialog>

<style>
  .intro {
    margin: 0 0 16px;
  }

  .grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 10px 12px;
    align-items: baseline;
  }

  .grid > label,
  .grid > .label {
    text-align: right;
    white-space: nowrap;
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

  .file {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .file-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hint,
  .unavailable {
    margin: 6px 0 0;
    font-size: 11px;
  }

  .unavailable {
    margin: 0;
    font-size: 13px;
  }

  .import {
    margin-right: auto;
  }

  .error {
    margin: 12px 0 0;
    color: var(--danger);
  }
</style>
