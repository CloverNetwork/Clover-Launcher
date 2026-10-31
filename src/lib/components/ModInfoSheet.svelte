<!-- What one installed mod or pack is, with what can be done to it. -->
<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, errorText, type InstalledMod, type ModDetails, type ModUpdate, type Place } from "$lib/api";
  import { locale, t } from "$lib/i18n";
  import BlockIcon from "./BlockIcon.svelte";

  let {
    instanceId,
    place,
    mod,
    update,
    running,
    onclose,
    ontoggle,
    onremove,
    onupdate,
  }: {
    instanceId: string;
    place: Place;
    mod: InstalledMod | null;
    update?: ModUpdate;
    running: boolean;
    onclose: () => void;
    ontoggle: (mod: InstalledMod) => void;
    onremove: (mod: InstalledMod) => void;
    onupdate: (update: ModUpdate) => void;
  } = $props();

  let dialog: HTMLDialogElement | undefined = $state();
  let details = $state<ModDetails | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    if (mod) dialog?.showModal();
    else dialog?.close();
  });

  $effect(() => {
    const file = mod?.file;
    details = null;
    error = null;
    if (!file) return;
    let current = true;
    api
      .modDetails(instanceId, place, file)
      .then((d) => current && (details = d))
      .catch((e) => current && (error = errorText(e)));
    return () => (current = false);
  });

  const numbers = new Intl.NumberFormat(locale, { notation: "compact", maximumFractionDigits: 1 });
  const bytes = (n: number) =>
    n >= 1048576 ? `${(n / 1048576).toLocaleString(locale, { maximumFractionDigits: 1 })} MB` : `${Math.max(1, Math.round(n / 1024))} KB`;
  const sourceName = (source: string | null | undefined) =>
    source === "modrinth" ? "Modrinth" : source === "curseforge" ? "CurseForge" : t("Added by hand");
  const icon = $derived(details?.iconUrl ?? mod?.iconUrl);
  const version = $derived(details?.version ?? mod?.version);
</script>

<dialog class="sheet mod-info" bind:this={dialog} aria-labelledby="mod-info-title" onclose={onclose}>
  {#if mod}
    <header>
      {#if icon}
        <img class="icon" src={icon} alt="" width="56" height="56" />
      {:else}
        <BlockIcon seed={mod.file} size={56} />
      {/if}
      <div class="heading">
        <h2 id="mod-info-title">{details?.title ?? mod.title}</h2>
        {#if details?.authors.length}
          <p class="secondary">{t("by {names}", { names: details.authors.join(", ") })}</p>
        {/if}
      </div>
    </header>

    <div class="scroll">
      {#if details?.summary}
        <p class="summary">{details.summary}</p>
      {:else if !details && !error}
        <p class="secondary summary" role="status">{t("Loading…")}</p>
      {/if}
      {#if error}
        <p class="error" role="alert">{error}</p>
      {/if}

      {#if details?.categories.length}
        <ul class="tags" aria-label={t("Categories")}>
          {#each details.categories as category (category)}
            <li>{category}</li>
          {/each}
        </ul>
      {/if}

      <dl class="facts">
        {#if version}
          <div>
            <dt>{t("Version")}</dt>
            <dd>{version}{#if update}<span class="new"> → {update.newVersion}</span>{/if}</dd>
          </div>
        {/if}
        <div><dt>{t("Status")}</dt><dd>{mod.enabled ? t("On") : t("Off")}{mod.dependency ? ` · ${t("Required by another mod")}` : ""}</dd></div>
        <div><dt>{t("From")}</dt><dd>{sourceName(mod.source ?? details?.source)}</dd></div>
        {#if details?.downloads != null}
          <div><dt>{t("Downloads")}</dt><dd>{numbers.format(details.downloads)}</dd></div>
        {/if}
        {#if details?.license}
          <div><dt>{t("License")}</dt><dd>{details.license}</dd></div>
        {/if}
        {#if details?.modIds.length}
          <div><dt>{t("Mod ID")}</dt><dd class="mono">{details.modIds.join(", ")}</dd></div>
        {/if}
        <div>
          <dt>{t("File")}</dt>
          <dd class="file" title={mod.file}>{mod.file}{details?.size ? ` · ${bytes(details.size)}` : ""}</dd>
        </div>
      </dl>

      {#if details?.gallery.length}
        <div class="gallery" aria-label={t("Screenshots")}>
          {#each details.gallery as image (image)}
            <button class="shot" title={t("Open Image")} onclick={() => openUrl(image)}>
              <img src={image} alt="" loading="lazy" />
            </button>
          {/each}
        </div>
      {/if}

      {#if details?.links.length}
        <div class="links">
          {#each details.links as link (link.url)}
            <button class="button small" title={link.url} onclick={() => openUrl(link.url)}>{t(link.label)}</button>
          {/each}
        </div>
      {/if}
    </div>

    <div class="actions">
      <button class="button destructive" onclick={() => onremove(mod)}>{t("Remove")}</button>
      <div class="spacer"></div>
      {#if update}
        <button class="button" disabled={running} onclick={() => onupdate(update)}>{t("Update to {version}", { version: update.newVersion })}</button>
      {/if}
      <button class="button" onclick={() => ontoggle(mod)}>{mod.enabled ? t("Turn Off") : t("Turn On")}</button>
      <button class="button default" onclick={onclose}>{t("Done")}</button>
    </div>
  {/if}
</dialog>

<style>
  dialog.mod-info {
    width: min(520px, calc(100vw - 32px));
  }

  header {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 14px;
  }

  .icon {
    flex: none;
    border-radius: 12px;
    object-fit: cover;
  }

  .heading {
    min-width: 0;
  }

  .heading p {
    margin: 0;
    font-size: 12px;
  }

  .scroll {
    max-height: min(440px, 55vh);
    overflow-y: auto;
  }

  .summary {
    margin: 0 0 12px;
    line-height: 1.45;
    white-space: pre-line;
  }

  .error {
    margin: 0 0 12px;
    color: var(--danger);
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0 0 12px;
    padding: 0;
    list-style: none;
  }

  .tags li {
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--bg-control);
    color: var(--text-secondary);
    font-size: 11px;
    text-transform: capitalize;
  }

  .facts {
    margin: 0;
    border-radius: var(--radius-panel);
    background: var(--bg-control);
  }

  .facts div {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    padding: 7px 12px;
  }

  .facts div + div {
    border-top: 1px solid var(--separator);
  }

  dt {
    flex: none;
    font-weight: 500;
  }

  dd {
    min-width: 0;
    margin: 0;
    color: var(--text-secondary);
    text-align: right;
  }

  .file {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 12px;
  }

  .new {
    color: var(--accent);
  }

  .gallery {
    display: flex;
    gap: 8px;
    margin-top: 12px;
    overflow-x: auto;
    padding-bottom: 4px;
  }

  .shot {
    flex: none;
    padding: 0;
    border: none;
    border-radius: 8px;
    overflow: hidden;
    background: var(--bg-control);
    cursor: pointer;
  }

  .shot:focus-visible {
    box-shadow: var(--focus-ring);
  }

  .shot img {
    display: block;
    width: 160px;
    height: 90px;
    object-fit: cover;
  }

  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 12px;
  }

  .small {
    min-height: 22px;
    padding: 2px 10px;
    font-size: 12px;
  }

  .actions .spacer {
    flex: 1;
  }
</style>
