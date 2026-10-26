import { defineStore, acceptHMRUpdate } from "pinia";
import { ref } from "vue";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { useToast } from "../composables/useToast";

const CHECK_INTERVAL_MS = 60 * 60 * 1000; // 1 hour
const RESURFACE_MS = 5 * 60 * 60 * 1000; // 5 hours

export const useUpdateStore = defineStore("update", () => {
  const updateAvailable = ref(false);
  const latestVersion = ref("");
  const releaseNotes = ref<string | null>(null);
  const dismissed = ref(false);

  // Download/install state
  const installing = ref(false);
  const downloadProgress = ref(0); // 0-100
  const downloadedBytes = ref(0);
  const totalBytes = ref(0);
  const installError = ref<string | null>(null);

  let intervalId: ReturnType<typeof setInterval> | null = null;
  let resurfaceId: ReturnType<typeof setTimeout> | null = null;
  let pendingUpdate: Update | null = null;

  async function checkForUpdate(showToast = true) {
    try {
      const update = await check();
      if (update) {
        const isNew = !updateAvailable.value;
        updateAvailable.value = true;
        latestVersion.value = update.version;
        releaseNotes.value = update.body ?? null;
        pendingUpdate = update;

        if (isNew && showToast) {
          const { info: toastInfo } = useToast();
          toastInfo(`Glogger v${update.version} is available!`);
        }
      }
    } catch {
      // Silently ignore — no network, rate-limited, etc.
    }
  }

  async function downloadAndInstall() {
    if (!pendingUpdate) return;

    installing.value = true;
    installError.value = null;
    downloadProgress.value = 0;
    downloadedBytes.value = 0;
    totalBytes.value = 0;

    try {
      await pendingUpdate.downloadAndInstall((event) => {
        if (event.event === "Started") {
          totalBytes.value = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloadedBytes.value += event.data.chunkLength;
          if (totalBytes.value > 0) {
            downloadProgress.value = Math.round(
              (downloadedBytes.value / totalBytes.value) * 100
            );
          }
        } else if (event.event === "Finished") {
          downloadProgress.value = 100;
        }
      });

      // Restart the app to apply the update
      await relaunch();
    } catch (e: any) {
      installError.value = e.toString();
      installing.value = false;
    }
  }

  function startPolling() {
    // Guard against double-start (HMR re-execution + explicit startup call):
    // a second ticker would double-fire update checks.
    if (intervalId) return
    // Check once shortly after startup (give the app a moment to settle)
    setTimeout(() => checkForUpdate(), 5000);
    intervalId = setInterval(() => checkForUpdate(), CHECK_INTERVAL_MS);
  }

  function stopPolling() {
    if (intervalId) {
      clearInterval(intervalId);
      intervalId = null;
    }
  }

  function dismiss() {
    dismissed.value = true;
    if (resurfaceId) clearTimeout(resurfaceId);
    resurfaceId = setTimeout(() => {
      dismissed.value = false;
    }, RESURFACE_MS);
  }

  return {
    updateAvailable,
    latestVersion,
    releaseNotes,
    dismissed,
    installing,
    downloadProgress,
    downloadedBytes,
    totalBytes,
    installError,
    checkForUpdate,
    downloadAndInstall,
    startPolling,
    stopPolling,
    dismiss,
  };
});

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useUpdateStore, import.meta.hot))
}
