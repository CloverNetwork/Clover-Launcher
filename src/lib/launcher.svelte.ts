import { ask } from "@tauri-apps/plugin-dialog";
import { api, errorText, events, type Accounts, type Instance, type LoaderSource, type Profile } from "./api";

const MAX_LOG_LINES = 2000;

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

  sidebarVisible = $state(true);
  logVisible = $state(false);
  newInstanceOpen = $state(false);
  accountOpen = $state(false);
  skinsOpen = $state(false);

  selected = $derived(this.instances.find((i) => i.id === this.selectedId) ?? null);

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

  replace(instance: Instance) {
    this.instances = this.instances.map((i) => (i.id === instance.id ? instance : i));
  }

  async create(name: string, version: string, loader: string, loaderVersion: string | null) {
    await this.select(await api.createInstance(name, version, loader, loaderVersion));
  }

  async createFromProfile(name: string, path: string) {
    await this.select(await api.createInstanceFromProfile(name, path));
  }

  private async select(instance: Instance) {
    await this.reload();
    this.selectedId = instance.id;
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

  async stop(id = this.selectedId) {
    if (id && this.activity[id] === "running") await api.stopInstance(id);
  }

  async remove(id = this.selectedId) {
    const instance = this.instances.find((i) => i.id === id);
    if (!instance) return;
    const confirmed = await ask(
      `Its worlds, screenshots and settings will be deleted. You can't undo this action.`,
      { title: `Delete “${instance.name}”?`, kind: "warning", okLabel: "Delete", cancelLabel: "Cancel" },
    );
    if (!confirmed) return;
    try {
      await api.deleteInstance(instance.id);
      await this.reload();
    } catch (e) {
      this.error = { instanceId: instance.id, message: errorText(e) };
    }
  }

  async showFolder(id = this.selectedId) {
    if (id) await api.showInstanceFolder(id);
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
