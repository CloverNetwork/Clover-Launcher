import { Menu } from "@tauri-apps/api/menu";
import { ask, message, save } from "@tauri-apps/plugin-dialog";
import { DELETE_INSTANCE, REMOVE_SHORTCUT_LABEL, SHORTCUT_LABEL, SHOW_FOLDER, TRASH } from "./platform";
import { locale, t } from "./i18n";
import { notifyIfAway } from "./notify";
import { api, errorText, events, type Accounts, type Instance, type LoaderSource, type CrashFinding, type Place, type QuickPlay, type Profile, type World } from "./api";

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

const MAIN_LOADERS = ["fabric", "neoforge", "forge"];

class Launcher {
  instances = $state<Instance[]>([]);
  selectedId = $state<string | null>(null);
  accounts = $state<Profile[]>([]);
  activeAccountId = $state<string | null>(null);
  account = $derived(this.accounts.find((a) => a.uuid === this.activeAccountId) ?? null);
  activity = $state<Record<string, Activity>>({});
  progress = $state<Record<string, { stage: string | null; done: number; total: number }>>({});
  loaders = $state<LoaderSource[]>([]);
  /** The loaders most people use, listed first and apart from the rest. */
  mainLoaders = $derived(MAIN_LOADERS.flatMap((id) => this.loaders.filter((l) => l.id === id)));
  otherLoaders = $derived(this.loaders.filter((l) => !l.custom && !MAIN_LOADERS.includes(l.id)));
  logs = $state<Record<string, string[]>>({});
  /** Inline error for the selected instance, shown in the detail view rather than an alert. */
  error = $state<{ instanceId: string; message: string } | null>(null);
  /** A short confirmation shown in the status line, such as the result of verifying files. */
  notice = $state<{ instanceId: string; message: string } | null>(null);
  /** Instances whose current launch had files to download; not shown, so not reactive. */
  private downloaded = new Set<string>();
  /** Likely causes of the last crash, with what happened to each fix by its index. */
  crash = $state<{ instanceId: string; findings: CrashFinding[]; done: Record<number, string> } | null>(null);

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
        if (total > 0) this.downloaded.add(instanceId);
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
        // Play time was just added.
        api.getInstance(instanceId).then((i) => this.replace(i), () => {});
        if (code !== null && code !== 0) {
          this.error = { instanceId, message: t("Minecraft quit unexpectedly (exit code {code}). The game log has details.", { code }) };
          this.logVisible = true;
          this.explainCrash(instanceId);
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
    if (instance.loader === "vanilla") return t("Vanilla");
    if (instance.loader === "custom") return t("Custom Profile");
    const name = this.loaders.find((l) => l.id === instance.loader)?.name ?? instance.loader;
    const version = instance.loaderVersion?.replace(`${instance.version}-`, "") ?? "";
    return `${name} ${version}`.trim();
  }

  /** Back to the instances: every sheet closed, and in the grid layout the grid itself. */
  closeSheets() {
    this.newInstanceOpen = this.accountOpen = this.skinsOpen = this.importOpen = this.editOpen = false;
    this.browse = null;
    this.dataPacksWorld = null;
    this.detailOpen = false;
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
      await message(t("The shortcut points to an instance that no longer exists. You can delete the shortcut."), {
        title: t("Instance Not Found"),
        kind: "warning",
      });
      return;
    }
    this.open(id);
    await this.play(id);
  }

  /** Adds the instance to the OS's launcher, or removes it if it's already there. */
  async toggleShortcut(id = this.selectedId) {
    if (!id) return;
    try {
      if (await api.hasShortcut(id)) {
        await api.removeShortcut(id);
        this.notice = { instanceId: id, message: t("Removed the shortcut.") };
        return;
      }
      const place = await api.createShortcut(id);
      this.notice = { instanceId: id, message: t("Added a shortcut: {place}", { place }) };
    } catch (e) {
      this.error = { instanceId: id, message: errorText(e) };
    }
  }

  /** Copies the command that starts an instance, for Hyprland keybinds and scripts. */
  async copyLaunchCommand(id = this.selectedId) {
    if (!id) return;
    try {
      await navigator.clipboard.writeText(await api.launchCommand(id));
      this.notice = { instanceId: id, message: t("Copied the launch command.") };
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
    const hasShortcut = await api.hasShortcut(id).catch(() => false);
    const menu = await Menu.new({
      items: [
        running
          ? { id: "stop", text: t("Stop"), action: () => this.stop(id) }
          : { id: "play", text: t("Play"), enabled: !this.activity[id], action: () => this.play(id) },
        { item: "Separator" },
        { id: "show", text: SHOW_FOLDER, action: () => this.showFolder(id) },
        { id: "verify", text: t("Verify Game Files"), enabled: !this.activity[id], action: () => this.verify(id) },
        { item: "Separator" },
        { id: "edit", text: t("Edit Instance…"), enabled: !running, action: () => this.edit(id) },
        { id: "duplicate", text: t("Duplicate"), action: () => this.duplicate(id) },
        { id: "export", text: t("Export as Modpack…"), enabled: !this.activity[id], action: () => this.exportPack(id) },
        { item: "Separator" },
        { id: "shortcut", text: hasShortcut ? REMOVE_SHORTCUT_LABEL : SHORTCUT_LABEL, action: () => this.toggleShortcut(id) },
        { id: "command", text: t("Copy Launch Command"), action: () => this.copyLaunchCommand(id) },
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

  /** Looks for known causes in the log once the last lines have arrived. */
  async explainCrash(instanceId: string) {
    await new Promise((resolve) => setTimeout(resolve, 500));
    try {
      const findings = await api.analyzeCrash(instanceId);
      if (findings.length && this.error?.instanceId === instanceId) {
        this.crash = { instanceId, findings, done: {} };
        this.error = { instanceId, message: t("Minecraft quit unexpectedly.") };
      }
    } catch {
      // The plain error and the log are still there.
    }
  }

  async applyCrashFix(index: number) {
    const crash = this.crash;
    const fix = crash?.findings[index]?.fix;
    if (!crash || !fix) return;
    if (fix.type === "editInstance") {
      this.edit(crash.instanceId);
      return;
    }
    crash.done[index] = t("Working…");
    try {
      this.replace(await api.applyCrashFix(crash.instanceId, fix));
      this.modsRevision++;
      crash.done[index] = t({ installMod: "Installed", turnOffMod: "Turned Off", setMemory: "Changed", automaticJava: "Changed" }[fix.type]);
    } catch (e) {
      delete crash.done[index];
      this.error = { instanceId: crash.instanceId, message: errorText(e) };
    }
  }

  async play(id = this.selectedId, quickPlay: QuickPlay | null = null) {
    if (!id || this.activity[id]) return;
    if (!this.account) {
      this.accountOpen = true;
      return;
    }
    this.error = null;
    this.crash = null;
    this.notice = null;
    this.activity[id] = "launching";
    this.logs[id] = [];
    try {
      this.downloaded.delete(id);
      const instance = await api.launchInstance(id, quickPlay);
      this.replace(instance);
      this.activity[id] = "running";
      // A first launch can download for minutes; say when the game is finally starting.
      if (this.downloaded.delete(id)) notifyIfAway(t("Minecraft Is Starting"), instance.name);
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
      t("Its worlds, screenshots and settings go to the {trash}, where you can restore them until it's emptied.", { trash: TRASH }),
      { title: t("Move “{name}” to the {trash}?", { name: instance.name, trash: TRASH }), kind: "warning", okLabel: DELETE_INSTANCE, cancelLabel: t("Cancel") },
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
    if (!id) return;
    try {
      await api.showInstanceFolder(id);
    } catch (e) {
      this.error = { instanceId: id, message: errorText(e) };
    }
  }

  /** Saves the instance as a Modrinth pack wherever the person picks. */
  async exportPack(id = this.selectedId) {
    const instance = this.instances.find((i) => i.id === id);
    if (!instance) return;
    const path = await save({
      title: t("Export as Modpack"),
      defaultPath: `${instance.name.replace(/[\\/:*?"<>|]/g, "-")}.mrpack`,
      filters: [{ name: t("Modrinth Modpack"), extensions: ["mrpack"] }],
    });
    if (!path) return;
    this.notice = { instanceId: instance.id, message: t("Exporting…") };
    try {
      const { linked, included } = await api.exportModpack(instance.id, path);
      const count = (n: number) => (n === 1 ? t("1 file") : t("{n} files", { n }));
      this.notice = {
        instanceId: instance.id,
        message: t("Exported. {linked} download from Modrinth; {included} packed in.", { linked: count(linked), included: count(included) }),
      };
    } catch (e) {
      this.notice = null;
      this.error = { instanceId: instance.id, message: t("Couldn't export: {error}", { error: errorText(e) }) };
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
        ? replaced === 1
          ? t("Replaced 1 missing or damaged file.")
          : t("Replaced {n} missing or damaged files.", { n: replaced.toLocaleString(locale) })
        : t("All game files are intact.");
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
