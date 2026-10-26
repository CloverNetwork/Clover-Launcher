import { Menu } from "@tauri-apps/api/menu";
import { ask, message, save } from "@tauri-apps/plugin-dialog";
import { DELETE_INSTANCE, SHORTCUT_LABEL, SHOW_FOLDER, TRASH } from "./platform";
import { api, errorText, events, type Accounts, type Instance, type LoaderSource, type Place, type Profile, type World } from "./api";

const MAX_LOG_LINES = 2000;

const LAYOUT_KEY = "clover.layout";

export type Layout = "sidebar" | "grid";

/** The layout is a per-computer view preference, so it lives in local storage. */
function storedLayout(): Layout {
  try {
    return localStorage.getItem(LAYOUT_KEY) === "grid" ? "grid" : "sidebar";
  } catch {
    return "sidebar";
  }
}

const SORT_KEY = "clover.sort";

export type Sort = "name" | "recent";

export type InstanceGroup = { name: string | null; instances: Instance[] };

function storedSort(): Sort {
  try {
    return localStorage.getItem(SORT_KEY) === "recent" ? "recent" : "name";
  } catch {
    return "name";
  }
}

type Activity = "verifying" | "launching" | "running";

class Launcher {
  instances = $state<Instance[]>([]);
  selectedId = $state<string | null>(null);
  accounts = $state<Profile[]>([]);
  activeAccountId = $state<string | null>(null);
  account = $derived(this.accounts.find((a) => a.uuid === this.activeAccountId) ?? null);
  activity = $state<Record<string, Activity>>({});
  progress = $state<Record<string, { stage: string | null; done: number; total: number }>>({});
  loaders = $state<LoaderSource[]>([]);
  logs = $state<Record<string, string[]>>({});
  /** Inline error for the selected instance, shown in the detail view rather than an alert. */
  error = $state<{ instanceId: string; message: string } | null>(null);
  /** A short confirmation shown in the status line, such as the result of verifying files. */
  notice = $state<{ instanceId: string; message: string } | null>(null);

  layout = $state<Layout>(storedLayout());
  /** In the grid layout, whether the selected instance's page is open over the grid. */
  detailOpen = $state(false);
  sidebarVisible = $state(true);
  logVisible = $state(false);
  newInstanceOpen = $state(false);
  accountOpen = $state(false);
  skinsOpen = $state(false);
  /** The folder the Add Mods (or packs) sheet installs into, while it's open. */
  browse = $state<Place | null>(null);
  /** The world whose data packs sheet is open. */
  dataPacksWorld = $state<World | null>(null);
  importOpen = $state(false);
  editOpen = $state(false);
  /** Sidebar and grid filter. */
  query = $state("");
  sort = $state<Sort>(storedSort());
  /** Bumped when mods change, so lists showing them reload. */
  modsRevision = $state(0);

  selected = $derived(this.instances.find((i) => i.id === this.selectedId) ?? null);

  /** Instances as the sidebar and grid show them: filtered, sorted, ungrouped first. */
  groups = $derived.by<InstanceGroup[]>(() => {
    const query = this.query.trim().toLowerCase();
    const matching = this.instances.filter((i) => !query || i.name.toLowerCase().includes(query));
    const sorted = [...matching].sort((a, b) =>
      this.sort === "recent" ? (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0) : a.name.localeCompare(b.name),
    );
    const byGroup = new Map<string | null, Instance[]>();
    for (const instance of sorted) {
      const group = instance.options?.group ?? null;
      byGroup.set(group, [...(byGroup.get(group) ?? []), instance]);
    }
    return [...byGroup.entries()]
      .sort(([a], [b]) => (a === null ? -1 : b === null ? 1 : a.localeCompare(b)))
      .map(([name, instances]) => ({ name, instances }));
  });

  /** Every group name in use, for the Edit Instance suggestions. */
  groupNames = $derived([...new Set(this.instances.map((i) => i.options?.group).filter((g): g is string => !!g))].sort());

  setSort(sort: Sort) {
    this.sort = sort;
    try {
      localStorage.setItem(SORT_KEY, sort);
    } catch {
      // Private storage just means the choice isn't remembered.
    }
  }

  edit(id = this.selectedId) {
    if (!id) return;
    this.selectedId = id;
    this.editOpen = true;
  }

  async duplicate(id = this.selectedId) {
    if (!id) return;
    try {
      await this.select(await api.duplicateInstance(id));
    } catch (e) {
      this.error = { instanceId: id, message: errorText(e) };
    }
  }

  async init() {
    await Promise.all([
      events.installProgress(({ instanceId, stage, done, total }) => {
        // A new stage resets the file count; file updates keep the current stage.
        this.progress[instanceId] = stage ? { stage, done: 0, total: 0 } : { stage: null, done, total };
      }),
      events.gameLog(({ instanceId, line }) => {
        const lines = (this.logs[instanceId] ??= []);
        lines.push(line);
        if (lines.length > MAX_LOG_LINES) lines.splice(0, lines.length - MAX_LOG_LINES);
      }),
      events.launchRequest((id) => this.launchFromOutside(id)),
      events.logPoppedIn((instanceId) => {
        this.open(instanceId);
        this.logVisible = true;
      }),
      events.gameExit(({ instanceId, code }) => {
        delete this.activity[instanceId];
        if (code !== null && code !== 0) {
          this.error = { instanceId, message: `Minecraft quit unexpectedly (exit code ${code}). The game log has details.` };
          this.logVisible = true;
        }
      }),
    ]);
    this.setAccounts(await api.getAccounts());
    await Promise.all([this.reload(), this.reloadLoaders()]);
    const pending = await api.takePendingLaunch();
    if (pending) this.launchFromOutside(pending);
  }

  async reload() {
    this.instances = await api.listInstances();
    if (!this.instances.some((i) => i.id === this.selectedId)) {
      this.selectedId = this.instances[0]?.id ?? null;
    }
  }

  async reloadLoaders() {
    this.loaders = await api.listLoaders();
  }

  /** "Vanilla", "Fabric 0.20.1", "Forge 54.1.0", or "Custom Profile". */
  loaderLabel(instance: Instance) {
    if (instance.loader === "vanilla") return "Vanilla";
    if (instance.loader === "custom") return "Custom Profile";
    const name = this.loaders.find((l) => l.id === instance.loader)?.name ?? instance.loader;
    const version = instance.loaderVersion?.replace(`${instance.version}-`, "") ?? "";
    return `${name} ${version}`.trim();
  }

  setLayout(layout: Layout) {
    this.layout = layout;
    this.detailOpen = false;
    try {
      localStorage.setItem(LAYOUT_KEY, layout);
    } catch {
      // Private storage just means the choice isn't remembered.
    }
  }

  /** Plays an instance named by a shortcut, keybind or `--launch`. */
  async launchFromOutside(id: string) {
    if (!this.instances.some((i) => i.id === id)) await this.reload();
    if (!this.instances.some((i) => i.id === id)) {
      await message("The shortcut points to an instance that no longer exists. You can delete the shortcut.", {
        title: "Instance Not Found",
        kind: "warning",
      });
      return;
    }
    this.open(id);
    await this.play(id);
  }

  async createShortcut(id = this.selectedId) {
    if (!id) return;
    try {
      const place = await api.createShortcut(id);
      this.notice = { instanceId: id, message: `Added a shortcut: ${place}` };
    } catch (e) {
      this.error = { instanceId: id, message: errorText(e) };
    }
  }

  /** Copies the command that starts an instance, for Hyprland keybinds and scripts. */
  async copyLaunchCommand(id = this.selectedId) {
    if (!id) return;
    try {
      await navigator.clipboard.writeText(await api.launchCommand(id));
      this.notice = { instanceId: id, message: "Copied the launch command." };
    } catch (e) {
      this.error = { instanceId: id, message: errorText(e) };
    }
  }

  /** Opens an instance: in the grid layout that shows its page, in the sidebar it selects it. */
  open(id: string) {
    this.selectedId = id;
    if (this.layout === "grid") this.detailOpen = true;
  }

  /** The right-click menu for an instance, shared by the sidebar and the grid. */
  async showInstanceMenu(e: MouseEvent, id: string) {
    e.preventDefault();
    this.selectedId = id;
    const running = this.activity[id] === "running";
    const menu = await Menu.new({
      items: [
        running
          ? { id: "stop", text: "Stop", action: () => this.stop(id) }
          : { id: "play", text: "Play", enabled: !this.activity[id], action: () => this.play(id) },
        { item: "Separator" },
        { id: "show", text: SHOW_FOLDER, action: () => this.showFolder(id) },
        { id: "verify", text: "Verify Game Files", enabled: !this.activity[id], action: () => this.verify(id) },
        { item: "Separator" },
        { id: "edit", text: "Edit Instance…", enabled: !running, action: () => this.edit(id) },
        { id: "duplicate", text: "Duplicate", action: () => this.duplicate(id) },
        { id: "export", text: "Export as Modpack…", enabled: !this.activity[id], action: () => this.exportPack(id) },
        { item: "Separator" },
        { id: "shortcut", text: SHORTCUT_LABEL, action: () => this.createShortcut(id) },
        { id: "command", text: "Copy Launch Command", action: () => this.copyLaunchCommand(id) },
        { item: "Separator" },
        { id: "delete", text: DELETE_INSTANCE, enabled: !running, action: () => this.remove(id) },
      ],
    });
    await menu.popup();
  }

  replace(instance: Instance) {
    this.instances = this.instances.map((i) => (i.id === instance.id ? instance : i));
  }

  async create(name: string, version: string, loader: string, loaderVersion: string | null) {
    await this.select(await api.createInstance(name, version, loader, loaderVersion));
  }

  async createFromProfile(name: string, path: string) {
    await this.select(await api.createInstanceFromProfile(name, path));
  }

  /** Shows a freshly imported instance. */
  async showImported(instance: Instance) {
    await this.select(instance);
  }

  private async select(instance: Instance) {
    await this.reload();
    this.open(instance.id);
  }

  async play(id = this.selectedId) {
    if (!id || this.activity[id]) return;
    if (!this.account) {
      this.accountOpen = true;
      return;
    }
    this.error = null;
    this.notice = null;
    this.activity[id] = "launching";
    this.logs[id] = [];
    try {
      this.replace(await api.launchInstance(id));
      this.activity[id] = "running";
    } catch (e) {
      delete this.activity[id];
      this.error = { instanceId: id, message: errorText(e) };
    } finally {
      delete this.progress[id];
    }
  }

  /** Moves the game log into its own window. */
  async popOutLog(id = this.selectedId) {
    if (!id) return;
    await api.openLogWindow(id);
    this.logVisible = false;
  }

  async stop(id = this.selectedId) {
    if (id && this.activity[id] === "running") await api.stopInstance(id);
  }

  async remove(id = this.selectedId) {
    const instance = this.instances.find((i) => i.id === id);
    if (!instance) return;
    const confirmed = await ask(
      `Its worlds, screenshots and settings go to the ${TRASH}, where you can restore them until it's emptied.`,
      { title: `Move “${instance.name}” to the ${TRASH}?`, kind: "warning", okLabel: DELETE_INSTANCE, cancelLabel: "Cancel" },
    );
    if (!confirmed) return;
    try {
      await api.deleteInstance(instance.id);
      await this.reload();
      this.detailOpen = false;
    } catch (e) {
      this.error = { instanceId: instance.id, message: errorText(e) };
    }
  }

  async showFolder(id = this.selectedId) {
    if (id) await api.showInstanceFolder(id);
  }

  /** Saves the instance as a Modrinth pack wherever the person picks. */
  async exportPack(id = this.selectedId) {
    const instance = this.instances.find((i) => i.id === id);
    if (!instance) return;
    const path = await save({
      title: "Export as Modpack",
      defaultPath: `${instance.name.replace(/[\\/:*?"<>|]/g, "-")}.mrpack`,
      filters: [{ name: "Modrinth Modpack", extensions: ["mrpack"] }],
    });
    if (!path) return;
    this.notice = { instanceId: instance.id, message: "Exporting…" };
    try {
      const { linked, included } = await api.exportModpack(instance.id, path);
      const count = (n: number) => `${n} ${n === 1 ? "file" : "files"}`;
      this.notice = {
        instanceId: instance.id,
        message: `Exported. ${count(linked)} download from Modrinth; ${count(included)} packed in.`,
      };
    } catch (e) {
      this.notice = null;
      this.error = { instanceId: instance.id, message: `Couldn't export: ${errorText(e)}` };
    }
  }

  /** Re-checks every file, replacing damaged ones. */
  async verify(id = this.selectedId) {
    if (!id || this.activity[id]) return;
    this.error = null;
    this.notice = null;
    this.activity[id] = "verifying";
    try {
      const [instance, replaced] = await api.verifyInstance(id);
      this.replace(instance);
      const message = replaced
        ? `Replaced ${replaced.toLocaleString()} missing or damaged ${replaced === 1 ? "file" : "files"}.`
        : "All game files are intact.";
      this.notice = { instanceId: id, message };
    } catch (e) {
      this.error = { instanceId: id, message: errorText(e) };
    } finally {
      delete this.activity[id];
      delete this.progress[id];
    }
  }

  setAccounts(accounts: Accounts) {
    this.accounts = accounts.accounts;
    this.activeAccountId = accounts.active;
  }

  async switchAccount(uuid: string) {
    this.setAccounts(await api.switchAccount(uuid));
  }

  /** Signs out the active account; the next one, if any, becomes active. */
  async signOut(uuid = this.activeAccountId) {
    if (uuid) this.setAccounts(await api.removeAccount(uuid));
  }
}

export const launcher = new Launcher();
