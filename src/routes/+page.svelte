<script lang="ts">
  import { onMount } from "svelte";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  import { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
  import { events } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import AccountSheet from "$lib/components/AccountSheet.svelte";
  import CloverLogo from "$lib/components/CloverLogo.svelte";
  import GameLog from "$lib/components/GameLog.svelte";
  import Icon from "$lib/components/Icon.svelte";
  import ImportModpackSheet from "$lib/components/ImportModpackSheet.svelte";
  import InstanceDetail from "$lib/components/InstanceDetail.svelte";
  import InstanceGrid from "$lib/components/InstanceGrid.svelte";
  import NewInstanceSheet from "$lib/components/NewInstanceSheet.svelte";
  import ModBrowserSheet from "$lib/components/ModBrowserSheet.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import WardrobeSheet from "$lib/components/WardrobeSheet.svelte";
  import WindowControls from "$lib/components/WindowControls.svelte";

  const menuActions: Record<string, () => void> = {
    new_instance: () => (launcher.newInstanceOpen = true),
    import_modpack: () => (launcher.importOpen = true),
    play: () => launcher.play(),
    stop: () => launcher.stop(),
    show_folder: () => launcher.showFolder(),
    delete_instance: () => launcher.remove(),
    toggle_sidebar: () => launcher.layout === "sidebar" && (launcher.sidebarVisible = !launcher.sidebarVisible),
    view_sidebar: () => launcher.setLayout("sidebar"),
    view_grid: () => launcher.setLayout("grid"),
    toggle_log: () => (launcher.logVisible = !launcher.logVisible),
    verify_files: () => launcher.verify(),
    add_account: () => (launcher.accountOpen = true),
    skins: () => launcher.account && (launcher.skinsOpen = true),
    sign_out: () => launcher.signOut(),
  };

  function onMenuCommand(id: string) {
    // Game › Account lists each account as "account:<uuid>".
    if (id.startsWith("account:")) launcher.switchAccount(id.slice("account:".length));
    else menuActions[id]?.();
  }

  /** The toolbar account button: a pull-down of accounts, like the menu bar's Game › Account. */
  async function showAccountMenu(e: MouseEvent) {
    if (!launcher.accounts.length) {
      launcher.accountOpen = true;
      return;
    }
    const button = e.currentTarget as HTMLElement;
    const accounts = await Promise.all(
      launcher.accounts.map((account) =>
        CheckMenuItem.new({
          text: account.offline ? `${account.name} (Offline)` : account.name,
          checked: account.uuid === launcher.activeAccountId,
          action: () => launcher.switchAccount(account.uuid),
        }),
      ),
    );
    const menu = await Menu.new({
      items: [
        ...accounts,
        await PredefinedMenuItem.new({ item: "Separator" }),
        await MenuItem.new({ text: "Skins & Capes…", accelerator: "CmdOrCtrl+Shift+K", action: () => (launcher.skinsOpen = true) }),
        await PredefinedMenuItem.new({ item: "Separator" }),
        await MenuItem.new({ text: "Add Account…", action: () => (launcher.accountOpen = true) }),
        await MenuItem.new({ text: `Sign Out ${launcher.account?.name ?? ""}`.trim(), action: () => launcher.signOut() }),
      ],
    });
    const rect = button.getBoundingClientRect();
    await menu.popup(new LogicalPosition(rect.left, rect.bottom + 4));
  }

  const grid = $derived(launcher.layout === "grid");
  const sidebarShown = $derived(!grid && launcher.sidebarVisible);
  /** The instance page is showing: always in the sidebar layout, on demand in the grid. */
  const showingDetail = $derived(!!launcher.selected && (!grid || launcher.detailOpen));

  function onKeydown(e: KeyboardEvent) {
    // Escape leaves an instance's page in the grid layout, unless a sheet has it.
    if (e.key === "Escape" && grid && launcher.detailOpen && !document.querySelector("dialog[open]")) {
      launcher.detailOpen = false;
    }
  }

  onMount(() => {
    document.body.classList.add("vibrant");
    launcher.init();
    const unlisten = events.menu(onMenuCommand);

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

<svelte:window onkeydown={onKeydown} />

<div class="window">
  {#if sidebarShown}
    <Sidebar />
  {/if}

  <main class:sidebar-hidden={!sidebarShown}>
    <header class="toolbar" data-tauri-drag-region>
      {#if !grid && !launcher.sidebarVisible}
        <button
          class="icon-button"
          aria-label="Show Sidebar"
          title="Show Sidebar"
          onclick={() => (launcher.sidebarVisible = true)}
        >
          <Icon name="sidebar" />
        </button>
      {/if}
      {#if grid && launcher.detailOpen}
        <button class="back" onclick={() => (launcher.detailOpen = false)}>
          <Icon name="back" />
          All Instances
        </button>
      {/if}
      <div class="spacer" data-tauri-drag-region></div>
      <div class="view-switch" role="radiogroup" aria-label="View instances as">
        <button
          role="radio"
          aria-checked={!grid}
          aria-label="Sidebar"
          title="View as Sidebar (⌘1)"
          onclick={() => launcher.setLayout("sidebar")}
        >
          <Icon name="sidebar" />
        </button>
        <button role="radio" aria-checked={grid} aria-label="Grid" title="View as Grid (⌘2)" onclick={() => launcher.setLayout("grid")}>
          <Icon name="grid" />
        </button>
      </div>
      {#if showingDetail}
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
      <button
        class="account"
        aria-haspopup={launcher.accounts.length ? "menu" : "dialog"}
        title={launcher.accounts.length ? "Switch Account" : "Sign In"}
        onclick={showAccountMenu}
      >
        <Icon name="person" />
        {launcher.account?.name ?? "Sign In"}
        {#if launcher.accounts.length}
          <svg class="chevron" width="8" height="8" viewBox="0 0 8 8" aria-hidden="true">
            <path d="M1.5 3l2.5 2.5L6.5 3" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
          </svg>
        {/if}
      </button>
      <WindowControls quits />
    </header>

    {#if showingDetail && launcher.selected}
      <InstanceDetail instance={launcher.selected} />
      {#if launcher.logVisible}
        <GameLog
          instanceId={launcher.selected.id}
          lines={launcher.logs[launcher.selected.id] ?? []}
          onPopOut={() => launcher.popOutLog()}
        />
      {/if}
    {:else if grid && launcher.instances.length}
      <InstanceGrid />
    {:else}
      <div class="empty">
        <span class="logo"><CloverLogo size={56} /></span>
        <h1>Welcome to Clover</h1>
        <p class="secondary">Create an instance to pick a Minecraft version and start playing.</p>
        <div class="empty-actions">
          <button class="button" onclick={() => (launcher.importOpen = true)}>Import Modpack…</button>
          <button class="button default" onclick={() => (launcher.newInstanceOpen = true)}>New Instance</button>
        </div>
      </div>
    {/if}
  </main>
</div>

<NewInstanceSheet />
<AccountSheet />
<WardrobeSheet />
<ModBrowserSheet />
<ImportModpackSheet />

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

  .back {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 28px;
    padding: 0 10px 0 4px;
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font-weight: 500;
  }

  .back:hover {
    background: var(--bg-control-hover);
    color: var(--text);
  }

  .view-switch {
    display: inline-flex;
    margin-right: 4px;
    padding: 2px;
    border-radius: var(--radius-control);
    background: var(--bg-control);
  }

  .view-switch button {
    display: grid;
    place-items: center;
    width: 30px;
    height: 22px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--text-secondary);
  }

  .view-switch button[aria-checked="true"] {
    background: var(--bg-segment-selected);
    color: var(--text);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
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

  .chevron {
    margin-left: -1px;
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

  .empty-actions {
    display: flex;
    gap: 8px;
  }

  .empty p {
    margin: 0 0 8px;
    max-width: 320px;
  }
</style>
