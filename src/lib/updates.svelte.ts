/**
 * Clover's own updates, from GitHub Releases. Checked once at launch (not in development
 * builds) and on demand from "Check for Updates…". Installing restarts the app.
 */
import { getVersion } from "@tauri-apps/api/app";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { errorText } from "./api";

class Updates {
  available = $state<Update | null>(null);
  /** Download progress from 0 to 1 while installing. */
  progress = $state<number | null>(null);

  /** `manual` reports "up to date" and errors; the launch check stays quiet. */
  async check(manual = false) {
    if (import.meta.env.DEV && !manual) return;
    try {
      this.available = await check();
      if (this.available && manual) await this.offer();
      else if (!this.available && manual) {
        await message(`Clover ${await getVersion()} is the newest version.`, { title: "You're Up to Date" });
      }
    } catch (e) {
      if (manual) await message(`Clover couldn't check for updates: ${errorText(e)}`, { title: "Update Check Failed", kind: "warning" });
    }
  }

  /** Asks first, since installing restarts Clover and stops nothing else. */
  async offer() {
    const update = this.available;
    if (!update) return;
    const notes = update.body?.trim() ? `\n\n${update.body.trim()}` : "";
    const go = await ask(`Clover ${update.version} is available. You have ${update.currentVersion}.${notes}`, {
      title: "Update Clover?",
      okLabel: "Update and Restart",
      cancelLabel: "Later",
    });
    if (go) await this.install();
  }

  async install() {
    const update = this.available;
    if (!update || this.progress !== null) return;
    let total = 0;
    let done = 0;
    this.progress = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? 0;
        if (event.event === "Progress") {
          done += event.data.chunkLength;
          this.progress = total ? done / total : null;
        }
      });
      await relaunch();
    } catch (e) {
      this.progress = null;
      await message(`The update didn't install: ${errorText(e)}`, { title: "Update Failed", kind: "warning" });
    }
  }
}

export const updates = new Updates();
