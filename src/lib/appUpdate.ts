import { relaunch } from "@tauri-apps/plugin-process";
import { check, Update } from "@tauri-apps/plugin-updater";

export type { Update };

/**
 * The DevHub self-update flow, shared by the header banner (which checks on its
 * own) and the Settings card (which checks when asked). Both must download,
 * verify, and relaunch identically — a second copy of this would eventually
 * disagree with the first about progress or restart behaviour.
 *
 * Signature verification is the updater plugin's job: `check` only resolves for
 * a release signed with the project key configured in tauri.conf.json.
 */

/** How long to wait on the GitHub feed before giving up. */
const CHECK_TIMEOUT_MS = 15000;

/** The newer release the update feed offers, or null when already current. */
export async function checkForAppUpdate(): Promise<Update | null> {
  return await check({ timeout: CHECK_TIMEOUT_MS });
}

/**
 * Download and install `update`, then restart into it. `onProgress` receives a
 * percentage, or null while the download size is unknown (the feed does not
 * always send a content length).
 *
 * This does not return on success — the process is replaced by the relaunch.
 */
export async function installAppUpdate(
  update: Update,
  onProgress?: (percent: number | null) => void,
): Promise<void> {
  let downloaded = 0;
  let total = 0;
  await update.downloadAndInstall((event) => {
    if (event.event === "Started") {
      total = event.data.contentLength ?? 0;
      onProgress?.(0);
    } else if (event.event === "Progress") {
      downloaded += event.data.chunkLength;
      onProgress?.(total ? Math.min(100, Math.round((downloaded / total) * 100)) : null);
    } else if (event.event === "Finished") {
      onProgress?.(100);
    }
  });
  await relaunch();
}
