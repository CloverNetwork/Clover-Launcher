<!--
  The instance's servers, from the game's own list, with live status. Join starts the game
  and connects straight away.
-->
<script lang="ts">
  import { Menu } from "@tauri-apps/api/menu";
  import { api, errorText, type Instance, type Server, type ServerStatus } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import BlockIcon from "./BlockIcon.svelte";
  import Icon from "./Icon.svelte";
  import { locale, t } from "$lib/i18n";

  let { instance }: { instance: Instance } = $props();

  let servers = $state<Server[]>([]);
  let loaded = $state(false);
  let error = $state<string | null>(null);
  /** Keyed by address: a status, an error text, or "pinging". */
  let statuses = $state<Record<string, ServerStatus | string>>({});
  const busy = $derived(!!launcher.activity[instance.id]);

  // The editing sheet: `index` is null when adding.
  let dialog: HTMLDialogElement | undefined = $state();
  let editing = $state<{ index: number | null; name: string; address: string } | null>(null);
  let saveError = $state<string | null>(null);

  $effect(() => {
    const id = instance.id;
    api
      .listServers(id)
      .then((list) => {
        if (id !== instance.id) return;
        servers = list;
        loaded = true;
        pingAll(list);
      })
      .catch((e) => (error = errorText(e)));
  });

  $effect(() => {
    if (editing) dialog?.showModal();
    else dialog?.close();
  });

  function pingAll(list: Server[]) {
    for (const server of list) ping(server.address);
  }

  async function ping(address: string) {
    statuses[address] = "pinging";
    try {
      statuses[address] = await api.pingServer(instance.id, address);
    } catch (e) {
      statuses[address] = errorText(e);
    }
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!editing) return;
    saveError = null;
    try {
      servers = await api.saveServer(instance.id, editing.index, editing.name, editing.address);
      const address = editing.address.trim();
      editing = null;
      ping(address);
    } catch (err) {
      saveError = errorText(err);
    }
  }

  async function remove(index: number) {
    error = null;
    try {
      servers = await api.removeServer(instance.id, index);
    } catch (e) {
      error = errorText(e);
    }
  }

  async function showMenu(e: MouseEvent, server: Server, index: number) {
    e.preventDefault();
    const menu = await Menu.new({
      items: [
        { id: "join", text: t("Join"), enabled: !busy, action: () => launcher.play(instance.id, { type: "server", address: server.address }) },
        { id: "refresh", text: t("Refresh"), action: () => ping(server.address) },
        { item: "Separator" },
        { id: "edit", text: t("Edit…"), action: () => (editing = { index, name: server.name, address: server.address }) },
        { id: "copy", text: t("Copy Address"), action: () => navigator.clipboard.writeText(server.address) },
        { item: "Separator" },
        { id: "remove", text: t("Remove"), action: () => remove(index) },
      ],
    });
    await menu.popup();
  }

  const count = (n: number) => n.toLocaleString(locale);
</script>

<section class="servers" aria-labelledby="servers-title">
  <header>
    <h2 id="servers-title">{t("Servers")}</h2>
    <div class="spacer"></div>
    {#if servers.length}
      <button class="icon-button" aria-label={t("Refresh All")} title={t("Refresh All")} onclick={() => pingAll(servers)}>
        <Icon name="refresh" size={14} />
      </button>
    {/if}
    <button class="button" onclick={() => (editing = { index: null, name: "", address: "" })}>{t("Add Server…")}</button>
  </header>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  {#if servers.length}
    <ul>
      {#each servers as server, index (index + server.address)}
        {@const status = statuses[server.address]}
        {@const icon = (typeof status === "object" && status.icon) || server.icon}
        <li oncontextmenu={(e) => showMenu(e, server, index)}>
          {#if icon}
            <img src={icon} alt="" width="40" height="40" />
          {:else}
            <BlockIcon seed={server.address} size={40} />
          {/if}
          <div class="text">
            <span class="title">{server.name}</span>
            {#if typeof status === "object"}
              <span class="secondary detail" title={status.motd}>{status.motd || server.address}</span>
              <span class="secondary detail meta">
                {t("{online} of {max} playing", { online: count(status.online), max: count(status.max) })} · {status.latencyMs} ms{status.version ? ` · ${status.version}` : ""}
              </span>
            {:else}
              <span class="secondary detail">{server.address}</span>
              <span class="secondary detail meta">{status === "pinging" ? t("Checking…") : (status ?? "")}</span>
            {/if}
          </div>
          <button
            class="button small"
            disabled={busy}
            onclick={() => launcher.play(instance.id, { type: "server", address: server.address })}>{t("Join")}</button
          >
          <button class="icon-button" aria-label={t("More for {name}", { name: server.name })} title={t("More")} onclick={(e) => showMenu(e, server, index)}>
            <Icon name="more" />
          </button>
        </li>
      {/each}
    </ul>
  {:else if loaded}
    <p class="empty secondary">{t("No servers yet. Add one here or in the game's Multiplayer screen.")}</p>
  {/if}
</section>

<dialog class="sheet" bind:this={dialog} aria-labelledby="server-title" onclose={() => (editing = null)}>
  {#if editing}
    <form onsubmit={save}>
      <h2 id="server-title">{editing.index === null ? t("Add Server") : t("Edit Server")}</h2>
      <div class="grid">
        <label for="server-name">{t("Name")}</label>
        <input id="server-name" class="field" bind:value={editing.name} placeholder={t("Minecraft Server")} autocomplete="off" />
        <label for="server-address">{t("Address")}</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          id="server-address"
          class="field"
          bind:value={editing.address}
          placeholder="mc.example.com"
          autocomplete="off"
          spellcheck="false"
          autocapitalize="off"
          required
          autofocus
        />
      </div>
      {#if saveError}
        <p class="error" role="alert">{saveError}</p>
      {/if}
      <div class="actions">
        <button type="button" class="button" onclick={() => (editing = null)}>{t("Cancel")}</button>
        <button type="submit" class="button default" disabled={!editing.address.trim()}>
          {editing.index === null ? t("Add") : t("Save")}
        </button>
      </div>
    </form>
  {/if}
</dialog>

<style>
  .servers {
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

  .meta {
    font-variant-numeric: tabular-nums;
  }

  .small {
    flex: none;
    min-height: 22px;
    padding: 2px 12px;
    font-size: 12px;
  }

  dialog {
    width: min(420px, calc(100vw - 32px));
  }

  .grid {
    display: grid;
    grid-template-columns: 70px 1fr;
    gap: 10px 12px;
    align-items: baseline;
    margin-top: 14px;
  }

  .grid label {
    text-align: right;
  }

  .empty,
  .error {
    margin: 0 0 8px;
  }

  .error {
    color: var(--danger);
  }

  dialog .error {
    margin: 12px 0 0;
  }
</style>
