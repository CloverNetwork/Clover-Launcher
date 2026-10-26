<!-- The instance's worlds: their data packs, backups, and moving one to the Trash. -->
<script lang="ts">
  import { Menu } from "@tauri-apps/api/menu";
  import { api, errorText, type Instance, type World } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import { DELETE_INSTANCE, SHOW_FOLDER } from "$lib/platform";
  import BlockIcon from "./BlockIcon.svelte";
  import Icon from "./Icon.svelte";

  let { instance }: { instance: Instance } = $props();

  let worlds = $state<World[]>([]);
  let loaded = $state(false);
  let error = $state<string | null>(null);
  let status = $state<string | null>(null);
  let backingUp = $state<string | null>(null);
  const running = $derived(launcher.activity[instance.id] === "running");

  // Reload when the game stops, since playing changes worlds.
  $effect(() => {
    const [id] = [instance.id, running];
    load(id);
  });

  async function load(id = instance.id) {
    try {
      const list = await api.listWorlds(id);
      if (id !== instance.id) return;
      worlds = list;
      loaded = true;
    } catch (e) {
      error = errorText(e);
    }
  }

  const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });
  const fileStamp = () => {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}.${pad(d.getMinutes())}.${pad(d.getSeconds())}`;
  };

  async function backup(world: World) {
    error = status = null;
    backingUp = world.folder;
    try {
      await api.backupWorld(instance.id, world.folder, fileStamp());
      status = `Backed up ${world.name}.`;
    } catch (e) {
      error = errorText(e);
    } finally {
      backingUp = null;
    }
  }

  async function trash(world: World) {
    error = status = null;
    try {
      await api.trashWorld(instance.id, world.folder);
      worlds = worlds.filter((w) => w.folder !== world.folder);
    } catch (e) {
      error = errorText(e);
    }
  }

  async function showMenu(e: MouseEvent, world: World) {
    e.preventDefault();
    const menu = await Menu.new({
      items: [
        { id: "datapacks", text: "Data Packs…", action: () => (launcher.dataPacksWorld = world) },
        { id: "backup", text: "Back Up", enabled: !backingUp, action: () => backup(world) },
        { id: "show", text: SHOW_FOLDER, action: () => api.showWorld(instance.id, world.folder) },
        { item: "Separator" },
        { id: "trash", text: DELETE_INSTANCE, enabled: !running, action: () => trash(world) },
      ],
    });
    await menu.popup();
  }
</script>

<section class="worlds" aria-labelledby="worlds-title">
  <header>
    <h2 id="worlds-title">Worlds</h2>
    {#if worlds.length}
      <span class="secondary count">{worlds.length}</span>
    {/if}
  </header>

  {#if backingUp || status}
    <p class="note secondary" role="status">{backingUp ? "Backing up…" : status}</p>
  {/if}
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  {#if worlds.length}
    <ul>
      {#each worlds as world (world.folder)}
        <li oncontextmenu={(e) => showMenu(e, world)}>
          {#if world.icon}
            <img src={world.icon} alt="" width="40" height="40" />
          {:else}
            <BlockIcon seed={world.folder} size={40} />
          {/if}
          <div class="text">
            <span class="title">{world.name}</span>
            <span class="secondary detail">
              {world.mode}{world.version ? ` · ${world.version}` : ""}{world.lastPlayed
                ? ` · Played ${dateFormat.format(world.lastPlayed * 1000)}`
                : ""}
            </span>
          </div>
          <button class="button small" onclick={() => (launcher.dataPacksWorld = world)}>Data Packs…</button>
          <button class="icon-button" aria-label="More for {world.name}" title="More" onclick={(e) => showMenu(e, world)}>
            <Icon name="more" />
          </button>
        </li>
      {/each}
    </ul>
  {:else if loaded}
    <p class="empty secondary">No worlds yet. Worlds you create in the game show up here.</p>
  {/if}
</section>

<style>
  .worlds {
    max-width: 640px;
    margin-top: 20px;
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

  img {
    flex: none;
    border-radius: 6px;
    image-rendering: pixelated;
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

  .small {
    flex: none;
    min-height: 22px;
    padding: 2px 10px;
    font-size: 12px;
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
