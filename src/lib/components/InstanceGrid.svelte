<!--
  The grid layout: every instance as a card. Clicking a card opens its page; the Play
  button and a double-click start the game straight from the grid.
-->
<script lang="ts">
  import { launcher } from "$lib/launcher.svelte";
  import BlockIcon from "./BlockIcon.svelte";
  import Icon from "./Icon.svelte";

  function status(id: string) {
    const activity = launcher.activity[id];
    const progress = launcher.progress[id];
    if (activity === "running") return "Playing";
    if (activity === "verifying") return "Verifying…";
    if (activity === "launching" && progress && progress.total > 0 && progress.done < progress.total)
      return `Downloading ${Math.round((progress.done / progress.total) * 100)}%`;
    if (activity === "launching") return "Starting…";
    return null;
  }
</script>

<section class="grid-page" aria-labelledby="grid-title">
  <h1 id="grid-title">Instances</h1>
  <ul class="grid">
    {#each launcher.instances as instance (instance.id)}
      {@const activity = launcher.activity[instance.id]}
      {@const text = status(instance.id)}
      <li
        class="card"
        class:selected={instance.id === launcher.selectedId}
        oncontextmenu={(e) => launcher.showInstanceMenu(e, instance.id)}
      >
        <button
          class="open"
          aria-label="{instance.name}, Minecraft {instance.version}, {launcher.loaderLabel(instance)}"
          onclick={() => launcher.open(instance.id)}
          ondblclick={() => launcher.play(instance.id)}
        >
          <BlockIcon seed={instance.id} size={72} />
          <span class="name">{instance.name}</span>
          <span class="meta secondary">{text ?? `${instance.version} · ${launcher.loaderLabel(instance)}`}</span>
        </button>
        {#if activity === "running"}
          <button class="play stop" onclick={() => launcher.stop(instance.id)}>
            <Icon name="stop" size={10} /> Stop
          </button>
        {:else}
          <button class="play" disabled={!!activity} onclick={() => launcher.play(instance.id)}>
            <Icon name="play" size={10} /> Play
          </button>
        {/if}
      </li>
    {/each}
    <li class="card add">
      <button class="open" onclick={() => (launcher.newInstanceOpen = true)}>
        <span class="plus"><Icon name="plus" size={28} /></span>
        <span class="name">New Instance</span>
      </button>
      <button class="import-link" onclick={() => (launcher.importOpen = true)}>Import Modpack…</button>
    </li>
  </ul>
</section>

<style>
  .grid-page {
    flex: 1;
    overflow-y: auto;
    padding: 4px 28px 28px;
  }

  h1 {
    margin: 0 0 16px;
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 14px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .card {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 0 0 14px;
    border: 2px solid transparent;
    border-radius: 12px;
    background: var(--bg-control);
  }

  .card:hover {
    background: var(--bg-control-hover);
  }

  .card.selected {
    border-color: var(--accent);
  }

  .open {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    width: 100%;
    padding: 18px 12px 10px;
    border: none;
    border-radius: 10px;
    background: none;
    color: inherit;
    text-align: center;
  }

  .open :global(svg) {
    margin-bottom: 9px;
  }

  .name {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }

  .meta {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .play {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 14px;
    border: none;
    border-radius: 7px;
    background: var(--play);
    color: var(--on-play);
    font-weight: 600;
  }

  .play:hover:not(:disabled) {
    background: var(--play-hover);
  }

  .play:disabled {
    opacity: 0.55;
  }

  .play.stop {
    background: var(--bg-window);
    color: var(--text);
  }

  .card.add {
    justify-content: center;
    padding: 0;
    border: 1.5px dashed var(--separator);
    background: transparent;
  }

  .card.add .open {
    flex: 1;
    justify-content: center;
    color: var(--text-secondary);
  }

  .card.add:hover .open {
    color: var(--text);
  }

  .import-link {
    margin-bottom: 12px;
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    font-size: 11px;
  }

  .import-link:hover {
    text-decoration: underline;
  }

  .plus {
    display: grid;
    place-items: center;
    width: 72px;
    height: 72px;
  }
</style>
