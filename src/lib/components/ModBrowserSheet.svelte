<!--
  Search Modrinth or CurseForge for mods that fit the instance's game version and loader, or
  for resource, shader and data packs for its game version.
-->
<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, CONTENT, CURSEFORGE_KEY_FORM, errorText, type ModHit, type ModSource, type Place } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import Icon from "./Icon.svelte";

  let dialog: HTMLDialogElement | undefined = $state();
  let source = $state<ModSource>("modrinth");
  let query = $state("");
  let hits = $state<ModHit[]>([]);
  let total = $state(0);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  /** Keyed by "source:id": "installing" while it runs, "installed" after. */
  let installs = $state<Record<string, "installing" | "installed">>({});

  const instance = $derived(launcher.selected);
  const place = $derived(launcher.browse);
  const names = $derived(CONTENT[place?.kind ?? "mod"]);
  const needsKey = $derived(source === "curseforge" && error?.includes("API key"));

  $effect(() => {
    if (launcher.browse) {
      dialog?.showModal();
      installs = {};
      markInstalled();
    } else {
      dialog?.close();
    }
  });

  // Search as the person types, a moment after they pause.
  $effect(() => {
    if (!place || !instance) return;
    const [id, where, from, text] = [instance.id, place, source, query];
    const timer = setTimeout(() => run(id, where, from, text, 0), text ? 300 : 0);
    return () => clearTimeout(timer);
  });

  async function run(id: string, where: Place, from: ModSource, text: string, offset: number) {
    loading = true;
    error = null;
    try {
      const page = await api.searchMods(id, where, from, text, offset);
      if (from !== source || text !== query) return; // a newer search replaced this one
      hits = offset ? [...hits, ...page.hits] : page.hits;
      total = page.total;
    } catch (e) {
      if (!offset) hits = [];
      error = errorText(e);
    } finally {
      loading = false;
    }
  }

  async function markInstalled() {
    if (!instance || !place) return;
    const mods = await api.listMods(instance.id, place);
    for (const mod of mods) {
      if (mod.source && mod.projectId) installs[`${mod.source}:${mod.projectId}`] = "installed";
    }
  }

  async function install(hit: ModHit) {
    if (!instance || !place) return;
    const key = `${hit.source}:${hit.id}`;
    installs[key] = "installing";
    error = notice = null;
    try {
      const added = await api.installMod(instance.id, place, hit.source, hit.id);
      installs[key] = "installed";
      const extra = added.slice(1);
      notice = extra.length ? `Also added ${extra.join(", ")}, which ${hit.title} needs.` : null;
      launcher.modsRevision++;
      await markInstalled();
    } catch (e) {
      delete installs[key];
      error = errorText(e);
    }
  }

  const compact = new Intl.NumberFormat(undefined, { notation: "compact" });
</script>

<dialog class="sheet browser" bind:this={dialog} aria-labelledby="mods-browser-title" onclose={() => (launcher.browse = null)}>
  <header>
    <h2 id="mods-browser-title">Add {names.title}</h2>
    {#if instance}
      <p class="secondary">
        {instance.name} · Minecraft {instance.version}{place?.kind === "mod" ? ` · ${launcher.loaderLabel(instance)}` : ""}{place?.world
          ? ` · ${place.world}`
          : ""}
      </p>
    {/if}
  </header>

  <div class="controls">
    <div class="segmented" role="radiogroup" aria-label="Source">
      <button role="radio" aria-checked={source === "modrinth"} onclick={() => (source = "modrinth")}>Modrinth</button>
      <button role="radio" aria-checked={source === "curseforge"} onclick={() => (source = "curseforge")}>CurseForge</button>
    </div>
    <label class="search">
      <Icon name="search" size={14} />
      <span class="visually-hidden">Search {names.plural}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        type="search"
        bind:value={query}
        placeholder="Search {source === 'modrinth' ? 'Modrinth' : 'CurseForge'}"
        autocomplete="off"
        spellcheck="false"
        autofocus
      />
    </label>
  </div>

  <div class="results" aria-busy={loading}>
    {#if needsKey}
      <div class="message">
        <p>{error}</p>
        <p class="secondary small">
          CurseForge only serves launchers that have a key approved for third-party apps. Apply for one, then paste it
          in Settings. Modrinth works without a key.
        </p>
        <div class="message-actions">
          <button class="button" onclick={() => openUrl(CURSEFORGE_KEY_FORM)}>Apply for a Key</button>
          <button class="button default" onclick={() => api.openSettings()}>Open Settings</button>
        </div>
      </div>
    {:else if hits.length}
      <ul>
        {#each hits as hit (hit.source + hit.id)}
          {@const state = installs[`${hit.source}:${hit.id}`]}
          <li>
            {#if hit.iconUrl}
              <img src={hit.iconUrl} alt="" width="44" height="44" loading="lazy" />
            {:else}
              <span class="no-icon" aria-hidden="true"></span>
            {/if}
            <div class="text">
              <button class="title" onclick={() => openUrl(hit.pageUrl)} title="Open on {hit.source === 'modrinth' ? 'Modrinth' : 'CurseForge'}">
                {hit.title}
              </button>
              <span class="secondary byline">by {hit.author} · {compact.format(hit.downloads)} downloads</span>
              <span class="description">{hit.description}</span>
            </div>
            <button
              class="button install"
              class:done={state === "installed"}
              disabled={!!state}
              onclick={() => install(hit)}
            >
              {state === "installing" ? "Installing…" : state === "installed" ? "Installed" : "Install"}
            </button>
          </li>
        {/each}
      </ul>
      {#if hits.length < total}
        <button class="button more" disabled={loading} onclick={() => instance && place && run(instance.id, place, source, query, hits.length)}>
          {loading ? "Loading…" : "Show More"}
        </button>
      {/if}
    {:else if loading}
      <p class="message secondary">Searching…</p>
    {:else if !error}
      <p class="message secondary">
        No {names.plural} match{query ? ` “${query}”` : ""} for this {place?.kind === "mod" ? "version and loader" : "version"}.
      </p>
    {/if}
  </div>

  <div class="footer">
    <p class="status" role="status" aria-live="polite">
      {#if error && !needsKey}<span class="error">{error}</span>{:else if notice}<span class="secondary">{notice}</span>{/if}
    </p>
    <button class="button default" onclick={() => (launcher.browse = null)}>Done</button>
  </div>
</dialog>

<style>
  dialog.browser {
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
    height: min(420px, 55vh);
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
    width: 44px;
    height: 44px;
    border-radius: 8px;
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
    min-width: 88px;
  }

  .install.done:disabled {
    opacity: 1;
    color: var(--text-secondary);
  }

  .more {
    display: block;
    margin: 10px auto 12px;
  }

  .message {
    margin: 0;
    padding: 24px;
    text-align: center;
  }

  .message p {
    margin: 0 auto 12px;
    max-width: 420px;
  }

  .message .small {
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
    gap: 12px;
    margin-top: 16px;
  }

  .status {
    flex: 1;
    min-width: 0;
    margin: 0;
  }

  .error {
    color: var(--danger);
  }
</style>
