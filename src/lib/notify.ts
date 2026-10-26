/**
 * A system notification when something long finishes while Clover is in the background.
 * In front, the window itself shows the result, so nothing extra appears.
 */
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";

export async function notifyIfAway(title: string, body: string) {
  if (document.hasFocus()) return;
  try {
    // The system asks once; after that the answer is remembered.
    if (!(await isPermissionGranted()) && (await requestPermission()) !== "granted") return;
    sendNotification({ title, body });
  } catch {
    // Notifications are a nicety; the window still has the result.
  }
}
