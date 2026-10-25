<script lang="ts">
  import type { Instance } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import BlockIcon from "./BlockIcon.svelte";
  import Icon from "./Icon.svelte";

  let { instance }: { instance: Instance } = $props();

  const activity = $derived(launcher.activity[instance.id]);
  const progress = $derived(launcher.progress[instance.id]);
  const error = $derived(launcher.error?.instanceId === instance.id ? launcher.error.message : null);

  const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });
  const formatDate = (seconds: number | null) => (seconds ? dateFormat.format(seconds * 1000) : "Never");

  const status = $derived.by(() => {
    if (activity === "running") return "Playing";
    if (activity === "launching" && progress?.stage) return progress.stage;
    if (activity === "launching" && progress && progress.done < progress.total)
      return `Downloading files: ${progress.done.toLocaleString()} of ${progress.total.toLocaleString()}`;
    if (activity === "launching") return "Starting Minecraft…";
    return null;
  });
</script>

<article class="detail" aria-labelledby="instance-name">
  <header class="hero">
    <BlockIcon seed={instance.id} size={96} />
    <div class="heading">
      <h1 id="instance-name">{instance.name}</h1>
      <p class="secondary">Minecraft {instance.version} · {launcher.loaderLabel(instance)}</p>

      <div class="play-row">
        {#if activity === "running"}
          <button class="play stop" onclick={() => launcher.stop(instance.id)}>
            <Icon name="stop" size={12} /> Stop
          </button>
        {:else}
          <button class="play" disabled={!!activity} onclick={() => launcher.play(instance.id)}>
            <Icon name="play" size={12} /> Play
          </button>
        {/if}
        <span class="status secondary" role="status" aria-live="polite">{status ?? ""}</span>
      </div>

      {#if activity === "launching" && progress && progress.total > 0}
        <progress max={progress.total} value={progress.done} aria-label="Download progress"></progress>
      {/if}
    </div>
  </header>

  {#if error}
    <div class="error" role="alert">
      <Icon name="warning" />
      <p>{error}</p>
      <button class="button" onclick={() => (launcher.error = null)}>Dismiss</button>
    </div>
  {/if}

  <dl class="facts">
    <div><dt>Last Played</dt><dd>{formatDate(instance.lastPlayed)}</dd></div>
    <div><dt>Created</dt><dd>{formatDate(instance.createdAt)}</dd></div>
    <div>
      <dt>Game Folder</dt>
      <dd><button class="link" onclick={() => launcher.showFolder(instance.id)}>Show in Finder</button></dd>
    </div>
  </dl>
</article>

<style>
  .detail {
    flex: 1;
    overflow-y: auto;
    padding: 12px 32px 32px;
  }

  .hero {
    display: flex;
    align-items: center;
    gap: 24px;
    max-width: 640px;
  }

  .heading {
    flex: 1;
    min-width: 0;
  }

  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.01em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .heading > p {
    margin: 2px 0 14px;
  }

  .play-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .play {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 96px;
    height: 30px;
    padding: 0 18px;
    justify-content: center;
    border: none;
    border-radius: 8px;
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
    background: var(--bg-control);
    color: var(--text);
  }

  .play.stop:hover {
    background: var(--bg-control-hover);
  }

  .status {
    font-variant-numeric: tabular-nums;
  }

  progress {
    width: 100%;
    max-width: 320px;
    height: 4px;
    margin-top: 10px;
    accent-color: var(--accent);
  }

  .error {
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 640px;
    margin-top: 20px;
    padding: 10px 12px;
    border-radius: var(--radius-panel);
    background: color-mix(in srgb, var(--danger) 10%, transparent);
    color: var(--text);
  }

  .error :global(svg) {
    flex: none;
    color: var(--danger);
  }

  .error p {
    flex: 1;
    margin: 0;
  }

  .facts {
    max-width: 640px;
    margin: 28px 0 0;
    border-radius: var(--radius-panel);
    background: var(--bg-control);
  }

  .facts div {
    display: flex;
    justify-content: space-between;
    padding: 8px 12px;
  }

  .facts div + div {
    border-top: 1px solid var(--separator);
  }

  dt {
    font-weight: 500;
  }

  dd {
    margin: 0;
    color: var(--text-secondary);
  }

  .link {
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
  }

  .link:hover {
    text-decoration: underline;
  }
</style>
