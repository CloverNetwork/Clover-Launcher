/**
 * Which OS Clover is running on, and the words and keys each one uses, so the app reads
 * like it belongs there: "Show in Finder" on macOS, "Show in Explorer" on Windows.
 */
import { t } from "./i18n";

export type Platform = "macos" | "windows" | "linux";

export const platform: Platform =
  typeof navigator === "undefined"
    ? "macos"
    : navigator.userAgent.includes("Mac")
      ? "macos"
      : navigator.userAgent.includes("Windows")
        ? "windows"
        : "linux";

/** Only macOS gets the system menu bar; Windows and Linux apps keep commands in the window. */
export const hasMenuBar = platform === "macos";

/** Opening an instance's game folder in the file manager. */
export const SHOW_FOLDER = t({ macos: "Show in Finder", windows: "Show in Explorer", linux: "Show in Files" }[platform]);

/** GNOME calls an app's settings Preferences; macOS and Windows say Settings. */
export const SETTINGS = t({ macos: "Settings", windows: "Settings", linux: "Preferences" }[platform]);

/** Where an instance shortcut goes, in each OS's words. */
export const SHORTCUT_LABEL = t(
  {
    macos: "Add to Applications",
    windows: "Create Desktop Shortcut",
    linux: "Add to App Launcher",
  }[platform],
);

/** Deleted instances go to the system's bin, which each OS names differently. */
export const TRASH = t({ macos: "Trash", windows: "Recycle Bin", linux: "Trash" }[platform]);

/** Finder and Files say "Move to Trash"; Windows Explorer says "Delete". */
export const DELETE_INSTANCE = t({ macos: "Move to Trash", windows: "Delete", linux: "Move to Trash" }[platform]);

/** The command key: ⌘ on macOS, Ctrl elsewhere. */
export function commandKey(e: KeyboardEvent): boolean {
  return platform === "macos" ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
}
