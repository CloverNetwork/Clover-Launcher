<script lang="ts">
  import type { Instance } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import BlockIcon from "./BlockIcon.svelte";
  import Icon from "./Icon.svelte";
  import ContentSection from "./ContentSection.svelte";
  import ScreenshotsSection from "./ScreenshotsSection.svelte";
  import ServersSection from "./ServersSection.svelte";
  import WorldsSection from "./WorldsSection.svelte";
  import { SHOW_FOLDER } from "$lib/platform";

  let { instance }: { instance: Instance } = $props();

  type Tab = "mods" | "resourcepacks" | "shaders" | "worlds" | "servers" | "screenshots";
  const modded = $derived(instance.loader !== "vanilla" && instance.loader !== "custom");
  // Shader packs need Iris or OptiFine, which are mods.
  const tabs = $derived<{ id: Tab; label: string }[]>([
    ...(modded ? [{ id: "mods" as const, label: "Mods" }] : []),
    { id: "resourcepacks", label: "Resource Packs" },
    ...(modded ? [{ id: "shaders" as const, label: "Shaders" }] : []),
    { id: "worlds", label: "Worlds" },
    { id: "servers", label: "Servers" },
    { id: "screenshots", label: "Screenshots" },
  ]);
  let chosen = $state<Tab>("mods");
  const tab = $derived(tabs.some((t) => t.id === chosen) ? chosen : tabs[0].id);

  /** Arrow keys move between tabs, as in a native segmented control. */
  function onTabKey(e: KeyboardEvent) {
    const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const index = tabs.findIndex((t) => t.id === tab);
    chosen = tabs[(index + step + tabs.length) % tabs.length].id;
    (e.currentTarget as HTMLElement).querySelector<HTMLElement>(`[data-tab="${chosen}"]`)?.focus();
  }

  const activity = $derived(launcher.activity[instance.id]);
  const progress = $derived(launcher.progress[instance.id]);
  const error = $derived(launcher.error?.instanceId === instance.id ? launcher.error.message : null);
  const notice = $derived(launcher.notice?.instanceId === instance.id ? launcher.notice.message : null);
  const crash = $derived(launcher.crash?.instanceId === instance.id ? launcher.crash : null);
  const busy = $derived(activity === "launching" || activity === "verifying");

  const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });
  const formatDate = (seconds: number | null) => (seconds ? dateFormat.format(seconds * 1000) : "Never");

  /** "3 hr 20 min", "45 min", or "Less than a minute". */
  function formatDuration(seconds: number) {
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    if (!hours && !minutes) return seconds ? "Less than a minute" : "None yet";
    const unit = (value: number, style: "hour" | "minute") =>
      new Intl.NumberFormat(undefined, { style: "unit", unit: style, unitDisplay: "short" }).format(value);
    return [hours ? unit(hours, "hour") : "", minutes ? unit(minutes, "minute") : ""].filter(Boolean).join(" ");
  }

  const status = $derived.by(() => {
    if (activity === "running") return "Playing";
    if (busy && progress && progress.total > 0 && progress.done < progress.total)
      return `Downloading ${progress.total.toLocaleString()} files: ${progress.done.toLocaleString()} done`;
    if (busy && progress?.stage) return progress.stage;
    if (activity === "launching") return "Starting Minecraft…";
    if (activity === "verifying") return "Verifying game files…";
    return notice;
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

      {#if busy && progress && progress.total > 0}
        <progress max={progress.total} value={progress.done} aria-label="Download progress"></progress>
      {/if}
    </div>
  </header>

  {#if error}
    <div class="error" role="alert">
      <div class="error-row">
        <Icon name="warning" />
        <p>{error}</p>
        <button
          class="button"
          onclick={() => {
            launcher.error = null;
            launcher.crash = null;
          }}>Dismiss</button
        >
      </div>
      {#if crash}
        <ul class="findings" aria-label="Likely causes">
          {#each crash.findings as finding, index (index)}
            {@const done = crash.done[index]}
            <li>
              <div class="finding-text">
                <span class="finding-title">{finding.title}</span>
                <span class="secondary">{finding.detail}</span>
              </div>
              {#if finding.action}
                <button class="button" class:done={done && done !== "Working…"} disabled={!!done} onclick={() => launcher.applyCrashFix(index)}>
                  {done ?? finding.action}
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}

  <dl class="facts">
    <div><dt>Last Played</dt><dd>{formatDate(instance.lastPlayed)}</dd></div>
    <div><dt>Time Played</dt><dd>{formatDuration(instance.playSeconds ?? 0)}</dd></div>
    <div><dt>Created</dt><dd>{formatDate(instance.createdAt)}</dd></div>
    <div>
      <dt>Game Folder</dt>
      <dd><button class="link" onclick={() => launcher.showFolder(instance.id)}>{SHOW_FOLDER}</button></dd>
    </div>
  </dl>

  <div class="tabs" role="tablist" aria-label="Instance contents" tabindex="-1" onkeydown={onTabKey}>
    {#each tabs as t (t.id)}
      <button
        role="tab"
        id="tab-{t.id}"
        data-tab={t.id}
        aria-selected={tab === t.id}
        aria-controls="tab-panel"
        tabindex={tab === t.id ? 0 : -1}
        onclick={() => (chosen = t.id)}
      >
        {t.label}
      </button>
    {/each}
  </div>

  <div id="tab-panel" role="tabpanel" aria-labelledby="tab-{tab}">
    {#if tab === "mods"}
      <ContentSection {instance} place={{ kind: "mod" }} />
    {:else if tab === "resourcepacks"}
      <ContentSection {instance} place={{ kind: "resourcepack" }} />
    {:else if tab === "shaders"}
      <ContentSection {instance} place={{ kind: "shader" }} />
    {:else if tab === "worlds"}
      <WorldsSection {instance} />
    {:else if tab === "servers"}
      <ServersSection {instance} />
    {:else}
      <ScreenshotsSection {instance} />
    {/if}
  </div>
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
    max-width: 640px;
    margin-top: 20px;
    padding: 10px 12px;
    border-radius: var(--radius-panel);
    background: color-mix(in srgb, var(--danger) 10%, transparent);
    color: var(--text);
  }

  .error-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .findings {
    margin: 10px 0 0 26px;
    padding: 0;
    list-style: none;
  }

  .findings li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 0;
    border-top: 1px solid var(--separator);
  }

  .finding-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .finding-title {
    font-weight: 600;
  }

  .finding-text .secondary {
    font-size: 12px;
  }

  .findings .button {
    flex: none;
  }

  .findings .button.done:disabled {
    opacity: 1;
    color: var(--text-secondary);
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

  .tabs {
    display: flex;
    width: fit-content;
    max-width: 640px;
    margin-top: 28px;
    padding: 2px;
    border-radius: var(--radius-control);
    background: var(--bg-control);
    overflow-x: auto;
  }

  .tabs:focus {
    outline: none;
  }

  .tabs button {
    flex: none;
    padding: 3px 12px;
    border: none;
    border-radius: 5px;
    background: transparent;
    font-weight: 500;
    white-space: nowrap;
  }

  .tabs button[aria-selected="true"] {
    background: var(--bg-segment-selected);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
  }

  #tab-panel > :global(section) {
    margin-top: 16px;
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
