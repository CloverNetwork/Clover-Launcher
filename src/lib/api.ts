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
export type Settings = { javaPath: string; memoryMb: number; loaderSources: LoaderSource[] };
export type Profile = { uuid: string; name: string };
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
  installInstance: (id: string) => invoke<Instance>("install_instance", { id }),
  launchInstance: (id: string) => invoke<Instance>("launch_instance", { id }),
  stopInstance: (id: string) => invoke<void>("stop_instance", { id }),
  showInstanceFolder: (id: string) => invoke<void>("show_instance_folder", { id }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  getAccount: () => invoke<Profile | null>("get_account"),
  beginSignIn: () => invoke<DeviceCode>("begin_sign_in"),
  finishSignIn: (code: DeviceCode) => invoke<Profile>("finish_sign_in", { code }),
  signOut: () => invoke<void>("sign_out"),
  openSettings: () => invoke<void>("open_settings"),
};

export const events = {
  installProgress: (fn: (p: InstallProgress) => void) => listen<InstallProgress>("install-progress", (e) => fn(e.payload)),
  gameLog: (fn: (l: LogLine) => void) => listen<LogLine>("game-log", (e) => fn(e.payload)),
  gameExit: (fn: (x: GameExit) => void) => listen<GameExit>("game-exit", (e) => fn(e.payload)),
  menu: (fn: (id: string) => void) => listen<string>("menu-command", (e) => fn(e.payload)),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : "Something went wrong.";
}
