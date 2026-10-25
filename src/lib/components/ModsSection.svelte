<!-- The instance page's list of mods: turn them on and off, remove them, add more. -->
<script lang="ts">
  import { Menu } from "@tauri-apps/api/menu";
  import { api, errorText, type Instance, type InstalledMod } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import BlockIcon from "./BlockIcon.svelte";
  import Icon from "./Icon.svelte";

  let { instance }: { instance: Instance } = $props();

  let mods = $state<InstalledMod[]>([]);
  let error = $state<string | null>(null);
  const running = $derived(launcher.activity[instance.id] === "running");
  const enabledCount = $derived(mods.filter((m) => m.enabled).length);

  $effect(() => {
    const [id] = [instance.id, launcher.modsRevision];
    api
      .listMods(id)
      .then((list) => (mods = list))
      .catch((e) => (error = errorText(e)));
  });

  async function toggle(mod: InstalledMod) {
    error = null;
    try {
      await api.setModEnabled(instance.id, mod.file, !mod.enabled);
      mod.enabled = !mod.enabled;
    } catch (e) {
      error = errorText(e);
    }
  }

  async function remove(mod: InstalledMod) {
    error = null;
    try {
      await api.removeMod(instance.id, mod.file);
      mods = mods.filter((m) => m.file !== mod.file);
    } catch (e) {
      error = errorText(e);
    }
  }

  async function showMenu(e: MouseEvent, mod: InstalledMod) {
    e.preventDefault();
    const menu = await Menu.new({
      items: [
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

<section class="mods" aria-labelledby="mods-title">
  <header>
    <h2 id="mods-title">Mods</h2>
    {#if mods.length}
      <span class="secondary count">{enabledCount} of {mods.length} on</span>
    {/if}
    <div class="spacer"></div>
    <button class="icon-button" aria-label="Show Mods Folder" title="Show Mods Folder" onclick={() => api.showModsFolder(instance.id)}>
      <Icon name="folder" />
    </button>
    <button class="button" onclick={() => (launcher.modsOpen = true)}>Add Mods…</button>
  </header>

  {#if running && mods.length}
    <p class="note secondary">Changes take effect the next time you play.</p>
  {/if}
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  {#if mods.length}
    <ul>
      {#each mods as mod (mod.file)}
        <li class:off={!mod.enabled} oncontextmenu={(e) => showMenu(e, mod)}>
          {#if mod.iconUrl}
            <img src={mod.iconUrl} alt="" width="32" height="32" loading="lazy" />
          {:else}
            <BlockIcon seed={mod.file} size={32} />
          {/if}
          <div class="text">
            <span class="title">{mod.title}</span>
            <span class="secondary detail">
              {mod.version ?? mod.file} · {sourceName(mod)}{mod.dependency ? " · Required by another mod" : ""}
            </span>
          </div>
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
    <p class="empty secondary">No mods yet. Add some from Modrinth or CurseForge.</p>
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
