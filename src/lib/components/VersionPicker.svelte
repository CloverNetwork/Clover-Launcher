<!--
  Minecraft version, mod loader and loader version, as rows of a two-column form.
  Lists only loader versions that support the chosen Minecraft version.
-->
<script lang="ts">
  import { api, errorText, type LoaderVersion, type VersionEntry } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import { t } from "$lib/i18n";

  let {
    version = $bindable(),
    loader = $bindable(),
    loaderVersion = $bindable(),
  }: { version: string; loader: string; loaderVersion: string | null } = $props();

  let versions = $state<VersionEntry[]>([]);
  let showAll = $state(false);
  let loaderVersions = $state<LoaderVersion[]>([]);
  let loadingLoaders = $state(false);
  let error = $state<string | null>(null);

  const loaderName = $derived(launcher.loaders.find((l) => l.id === loader)?.name ?? loader);

  $effect(() => {
    const includeAll = showAll;
    api
      .listVersions(includeAll)
      .then((list) => {
        // Keep the current version listed even when it's an old one or a snapshot.
        versions = list.some((v) => v.id === version) || !version ? list : [{ id: version, type: "", releaseTime: "" }, ...list];
      })
      .catch((e) => (error = errorText(e)));
  });

  $effect(() => {
    const [id, game] = [loader, version];
    if (id === "vanilla" || !game) {
      loaderVersions = [];
      loaderVersion = null;
      return;
    }
    let current = true;
    loadingLoaders = true;
    api
      .listLoaderVersions(id, game)
      .then((list) => {
        if (!current) return;
        loaderVersions = list;
        if (!list.some((v) => v.version === loaderVersion)) {
          loaderVersion = (list.find((v) => v.stable) ?? list[0])?.version ?? null;
        }
      })
      .catch((e) => current && (error = errorText(e)))
      .finally(() => current && (loadingLoaders = false));
    return () => (current = false);
  });
</script>

<label for="picker-version">Minecraft</label>
<div>
  <select id="picker-version" class="field" bind:value={version}>
    {#each versions as v (v.id)}
      <option value={v.id}>{v.id}{v.type && v.type !== "release" ? ` (${v.type.replace("old_", "").replace("_", " ")})` : ""}</option>
    {/each}
  </select>
  <label class="checkbox">
    <input type="checkbox" bind:checked={showAll} />
    {t("Show snapshots and old versions")}
  </label>
</div>

<label for="picker-loader">{t("Mod Loader")}</label>
<select id="picker-loader" class="field" bind:value={loader}>
  <option value="vanilla">{t("None")}</option>
  <hr />
  {#each launcher.mainLoaders as source (source.id)}
    <option value={source.id}>{source.name}</option>
  {/each}
  <hr />
  {#each launcher.otherLoaders as source (source.id)}
    <option value={source.id}>{source.name}</option>
  {/each}
  {#if launcher.loaders.some((l) => l.custom)}
    <hr />
    {#each launcher.loaders.filter((l) => l.custom) as source (source.id)}
      <option value={source.id}>{source.name}</option>
    {/each}
  {/if}
</select>

{#if loader !== "vanilla"}
  <label for="picker-loader-version">{t("{loader} Version", { loader: loaderName })}</label>
  <div>
    {#if loadingLoaders}
      <select id="picker-loader-version" class="field" disabled><option>{t("Loading versions…")}</option></select>
    {:else if loaderVersions.length}
      <select id="picker-loader-version" class="field" bind:value={loaderVersion}>
        {#each loaderVersions as v (v.version)}
          <option value={v.version}>{v.label}{v.stable ? "" : " (Beta)"}</option>
        {/each}
      </select>
    {:else}
      <p class="secondary unavailable">{t("{loader} isn't available for Minecraft {version}.", { loader: loaderName, version })}</p>
    {/if}
  </div>
{/if}

{#if error}
  <span></span>
  <p class="error">{error}</p>
{/if}

<style>
  .checkbox {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
    font-size: 12px;
  }

  .checkbox input {
    margin: 0;
  }

  .unavailable,
  .error {
    margin: 0;
  }

  .error {
    color: var(--danger);
  }
</style>
