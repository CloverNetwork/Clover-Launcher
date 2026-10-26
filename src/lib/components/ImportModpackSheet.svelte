<!--
  Import Modpack: browse Modrinth or CurseForge, or pick a .mrpack / CurseForge zip from
  this computer. Each import becomes a new instance.
-->
<script lang="ts">
  import { notifyIfAway } from "$lib/notify";
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import {
    api,
    CURSEFORGE_KEY_FORM,
    errorText,
    events,
    type BlockedFile,
    type ImportedPack,
    type ImportProgress,
    type ModHit,
    type ModSource,
  } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import { locale, t } from "$lib/i18n";
  import Icon from "./Icon.svelte";

  type Tab = ModSource | "file";

  let dialog: HTMLDialogElement | undefined = $state();
  let tab = $state<Tab>("modrinth");
  let query = $state("");
  let hits = $state<ModHit[]>([]);
  let total = $state(0);
  let loading = $state(false);
  let error = $state<string | null>(null);
  /** The pack being imported, by "source:id" or "file". */
  let importing = $state<string | null>(null);
  let progress = $state<ImportProgress | null>(null);
  let blocked = $state<{ pack: ImportedPack; files: BlockedFile[] } | null>(null);

  const needsKey = $derived(tab === "curseforge" && error?.includes(t("API key")));
  const busy = $derived(importing !== null);

  onMount(() => {
    const unlisten = events.importProgress((p) => {
      progress = p.stage ? p : { stage: progress?.stage ?? null, done: p.done, total: p.total };
    });
    return () => unlisten.then((fn) => fn());
  });

  $effect(() => {
    if (launcher.importOpen) {
      blocked = null;
      error = null;
      dialog?.showModal();
    } else {
      dialog?.close();
    }
  });

  // Search as the person types, a moment after they pause.
  $effect(() => {
    if (!launcher.importOpen || tab === "file") return;
    const [source, text] = [tab, query];
    const timer = setTimeout(() => search(source, text, 0), text ? 300 : 0);
    return () => clearTimeout(timer);
  });

  async function search(source: ModSource, text: string, offset: number) {
    loading = true;
    error = null;
    try {
      const page = await api.searchModpacks(source, text, offset);
      if (source !== tab || text !== query) return;
      hits = offset ? [...hits, ...page.hits] : page.hits;
      total = page.total;
    } catch (e) {
      if (!offset) hits = [];
      error = errorText(e);
    } finally {
      loading = false;
    }
  }

  async function run(key: string, job: () => Promise<ImportedPack>) {
    importing = key;
    progress = null;
    error = null;
    try {
      const pack = await job();
      notifyIfAway(t("Modpack Imported"), t("{name} is ready to play.", { name: pack.instance.name }));
      await launcher.showImported(pack.instance);
      if (pack.blocked.length) blocked = { pack, files: pack.blocked };
      else launcher.importOpen = false;
    } catch (e) {
      error = errorText(e);
    } finally {
      importing = null;
      progress = null;
    }
  }

  async function chooseFile() {
    const path = await open({
      title: t("Choose a Modpack"),
      filters: [{ name: t("Modpack"), extensions: ["mrpack", "zip"] }],
    });
    if (typeof path === "string") run("file", () => api.importModpackFile(path));
  }

  const compact = new Intl.NumberFormat(locale, { notation: "compact" });
  const progressText = $derived(
    progress?.total
      ? t("Downloading files: {done} of {total}", { done: progress.done.toLocaleString(locale), total: progress.total.toLocaleString(locale) })
      : (progress?.stage ?? t("Starting…")),
  );
</script>

<dialog
  class="sheet import"
  bind:this={dialog}
  aria-labelledby="import-title"
  onclose={() => (launcher.importOpen = false)}
  oncancel={(e) => busy && e.preventDefault()}
>
  <header>
    <h2 id="import-title">{t("Import Modpack")}</h2>
    <p class="secondary">{t("Each modpack becomes a new instance with its own mods and settings.")}</p>
  </header>

  {#if blocked}
    <div class="blocked">
      <p>
        {blocked.files.length === 1
          ? t("{name} is ready, but one file can only be downloaded from CurseForge's website. Download it and put it in the folder shown, inside the instance's game folder.", { name: blocked.pack.instance.name })
          : t("{name} is ready, but {n} files can only be downloaded from CurseForge's website. Download each one and put it in the folder shown, inside the instance's game folder.", { name: blocked.pack.instance.name, n: blocked.files.length })}
      </p>
      <ul>
        {#each blocked.files as file (file.name)}
          <li>
            <button class="link" onclick={() => openUrl(file.url)}>{file.name}</button>
            <span class="secondary">→ {file.folder}</span>
          </li>
        {/each}
      </ul>
    </div>
    <div class="footer">
      <span class="status"></span>
      <button class="button" onclick={() => api.showModsFolder(blocked!.pack.instance.id)}>{t("Show Mods Folder")}</button>
      <button class="button default" onclick={() => (launcher.importOpen = false)}>{t("Done")}</button>
    </div>
  {:else}
    <div class="controls">
      <div class="segmented" role="radiogroup" aria-label={t("Import from")}>
        <button role="radio" aria-checked={tab === "modrinth"} disabled={busy} onclick={() => (tab = "modrinth")}>Modrinth</button>
        <button role="radio" aria-checked={tab === "curseforge"} disabled={busy} onclick={() => (tab = "curseforge")}>CurseForge</button>
        <button role="radio" aria-checked={tab === "file"} disabled={busy} onclick={() => (tab = "file")}>{t("From File")}</button>
      </div>
      {#if tab !== "file"}
        <label class="search">
          <Icon name="search" size={14} />
          <span class="visually-hidden">{t("Search modpacks")}</span>
          <input
            type="search"
            bind:value={query}
            placeholder={t("Search {site} modpacks", { site: tab === "modrinth" ? "Modrinth" : "CurseForge" })}
            autocomplete="off"
            spellcheck="false"
            disabled={busy}
          />
        </label>
      {/if}
    </div>

    <div class="results" aria-busy={loading || busy}>
      {#if tab === "file"}
        <div class="message">
          <p>{t("Choose a Modrinth pack (.mrpack) or a CurseForge modpack .zip from this computer.")}</p>
          <button class="button default" disabled={busy} onclick={chooseFile}>
            {importing === "file" ? t("Importing…") : t("Choose File…")}
          </button>
          <p class="secondary small">{t("CurseForge zips need a CurseForge API key to download their mods.")}</p>
        </div>
      {:else if needsKey}
        <div class="message">
          <p>{error}</p>
          <p class="secondary small">
            {t("CurseForge only serves launchers that have a key approved for third-party apps. Apply for one, then paste it in Settings. Modrinth works without a key.")}
          </p>
          <div class="message-actions">
            <button class="button" onclick={() => openUrl(CURSEFORGE_KEY_FORM)}>{t("Apply for a Key")}</button>
            <button class="button default" onclick={() => api.openSettings()}>{t("Open Settings")}</button>
          </div>
        </div>
      {:else if hits.length}
        <ul>
          {#each hits as hit (hit.source + hit.id)}
            {@const key = `${hit.source}:${hit.id}`}
            <li>
              {#if hit.iconUrl}
                <img src={hit.iconUrl} alt="" width="48" height="48" loading="lazy" />
              {:else}
                <span class="no-icon" aria-hidden="true"></span>
              {/if}
              <div class="text">
                <button class="title" onclick={() => openUrl(hit.pageUrl)}>{hit.title}</button>
                <span class="secondary byline">{t("by {author} · {downloads} downloads", { author: hit.author, downloads: compact.format(hit.downloads) })}</span>
                <span class="description">{hit.description}</span>
              </div>
              <button class="button install" disabled={busy} onclick={() => run(key, () => api.installModpack(hit.source, hit.id))}>
                {importing === key ? t("Importing…") : t("Install")}
              </button>
            </li>
          {/each}
        </ul>
        {#if hits.length < total}
          <button class="button more" disabled={loading || busy} onclick={() => tab !== "file" && search(tab, query, hits.length)}>
            {loading ? t("Loading…") : t("Show More")}
          </button>
        {/if}
      {:else if loading}
        <p class="message secondary">{t("Searching…")}</p>
      {:else if !error}
        <p class="message secondary">{query ? t("No modpacks match “{query}”.", { query }) : t("No modpacks found.")}</p>
      {/if}
    </div>

    <div class="footer">
      <div class="status" role="status" aria-live="polite">
        {#if busy}
          <span class="secondary">{progressText}</span>
          {#if progress?.total}
            <progress max={progress.total} value={progress.done} aria-label={t("Import progress")}></progress>
          {/if}
        {:else if error && !needsKey}
          <span class="error">{error}</span>
        {/if}
      </div>
      <button class="button" disabled={busy} onclick={() => (launcher.importOpen = false)}>{t("Cancel")}</button>
    </div>
  {/if}
</dialog>

<style>
  dialog.import {
    width: min(680px, calc(100vw - 32px));
  }

  header p {
    margin: 0;
  }

  .controls {
    display: flex;
    gap: 10px;
    margin: 14px 0 10px;
  }

  .segmented {
    display: inline-flex;
    flex: none;
    padding: 2px;
    border-radius: var(--radius-control);
    background: var(--bg-control);
  }

  .segmented button {
    padding: 3px 12px;
    border: none;
    border-radius: 5px;
    background: transparent;
    font-weight: 500;
  }

  .segmented button[aria-checked="true"] {
    background: var(--bg-segment-selected);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
  }

  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    border: 1px solid var(--separator);
    border-radius: var(--radius-control);
    background: var(--bg-window);
    color: var(--text-secondary);
  }

  .search:focus-within {
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }

  .search input {
    flex: 1;
    min-width: 0;
    height: 24px;
    border: none;
    background: none;
    outline: none;
    color: var(--text);
  }

  .results {
    height: min(400px, 52vh);
    overflow-y: auto;
    border-radius: var(--radius-panel);
    background: var(--bg-control);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
  }

  li + li {
    border-top: 1px solid var(--separator);
  }

  img,
  .no-icon {
    flex: none;
    width: 48px;
    height: 48px;
    border-radius: 9px;
    object-fit: cover;
    background: var(--bg-control-hover);
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .title {
    align-self: flex-start;
    max-width: 100%;
    padding: 0;
    border: none;
    background: none;
    font-weight: 600;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title:hover {
    text-decoration: underline;
  }

  .byline {
    font-size: 11px;
  }

  .description {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font-size: 12px;
    color: var(--text-secondary);
  }

  .install {
    flex: none;
    min-width: 92px;
  }

  .more {
    display: block;
    margin: 10px auto 12px;
  }

  .message {
    margin: 0;
    padding: 32px 24px;
    text-align: center;
  }

  .message p {
    margin: 0 auto 14px;
    max-width: 440px;
  }

  .message .small {
    margin-top: 14px;
    font-size: 12px;
  }

  .message-actions {
    display: flex;
    justify-content: center;
    gap: 8px;
  }

  .footer {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
  }

  .status {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  progress {
    width: 100%;
    max-width: 320px;
    height: 4px;
    accent-color: var(--accent);
  }

  .error {
    color: var(--danger);
  }

  .blocked {
    margin-top: 14px;
    padding: 14px 16px;
    border-radius: var(--radius-panel);
    background: var(--bg-control);
  }

  .blocked p {
    margin: 0 0 10px;
  }

  .blocked li {
    padding: 4px 0;
    gap: 8px;
  }

  .blocked li + li {
    border-top: none;
  }

  .link {
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    text-align: left;
  }

  .link:hover {
    text-decoration: underline;
  }
</style>
