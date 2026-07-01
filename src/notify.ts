// Thin wrapper around the OS notification plugin for incoming calls.

import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

/** Fire a desktop notification for an incoming call (best-effort). */
export async function notifyIncomingCall(reason: string | null): Promise<void> {
  try {
    let granted = await isPermissionGranted();
    if (!granted) {
      granted = (await requestPermission()) === "granted";
    }
    if (granted) {
      sendNotification({
        title: "Copilot is calling…",
        body: reason ?? "Incoming voice call",
      });
    }
  } catch {
    // Notifications are non-essential; ignore failures.
  }
}
