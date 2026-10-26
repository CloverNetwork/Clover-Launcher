<!-- The instance's screenshots, newest first. Opening one uses the system's image viewer. -->
<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { Menu } from "@tauri-apps/api/menu";
  import { api, errorText, type Instance, type Screenshot } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import { locale, t } from "$lib/i18n";
  import { DELETE_INSTANCE, SHOW_FOLDER } from "$lib/platform";

  let { instance }: { instance: Instance } = $props();

  let shots = $state<Screenshot[]>([]);
  let loaded = $state(false);
  let error = $state<string | null>(null);
  /** Shown a page at a time, so a folder of thousands stays quick. */
  let shown = $state(48);
  const running = $derived(launcher.activity[instance.id] === "running");

  // Reload when the game stops, since that's when new screenshots appear.
  $effect(() => {
    const [id] = [instance.id, running];
    api
      .listScreenshots(id)
      .then((list) => {
        if (id !== instance.id) return;
        shots = list;
        loaded = true;
      })
      .catch((e) => (error = errorText(e)));
  });

  const dateFormat = new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" });

  async function open(shot: Screenshot, reveal = false) {
    error = null;
    try {
      await api.openScreenshot(instance.id, shot.file, reveal);
    } catch (e) {
      error = errorText(e);
    }
  }

  async function trash(shot: Screenshot) {
    error = null;
    try {
      await api.trashScreenshot(instance.id, shot.file);
      shots = shots.filter((s) => s.file !== shot.file);
    } catch (e) {
      error = errorText(e);
    }
  }

  async function showMenu(e: MouseEvent, shot: Screenshot) {
    e.preventDefault();
    const menu = await Menu.new({
      items: [
        { id: "open", text: t("Open"), action: () => open(shot) },
        { id: "show", text: SHOW_FOLDER, action: () => open(shot, true) },
        { item: "Separator" },
        { id: "trash", text: DELETE_INSTANCE, action: () => trash(shot) },
      ],
    });
    await menu.popup();
  }
</script>

<section class="screenshots" aria-labelledby="screenshots-title">
  <header>
    <h2 id="screenshots-title">{t("Screenshots")}</h2>
    {#if shots.length}
      <span class="secondary count">{shots.length.toLocaleString(locale)}</span>
    {/if}
  </header>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  {#if shots.length}
    <ul>
      {#each shots.slice(0, shown) as shot (shot.file)}
        <li>
          <button
            class="shot"
            title="{shot.file} · {dateFormat.format(shot.taken * 1000)}"
            aria-label={t("Screenshot from {date}", { date: dateFormat.format(shot.taken * 1000) })}
            onclick={() => open(shot)}
            oncontextmenu={(e) => showMenu(e, shot)}
          >
            <img src={convertFileSrc(shot.path)} alt="" loading="lazy" decoding="async" />
          </button>
        </li>
      {/each}
    </ul>
    {#if shots.length > shown}
      <button class="button more" onclick={() => (shown += 48)}>{t("Show More")}</button>
    {/if}
  {:else if loaded}
    <p class="empty secondary">{t("No screenshots yet. Press F2 in the game to take one.")}</p>
  {/if}
</section>

<style>
  .screenshots {
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
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .shot {
    display: block;
    width: 100%;
    aspect-ratio: 16 / 9;
    padding: 0;
    overflow: hidden;
    border: none;
    border-radius: 6px;
    background: var(--bg-control);
  }

  .shot:focus-visible {
    box-shadow: var(--focus-ring);
  }

  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .more {
    display: block;
    margin: 12px auto 0;
  }

  .empty,
  .error {
    margin: 0 0 8px;
  }

  .error {
    color: var(--danger);
  }
</style>
