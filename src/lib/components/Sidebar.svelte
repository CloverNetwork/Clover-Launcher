<script lang="ts">
  import { Menu } from "@tauri-apps/api/menu";
  import { launcher } from "$lib/launcher.svelte";
  import BlockIcon from "./BlockIcon.svelte";
  import Icon from "./Icon.svelte";

  let list: HTMLUListElement | undefined = $state();

  function onKeydown(e: KeyboardEvent) {
    const ids = launcher.instances.map((i) => i.id);
    const index = ids.indexOf(launcher.selectedId ?? "");
    if (e.key === "ArrowDown" && index < ids.length - 1) launcher.selectedId = ids[index + 1];
    else if (e.key === "ArrowUp" && index > 0) launcher.selectedId = ids[index - 1];
    else return;
    e.preventDefault();
    list?.querySelector<HTMLElement>(`[data-id="${launcher.selectedId}"]`)?.scrollIntoView({ block: "nearest" });
  }

  async function showContextMenu(e: MouseEvent, id: string) {
    e.preventDefault();
    launcher.selectedId = id;
    const running = launcher.activity[id] === "running";
    const menu = await Menu.new({
      items: [
        running
          ? { id: "stop", text: "Stop", action: () => launcher.stop(id) }
          : { id: "play", text: "Play", enabled: !launcher.activity[id], action: () => launcher.play(id) },
        { item: "Separator" },
        { id: "show", text: "Show in Finder", action: () => launcher.showFolder(id) },
        { item: "Separator" },
        { id: "delete", text: "Delete Instance…", enabled: !running, action: () => launcher.remove(id) },
      ],
    });
    await menu.popup();
  }
</script>

<nav class="sidebar" aria-label="Instances">
  <div class="titlebar" data-tauri-drag-region>
    <button
      class="icon-button"
      aria-label="Hide Sidebar"
      title="Hide Sidebar"
      onclick={() => (launcher.sidebarVisible = false)}
    >
      <Icon name="sidebar" />
    </button>
  </div>

  <div class="section-header">
    <h2 id="instances-heading">Instances</h2>
    <button
      class="icon-button small"
      aria-label="New Instance"
      title="New Instance"
      onclick={() => (launcher.newInstanceOpen = true)}
    >
      <Icon name="plus" size={14} />
    </button>
  </div>

  {#if launcher.instances.length}
    <ul
      bind:this={list}
      role="listbox"
      tabindex="0"
      aria-labelledby="instances-heading"
      aria-activedescendant={launcher.selectedId ? `row-${launcher.selectedId}` : undefined}
      onkeydown={onKeydown}
    >
      {#each launcher.instances as instance (instance.id)}
        {@const activity = launcher.activity[instance.id]}
        <!-- Keyboard selection is handled by the listbox via aria-activedescendant. -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li
          id="row-{instance.id}"
          data-id={instance.id}
          role="option"
          aria-selected={instance.id === launcher.selectedId}
          onclick={() => (launcher.selectedId = instance.id)}
          ondblclick={() => launcher.play(instance.id)}
          oncontextmenu={(e) => showContextMenu(e, instance.id)}
        >
          <BlockIcon seed={instance.id} size={18} />
          <span class="name">{instance.name}</span>
          {#if activity === "running"}
            <span class="status" title="Playing"><Icon name="play" size={10} /><span class="visually-hidden">Playing</span></span>
          {:else}
            <span class="version">{instance.version}</span>
          {/if}
        </li>
      {/each}
    </ul>
  {:else}
    <p class="empty secondary">No instances</p>
  {/if}
</nav>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: 220px;
    flex: none;
    background: var(--bg-sidebar);
    border-right: 1px solid var(--separator);
  }

  .titlebar {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    height: var(--toolbar-height);
    padding: 0 10px 0 var(--traffic-lights);
  }

  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 10px 2px 18px;
  }

  h2 {
    margin: 0;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .icon-button.small {
    width: 22px;
    height: 22px;
  }

  ul {
    flex: 1;
    margin: 0;
    padding: 2px 10px 10px;
    list-style: none;
    overflow-y: auto;
    border-radius: var(--radius-control);
  }

  ul:focus-visible {
    box-shadow: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 8px;
    border-radius: var(--radius-control);
  }

  li[aria-selected="true"] {
    background: var(--bg-row-selected);
  }

  /* Selection takes the accent color while the list has keyboard focus, as in Finder. */
  ul:focus li[aria-selected="true"] {
    background: var(--accent);
    color: var(--on-accent);
  }

  ul:focus li[aria-selected="true"] .version,
  ul:focus li[aria-selected="true"] .status {
    color: inherit;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .version {
    font-size: 11px;
    color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }

  .status {
    display: inline-flex;
    color: var(--play);
  }

  .empty {
    padding: 4px 18px;
    margin: 0;
  }
</style>
