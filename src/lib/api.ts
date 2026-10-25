import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Instance = {
  id: string;
  name: string;
  version: string;
  /** "vanilla", "custom", or a loader source ID such as "fabric". */
  loader: string;
  /** The loader's version, or the profile ID for a custom profile. */
  loaderVersion: string | null;
  createdAt: number;
  lastPlayed: number | null;
  installed: boolean;
};

export type VersionEntry = { id: string; type: string; releaseTime: string };
export type LoaderSource = {
  id: string;
  name: string;
  kind: "meta" | "forge" | "neoForge";
  url?: string;
  loaderPath?: string;
  custom: boolean;
};
export type LoaderVersion = { version: string; label: string; stable: boolean };
export type Settings = {
  javaPath: string;
  memoryMb: number;
  loaderSources: LoaderSource[];
  curseforgeApiKey: string;
  /** On Hyprland, where windows have no title bar: show Clover's own window buttons. */
  showWindowButtons: boolean;
};
/** How windows are framed: on Hyprland they have no title bar. */
export type WindowChrome = { hyprland: boolean; buttons: boolean };
export type ModSource = "modrinth" | "curseforge";
export type ModHit = {
  source: ModSource;
  id: string;
  title: string;
  author: string;
  description: string;
  iconUrl: string | null;
  downloads: number;
  pageUrl: string;
};
export type ModPage = { hits: ModHit[]; total: number };
/** A newer file for an installed mod; passed back as is to install it. */
export type ModUpdate = {
  file: string;
  title: string;
  source: ModSource;
  projectId: string;
  currentVersion: string | null;
  newVersion: string;
  newFile: string;
  url: string;
  sha1: string | null;
  iconUrl: string | null;
  requires: string[];
};
export type BlockedFile = { name: string; url: string; folder: string };
export type ImportedPack = { instance: Instance; blocked: BlockedFile[] };
export type ImportProgress = { stage: string | null; done: number; total: number };
export type UpdateCheck = { updates: ModUpdate[]; skipped: string | null };
/** Where launchers apply for a third-party CurseForge API key. */
export const CURSEFORGE_KEY_FORM = "https://forms.monday.com/forms/dce5ccb7afda9a1c21dab1a1aa1d84eb?r=use1";
export type InstalledMod = {
  file: string;
  title: string;
  source: ModSource | null;
  projectId: string | null;
  version: string | null;
  iconUrl: string | null;
  /** Installed because another mod needs it. */
  dependency: boolean;
  enabled: boolean;
};
export type Profile = { uuid: string; name: string; offline: boolean };
export type Accounts = { active: string | null; accounts: Profile[] };
export type SkinVariant = "classic" | "slim";
export type RemoteSkin = { id: string; state: "ACTIVE" | "INACTIVE"; url: string; variant: "CLASSIC" | "SLIM" };
export type RemoteCape = { id: string; state: "ACTIVE" | "INACTIVE"; url: string; alias: string | null };
export type Wardrobe = { skins: RemoteSkin[]; capes: RemoteCape[] };
export type LibrarySkin = { id: string; name: string; variant: SkinVariant };
export type DeviceCode = {
  userCode: string;
  deviceCode: string;
  verificationUri: string;
  interval: number;
  expiresIn: number;
};

export type InstallProgress = { instanceId: string; stage: string | null; done: number; total: number };
export type LogLine = { instanceId: string; line: string };
export type GameExit = { instanceId: string; code: number | null };

export const api = {
  listVersions: (includeAll: boolean) => invoke<VersionEntry[]>("list_versions", { includeAll }),
  listInstances: () => invoke<Instance[]>("list_instances"),
  listLoaders: () => invoke<LoaderSource[]>("list_loaders"),
  listLoaderVersions: (loader: string, gameVersion: string) =>
    invoke<LoaderVersion[]>("list_loader_versions", { loader, gameVersion }),
  addLoaderSource: (name: string, url: string, loaderPath: string) =>
    invoke<LoaderSource>("add_loader_source", { name, url, loaderPath }),
  removeLoaderSource: (id: string) => invoke<void>("remove_loader_source", { id }),
  createInstance: (name: string, version: string, loader: string, loaderVersion: string | null) =>
    invoke<Instance>("create_instance", { name, version, loader, loaderVersion }),
  createInstanceFromProfile: (name: string, path: string) =>
    invoke<Instance>("create_instance_from_profile", { name, path }),
  deleteInstance: (id: string) => invoke<void>("delete_instance", { id }),
  /** Re-hashes every file and replaces damaged ones. Resolves to the instance and how many files were replaced. */
  verifyInstance: (id: string) => invoke<[Instance, number]>("verify_instance", { id }),
  launchInstance: (id: string) => invoke<Instance>("launch_instance", { id }),
  stopInstance: (id: string) => invoke<void>("stop_instance", { id }),
  searchMods: (id: string, source: ModSource, query: string, offset: number) =>
    invoke<ModPage>("search_mods", { id, source, query, offset }),
  listMods: (id: string) => invoke<InstalledMod[]>("list_mods", { id }),
  /** Rejects with CurseForge's reason when the key doesn't work. */
  checkCurseforgeKey: (key: string) => invoke<void>("check_curseforge_key", { key }),
  /** Resolves to the titles of everything added, dependencies included. */
  installMod: (id: string, source: ModSource, project: string) => invoke<string[]>("install_mod", { id, source, project }),
  searchModpacks: (source: ModSource, query: string, offset: number) =>
    invoke<ModPage>("search_modpacks", { source, query, offset }),
  installModpack: (source: ModSource, project: string) => invoke<ImportedPack>("install_modpack", { source, project }),
  importModpackFile: (path: string) => invoke<ImportedPack>("import_modpack_file", { path }),
  checkModUpdates: (id: string) => invoke<UpdateCheck>("check_mod_updates", { id }),
  /** Resolves to the titles of everything updated or newly required. */
  updateMods: (id: string, updates: ModUpdate[]) => invoke<string[]>("update_mods", { id, updates }),
  setModEnabled: (id: string, file: string, enabled: boolean) => invoke<void>("set_mod_enabled", { id, file, enabled }),
  removeMod: (id: string, file: string) => invoke<void>("remove_mod", { id, file }),
  showModsFolder: (id: string) => invoke<void>("show_mods_folder", { id }),
  /** The recent output of an instance's game and installers. */
  gameLog: (id: string) => invoke<string[]>("game_log", { id }),
  openLogWindow: (id: string) => invoke<void>("open_log_window", { id }),
  /** Closes a log window and shows the log under the instance page again. */
  popInLogWindow: (id: string) => invoke<void>("pop_in_log_window", { id }),
  /** Shares the log on mclo.gs; tokens and the home folder are removed first. */
  uploadLog: (id: string) => invoke<{ url: string }>("upload_log", { id }),
  showInstanceFolder: (id: string) => invoke<void>("show_instance_folder", { id }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  getAccounts: () => invoke<Accounts>("get_accounts"),
  switchAccount: (uuid: string) => invoke<Accounts>("switch_account", { uuid }),
  removeAccount: (uuid: string) => invoke<Accounts>("remove_account", { uuid }),
  beginSignIn: () => invoke<DeviceCode>("begin_sign_in"),
  finishSignIn: (code: DeviceCode) => invoke<Accounts>("finish_sign_in", { code }),
  /** Development builds only; release builds refuse it. */
  signInOffline: (name: string) => invoke<Accounts>("sign_in_offline", { name }),
  openSettings: () => invoke<void>("open_settings"),
  windowChrome: () => invoke<WindowChrome>("window_chrome"),
  /** An instance named with --launch when Clover started, once. */
  takePendingLaunch: () => invoke<string | null>("take_pending_launch"),
  /** Adds the instance to the OS's launcher; resolves to where it went. */
  createShortcut: (id: string) => invoke<string>("create_shortcut", { id }),
  launchCommand: (id: string) => invoke<string>("launch_command", { id }),
  quit: () => invoke<void>("quit"),
  /** `null` for offline accounts, which have no skins on Mojang's servers. */
  getWardrobe: () => invoke<Wardrobe | null>("get_wardrobe"),
  listSkinLibrary: () => invoke<LibrarySkin[]>("list_skin_library"),
  addSkinToLibrary: (path: string, variant: SkinVariant) => invoke<LibrarySkin>("add_skin_to_library", { path, variant }),
  updateLibrarySkin: (id: string, changes: { name?: string; variant?: SkinVariant }) =>
    invoke<LibrarySkin>("update_library_skin", { id, ...changes }),
  removeLibrarySkin: (id: string) => invoke<void>("remove_library_skin", { id }),
  librarySkinTexture: (id: string) => invoke<string>("library_skin_texture", { id }),
  skinTexture: (url: string) => invoke<string>("skin_texture", { url }),
  saveWornSkin: (url: string, variant: SkinVariant, name: string) =>
    invoke<LibrarySkin>("save_worn_skin", { url, variant, name }),
  wearLibrarySkin: (id: string) => invoke<Wardrobe>("wear_library_skin", { id }),
  setSkinVariant: (variant: SkinVariant) => invoke<Wardrobe>("set_skin_variant", { variant }),
  wearCape: (capeId: string | null) => invoke<Wardrobe>("wear_cape", { capeId }),
};

export const events = {
  installProgress: (fn: (p: InstallProgress) => void) => listen<InstallProgress>("install-progress", (e) => fn(e.payload)),
  gameLog: (fn: (l: LogLine) => void) => listen<LogLine>("game-log", (e) => fn(e.payload)),
  gameExit: (fn: (x: GameExit) => void) => listen<GameExit>("game-exit", (e) => fn(e.payload)),
  /** A new launch started this instance's log over. */
  importProgress: (fn: (p: ImportProgress) => void) => listen<ImportProgress>("import-progress", (e) => fn(e.payload)),
  /** A shortcut or keybind asked the open app to start an instance. */
  launchRequest: (fn: (id: string) => void) => listen<string>("launch-request", (e) => fn(e.payload)),
  windowChrome: (fn: (c: WindowChrome) => void) => listen<WindowChrome>("window-chrome", (e) => fn(e.payload)),
  logPoppedIn: (fn: (instanceId: string) => void) => listen<string>("log-popped-in", (e) => fn(e.payload)),
  gameLogCleared: (fn: (instanceId: string) => void) => listen<string>("game-log-cleared", (e) => fn(e.payload)),
  menu: (fn: (id: string) => void) => listen<string>("menu-command", (e) => fn(e.payload)),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : "Something went wrong.";
}
