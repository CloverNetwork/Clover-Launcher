<script lang="ts">
  import { onMount } from "svelte";
  import { LogicalPosition } from "@tauri-apps/api/dpi";
  import { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
  import { api, events } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import AccountSheet from "$lib/components/AccountSheet.svelte";
  import CloverLogo from "$lib/components/CloverLogo.svelte";
  import GameLog from "$lib/components/GameLog.svelte";
  import Icon from "$lib/components/Icon.svelte";
  import DataPacksSheet from "$lib/components/DataPacksSheet.svelte";
  import EditInstanceSheet from "$lib/components/EditInstanceSheet.svelte";
  import ImportModpackSheet from "$lib/components/ImportModpackSheet.svelte";
  import InstanceDetail from "$lib/components/InstanceDetail.svelte";
  import InstanceGrid from "$lib/components/InstanceGrid.svelte";
  import NewInstanceSheet from "$lib/components/NewInstanceSheet.svelte";
  import ModBrowserSheet from "$lib/components/ModBrowserSheet.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import WardrobeSheet from "$lib/components/WardrobeSheet.svelte";
  import WindowControls from "$lib/components/WindowControls.svelte";
  import { commandKey, hasMenuBar, SETTINGS } from "$lib/platform";
  import { updates } from "$lib/updates.svelte";
  import { t } from "$lib/i18n";

  const menuActions: Record<string, () => void> = {
    new_instance: () => (launcher.newInstanceOpen = true),
    import_modpack: () => (launcher.importOpen = true),
    check_updates: () => updates.check(true),
    play: () => launcher.play(),
    stop: () => launcher.stop(),
    show_folder: () => launcher.showFolder(),
    delete_instance: () => launcher.remove(),
    toggle_sidebar: () => launcher.layout === "sidebar" && (launcher.sidebarVisible = !launcher.sidebarVisible),
    view_sidebar: () => launcher.setLayout("sidebar"),
    view_grid: () => launcher.setLayout("grid"),
    toggle_log: () => (launcher.logVisible = !launcher.logVisible),
    verify_files: () => launcher.verify(),
    create_shortcut: () => launcher.createShortcut(),
    edit_instance: () => launcher.edit(),
    duplicate_instance: () => launcher.duplicate(),
    export_instance: () => launcher.exportPack(),
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
          text: account.offline ? t("{name} (Offline)", { name: account.name }) : account.name,
          checked: account.uuid === launcher.activeAccountId,
          action: () => launcher.switchAccount(account.uuid),
        }),
      ),
    );
    const menu = await Menu.new({
      items: [
        ...accounts,
        await PredefinedMenuItem.new({ item: "Separator" }),
        await MenuItem.new({ text: t("Skins & Capes…"), accelerator: "CmdOrCtrl+Shift+K", action: () => (launcher.skinsOpen = true) }),
        await PredefinedMenuItem.new({ item: "Separator" }),
        await MenuItem.new({ text: t("Add Account…"), action: () => (launcher.accountOpen = true) }),
        await MenuItem.new({ text: t("Sign Out {name}", { name: launcher.account?.name ?? "" }).trim(), action: () => launcher.signOut() }),
      ],
    });
    const rect = button.getBoundingClientRect();
    await menu.popup(new LogicalPosition(rect.left, rect.bottom + 4));
  }

  const grid = $derived(launcher.layout === "grid");
  const sidebarShown = $derived(!grid && launcher.sidebarVisible);
  /** The instance page is showing: always in the sidebar layout, on demand in the grid. */
  const showingDetail = $derived(!!launcher.selected && (!grid || launcher.detailOpen));

  /**
   * Windows and Linux have no menu bar to own the shortcuts, so the window does. The keys
   * match the macOS menu, with Ctrl in place of ⌘.
   */
  const shortcuts: { key: string; shift?: boolean; command: string }[] = [
    { key: "n", command: "new_instance" },
    { key: "n", shift: true, command: "import_modpack" },
    { key: "i", command: "edit_instance" },
    { key: "d", command: "duplicate_instance" },
    { key: ",", command: "settings" },
    { key: "r", shift: true, command: "show_folder" },
    { key: "1", command: "view_sidebar" },
    { key: "2", command: "view_grid" },
    { key: "l", command: "toggle_log" },
    { key: "enter", command: "play" },
    { key: ".", command: "stop" },
    { key: "k", shift: true, command: "skins" },
  ];

  function runShortcut(e: KeyboardEvent): boolean {
    if (hasMenuBar || !commandKey(e) || document.querySelector("dialog[open]")) return false;
    const typing = e.target instanceof HTMLElement && e.target.closest("input, textarea, select");
    const key = e.key.toLowerCase();
    const match = shortcuts.find((s) => s.key === key && !!s.shift === e.shiftKey);
    if (!match || (typing && key !== "enter")) return false;
    e.preventDefault();
    if (match.command === "settings") api.openSettings();
    else onMenuCommand(match.command);
    return true;
  }

  function onKeydown(e: KeyboardEvent) {
    if (runShortcut(e)) return;
    // Escape leaves an instance's page in the grid layout, unless a sheet has it.
    if (e.key === "Escape" && grid && launcher.detailOpen && !document.querySelector("dialog[open]")) {
      launcher.detailOpen = false;
    }
  }

  onMount(() => {
    document.body.classList.add("vibrant");
    launcher.init();
    updates.check();
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
          aria-label={t("Show Sidebar")}
          title={t("Show Sidebar")}
          onclick={() => (launcher.sidebarVisible = true)}
        >
          <Icon name="sidebar" />
        </button>
      {/if}
      {#if grid && launcher.detailOpen}
        <button class="back" onclick={() => (launcher.detailOpen = false)}>
          <Icon name="back" />
          {t("All Instances")}
        </button>
      {/if}
      <div class="spacer" data-tauri-drag-region></div>
      <div class="view-switch" role="radiogroup" aria-label={t("View instances as")}>
        <button
          role="radio"
          aria-checked={!grid}
          aria-label={t("Sidebar")}
          title={t("View as Sidebar")}
          onclick={() => launcher.setLayout("sidebar")}
        >
          <Icon name="sidebar" />
        </button>
        <button role="radio" aria-checked={grid} aria-label={t("Grid")} title={t("View as Grid")} onclick={() => launcher.setLayout("grid")}>
          <Icon name="grid" />
        </button>
      </div>
      {#if showingDetail}
        <button
          class="icon-button"
          aria-label={t("Game Log")}
          title={t("Game Log")}
          aria-pressed={launcher.logVisible}
          onclick={() => (launcher.logVisible = !launcher.logVisible)}
        >
          <Icon name="log" />
        </button>
      {/if}
      {#if updates.available}
        <button class="update-pill" onclick={() => updates.offer()} disabled={updates.progress !== null}>
          {updates.progress === null
            ? t("Update to {version}", { version: updates.available.version })
            : t("Updating {percent} %", { percent: Math.round(updates.progress * 100) })}
        </button>
      {/if}
      {#if !hasMenuBar}
        <button class="icon-button" aria-label={SETTINGS} title={SETTINGS} onclick={() => api.openSettings()}>
          <Icon name="gear" />
        </button>
      {/if}
      <button
        class="account"
        aria-haspopup={launcher.accounts.length ? "menu" : "dialog"}
        title={launcher.accounts.length ? t("Switch Account") : t("Sign In")}
        onclick={showAccountMenu}
      >
        <Icon name="person" />
        {launcher.account?.name ?? t("Sign In")}
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
        <h1>{t("Welcome to Clover")}</h1>
        <p class="secondary">{t("Create an instance to pick a Minecraft version and start playing.")}</p>
        <div class="empty-actions">
          <button class="button" onclick={() => (launcher.importOpen = true)}>{t("Import Modpack…")}</button>
          <button class="button default" onclick={() => (launcher.newInstanceOpen = true)}>{t("New Instance")}</button>
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
<EditInstanceSheet />
<DataPacksSheet />

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

  .update-pill {
    height: 24px;
    padding: 0 10px;
    border: 1px solid var(--accent);
    border-radius: 12px;
    background: transparent;
    color: var(--accent);
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .update-pill:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
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
