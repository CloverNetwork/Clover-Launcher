<script lang="ts">
  import { onMount } from "svelte";
  import { events } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import AccountSheet from "$lib/components/AccountSheet.svelte";
  import CloverLogo from "$lib/components/CloverLogo.svelte";
  import GameLog from "$lib/components/GameLog.svelte";
  import Icon from "$lib/components/Icon.svelte";
  import InstanceDetail from "$lib/components/InstanceDetail.svelte";
  import NewInstanceSheet from "$lib/components/NewInstanceSheet.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";

  const menuActions: Record<string, () => void> = {
    new_instance: () => (launcher.newInstanceOpen = true),
    play: () => launcher.play(),
    stop: () => launcher.stop(),
    show_folder: () => launcher.showFolder(),
    delete_instance: () => launcher.remove(),
    toggle_sidebar: () => (launcher.sidebarVisible = !launcher.sidebarVisible),
    toggle_log: () => (launcher.logVisible = !launcher.logVisible),
    account: () => (launcher.accountOpen = true),
  };

  onMount(() => {
    document.body.classList.add("vibrant");
    launcher.init();
    const unlisten = events.menu((id) => menuActions[id]?.());

    // Collapse the sidebar when the window gets narrow, and bring it back when it widens.
    const narrow = matchMedia("(max-width: 700px)");
    const onResize = () => (launcher.sidebarVisible = !narrow.matches);
    narrow.addEventListener("change", onResize);
    onResize();

    return () => {
      unlisten.then((fn) => fn());
      narrow.removeEventListener("change", onResize);
    };
  });
</script>

<div class="window">
  {#if launcher.sidebarVisible}
    <Sidebar />
  {/if}

  <main class:sidebar-hidden={!launcher.sidebarVisible}>
    <header class="toolbar" data-tauri-drag-region>
      {#if !launcher.sidebarVisible}
        <button
          class="icon-button"
          aria-label="Show Sidebar"
          title="Show Sidebar"
          onclick={() => (launcher.sidebarVisible = true)}
        >
          <Icon name="sidebar" />
        </button>
      {/if}
      <div class="spacer" data-tauri-drag-region></div>
      {#if launcher.selected}
        <button
          class="icon-button"
          aria-label="Game Log"
          title="Game Log"
          aria-pressed={launcher.logVisible}
          onclick={() => (launcher.logVisible = !launcher.logVisible)}
        >
          <Icon name="log" />
        </button>
      {/if}
      <button class="account" onclick={() => (launcher.accountOpen = true)}>
        <Icon name="person" />
        {launcher.account?.name ?? "Sign In"}
      </button>
    </header>

    {#if launcher.selected}
      <InstanceDetail instance={launcher.selected} />
      {#if launcher.logVisible}
        <GameLog lines={launcher.logs[launcher.selected.id] ?? []} />
      {/if}
    {:else}
      <div class="empty">
        <span class="logo"><CloverLogo size={56} /></span>
        <h1>Welcome to Clover</h1>
        <p class="secondary">Create an instance to pick a Minecraft version and start playing.</p>
        <button class="button default" onclick={() => (launcher.newInstanceOpen = true)}>New Instance</button>
      </div>
    {/if}
  </main>
</div>

<NewInstanceSheet />
<AccountSheet />

<style>
  .window {
    display: flex;
    height: 100vh;
  }

  main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-window);
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    height: var(--toolbar-height);
    flex: none;
    padding: 0 12px;
  }

  main.sidebar-hidden .toolbar {
    padding-left: var(--traffic-lights);
  }

  .spacer {
    flex: 1;
    align-self: stretch;
  }

  .account {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font-weight: 500;
  }

  .account:hover {
    background: var(--bg-control-hover);
    color: var(--text);
  }

  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 32px;
    text-align: center;
  }

  .empty .logo {
    display: flex;
    margin-bottom: 8px;
    color: var(--play);
  }

  .empty h1 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }

  .empty p {
    margin: 0 0 8px;
    max-width: 320px;
  }
</style>
