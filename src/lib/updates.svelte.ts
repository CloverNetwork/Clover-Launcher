/**
 * Clover's own updates, from GitHub Releases. Checked once at launch (not in development
 * builds) and on demand from "Check for Updates…". Installing restarts the app.
 */
import { getVersion } from "@tauri-apps/api/app";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { errorText } from "./api";
import { t } from "./i18n";

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
        await message(t("Clover {version} is the newest version.", { version: await getVersion() }), { title: t("You're Up to Date") });
      }
    } catch (e) {
      if (manual) await message(t("Clover couldn't check for updates: {error}", { error: errorText(e) }), { title: t("Update Check Failed"), kind: "warning" });
    }
  }

  /** Asks first, since installing restarts Clover and stops nothing else. */
  async offer() {
    const update = this.available;
    if (!update) return;
    const notes = update.body?.trim() ? `\n\n${update.body.trim()}` : "";
    const go = await ask(t("Clover {version} is available. You have {current}.", { version: update.version, current: update.currentVersion }) + notes, {
      title: t("Update Clover?"),
      okLabel: t("Update and Restart"),
      cancelLabel: t("Later"),
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
      await message(t("The update didn't install: {error}", { error: errorText(e) }), { title: t("Update Failed"), kind: "warning" });
    }
  }
}

export const updates = new Updates();
