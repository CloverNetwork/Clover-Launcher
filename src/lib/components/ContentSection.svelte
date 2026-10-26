<!--
  One content folder of an instance (mods, resource packs, shader packs, or a world's data
  packs): turn files on and off, update, remove, add more.
-->
<script lang="ts">
  import { Menu } from "@tauri-apps/api/menu";
  import { api, CONTENT, errorText, type Instance, type InstalledMod, type ModUpdate, type Place } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import BlockIcon from "./BlockIcon.svelte";
  import Icon from "./Icon.svelte";

  let { instance, place, subtitle }: { instance: Instance; place: Place; subtitle?: string } = $props();

  let mods = $state<InstalledMod[]>([]);
  let error = $state<string | null>(null);
  let updates = $state<ModUpdate[]>([]);
  let checking = $state(false);
  let updating = $state<Set<string>>(new Set());
  let status = $state<string | null>(null);
  const names = $derived(CONTENT[place.kind]);
  const isMods = $derived(place.kind === "mod");
  const titleId = $derived(`content-title-${place.kind}`);
  // Mods are loaded once at startup; packs can be swapped while playing.
  const running = $derived(isMods && launcher.activity[instance.id] === "running");
  const enabledCount = $derived(mods.filter((m) => m.enabled).length);
  const updateFor = (mod: InstalledMod) => updates.find((u) => u.file === mod.file);

  // Load the list, then ask Modrinth and CurseForge whether anything is newer.
  $effect(() => {
    const [id, where] = [instance.id, { kind: place.kind, world: place.world }, launcher.modsRevision];
    let current = true;
    updates = [];
    mods = [];
    error = status = null;
    api
      .listMods(id, where)
      .then((list) => {
        if (!current) return;
        mods = list;
        if (list.length) checkUpdates(id, where, () => current);
      })
      .catch((e) => (error = errorText(e)));
    return () => (current = false);
  });

  async function checkUpdates(id: string, where: Place, stillCurrent = () => true) {
    checking = true;
    try {
      const check = await api.checkModUpdates(id, where);
      if (!stillCurrent()) return;
      updates = check.updates;
      status = check.skipped;
    } catch (e) {
      // Being offline shouldn't make the mod list look broken.
      if (stillCurrent()) status = `Couldn't check for updates: ${errorText(e)}`;
    } finally {
      checking = false;
    }
  }

  async function applyUpdates(list: ModUpdate[]) {
    error = status = null;
    updating = new Set(list.map((u) => u.file));
    try {
      const changed = await api.updateMods(instance.id, place, list);
      const extra = changed.filter((title) => !list.some((u) => u.title === title));
      status =
        `Updated ${list.length === 1 ? list[0].title : `${list.length} ${names.plural}`}.` +
        (extra.length ? ` Also added ${extra.join(", ")}, which the new versions need.` : "");
      launcher.modsRevision++;
    } catch (e) {
      error = errorText(e);
    } finally {
      updating = new Set();
    }
  }

  async function toggle(mod: InstalledMod) {
    error = null;
    try {
      await api.setModEnabled(instance.id, place, mod.file, !mod.enabled);
      mod.enabled = !mod.enabled;
    } catch (e) {
      error = errorText(e);
    }
  }

  async function remove(mod: InstalledMod) {
    error = null;
    try {
      await api.removeMod(instance.id, place, mod.file);
      mods = mods.filter((m) => m.file !== mod.file);
    } catch (e) {
      error = errorText(e);
    }
  }

  async function showMenu(e: MouseEvent, mod: InstalledMod) {
    e.preventDefault();
    const update = updateFor(mod);
    const menu = await Menu.new({
      items: [
        ...(update
          ? [{ id: "update", text: `Update to ${update.newVersion}`, enabled: !running, action: () => applyUpdates([update]) }]
          : []),
        { id: "toggle", text: mod.enabled ? "Turn Off" : "Turn On", action: () => toggle(mod) },
        { item: "Separator" },
        { id: "remove", text: "Remove", action: () => remove(mod) },
      ],
    });
    await menu.popup();
  }

  const sourceName = (mod: InstalledMod) =>
    mod.source === "modrinth" ? "Modrinth" : mod.source === "curseforge" ? "CurseForge" : "Added by hand";
</script>

<section class="mods" aria-labelledby={titleId}>
  <header>
    <div class="heading">
      <h2 id={titleId}>{names.title}</h2>
      {#if subtitle}<p class="secondary subtitle">{subtitle}</p>{/if}
    </div>
    {#if mods.length}
      <span class="secondary count">{enabledCount} of {mods.length} on</span>
    {/if}
    <div class="spacer"></div>
    {#if updates.length}
      <button class="button" disabled={running || updating.size > 0} onclick={() => applyUpdates(updates)}>
        {updating.size > 1 ? "Updating…" : `Update All (${updates.length})`}
      </button>
    {:else if mods.length}
      <button class="icon-button" aria-label="Check for Updates" title="Check for Updates" disabled={checking} onclick={() => checkUpdates(instance.id, place)}>
        <Icon name="refresh" size={14} />
      </button>
    {/if}
    <button class="icon-button" aria-label="Show {names.title} Folder" title="Show {names.title} Folder" onclick={() => api.showModsFolder(instance.id, place)}>
      <Icon name="folder" />
    </button>
    <button class="button" onclick={() => (launcher.browse = { ...place })}>Add {names.title}…</button>
  </header>

  {#if running && mods.length}
    <p class="note secondary">Changes take effect the next time you play. Quit the game to update mods.</p>
  {/if}
  {#if checking || status}
    <p class="note secondary" role="status">{checking ? "Checking for updates…" : status}</p>
  {/if}
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  {#if mods.length}
    <ul>
      {#each mods as mod (mod.file)}
        {@const update = updateFor(mod)}
        <li class:off={!mod.enabled} oncontextmenu={(e) => showMenu(e, mod)}>
          {#if mod.iconUrl}
            <img src={mod.iconUrl} alt="" width="32" height="32" loading="lazy" />
          {:else}
            <BlockIcon seed={mod.file} size={32} />
          {/if}
          <div class="text">
            <span class="title">{mod.title}</span>
            <span class="secondary detail">
              {#if update}
                <span class="new">{mod.version ?? update.currentVersion ?? mod.file} → {update.newVersion}</span>
              {:else}
                {mod.version ?? mod.file}
              {/if}
              · {sourceName(mod)}{mod.dependency ? " · Required by another mod" : ""}
            </span>
          </div>
          {#if update}
            <button
              class="button update"
              disabled={running || updating.has(mod.file)}
              title="Update to {update.newVersion}"
              onclick={() => applyUpdates([update])}
            >
              {updating.has(mod.file) ? "Updating…" : "Update"}
            </button>
          {/if}
          <label class="switch" title={mod.enabled ? "Turn Off" : "Turn On"}>
            <input type="checkbox" role="switch" checked={mod.enabled} onchange={() => toggle(mod)} aria-label="{mod.title} on" />
            <span aria-hidden="true"></span>
          </label>
          <button class="icon-button remove" aria-label="Remove {mod.title}" title="Remove" onclick={() => remove(mod)}>
            <Icon name="trash" size={14} />
          </button>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="empty secondary">No {names.plural} yet. Add some from Modrinth or CurseForge, or drop files in the folder.</p>
  {/if}
</section>

<style>
  .mods {
    max-width: 640px;
    margin-top: 28px;
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }

  .subtitle {
    margin: 0;
    font-size: 11px;
  }

  .count {
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .spacer {
    flex: 1;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    border-radius: var(--radius-panel);
    background: var(--bg-control);
  }

  li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 8px 8px 12px;
  }

  li + li {
    border-top: 1px solid var(--separator);
  }

  li.off img,
  li.off :global(svg),
  li.off .text {
    opacity: 0.5;
  }

  img {
    flex: none;
    border-radius: 6px;
    object-fit: cover;
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .title,
  .detail {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title {
    font-weight: 500;
  }

  .detail {
    font-size: 11px;
  }

  /* A macOS-style switch built on a real checkbox, so keyboard and screen readers work. */
  .switch {
    position: relative;
    flex: none;
    width: 32px;
    height: 18px;
  }

  .switch input {
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
  }

  .switch span {
    position: absolute;
    inset: 0;
    border-radius: 9px;
    background: var(--bg-control-hover);
    transition: background 150ms;
  }

  .switch span::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #ffffff;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
    transition: transform 150ms;
  }

  .switch input:checked + span {
    background: var(--accent);
  }

  .switch input:checked + span::after {
    transform: translateX(14px);
  }

  .switch input:focus-visible + span {
    box-shadow: var(--focus-ring);
  }

  .new {
    color: var(--accent);
  }

  .update {
    flex: none;
    min-height: 22px;
    padding: 2px 10px;
    font-size: 12px;
  }

  .remove {
    opacity: 0;
  }

  li:hover .remove,
  .remove:focus-visible {
    opacity: 1;
  }

  .note,
  .empty,
  .error {
    margin: 0 0 8px;
  }

  .error {
    color: var(--danger);
  }
</style>
