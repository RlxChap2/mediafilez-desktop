import type {
  AppSettings,
  AppUpdateInfo,
  AudioBitrate,
  AudioFormat,
  DownloadMode,
  JobUpdate,
  SetupProgress,
  ToolsReport,
  ToolUpdatesReport,
  VideoQuality,
} from "./types";

export type StartRequest = {
  url: string;
  outputDir: string;
  mode: DownloadMode;
  videoQuality: VideoQuality;
  audioFormat: AudioFormat;
  audioBitrate: AudioBitrate;
};

export type TauriClient = {
  isDesktop: boolean;
  chooseOutputFolder: () => Promise<string | null>;
  chooseCookieFile: () => Promise<string | null>;
  getDefaultOutputDir: () => Promise<string>;
  getSettings: () => Promise<AppSettings>;
  saveSettings: (settings: AppSettings) => Promise<AppSettings>;
  checkTools: () => Promise<ToolsReport>;
  setupTools: () => Promise<ToolsReport>;
  checkToolUpdates: () => Promise<ToolUpdatesReport>;
  checkAppUpdate: () => Promise<AppUpdateInfo | null>;
  installAppUpdate: (onProgress: (percent: number | null) => void) => Promise<void>;
  startDownload: (request: StartRequest) => Promise<string>;
  cancelDownload: (id: string) => Promise<boolean>;
  onDownloadUpdate: (callback: (update: JobUpdate) => void) => Promise<() => void>;
  onSetupProgress: (callback: (progress: SetupProgress) => void) => Promise<() => void>;
  openFile: (path: string) => Promise<void>;
  showInFolder: (path: string) => Promise<void>;
  openUrl: (url: string) => Promise<void>;
};

const FALLBACK_FOLDER = "Downloads";
const FALLBACK_PICKED_FOLDER = "Downloads\\MediaFilez";
const UPDATE_TIMEOUT_MS = 20_000;

const fallbackSettings: AppSettings = {
  outputFolder: "",
  concurrency: 2,
  cookiesFromBrowser: false,
  cookieBrowser: "firefox",
  cookieBrowserProfile: "",
  cookieFile: "",
  ffmpegPath: "",
  apiProvider: {
    enabled: false,
    baseUrl: "",
    authType: "none",
    token: "",
    timeoutSeconds: 20,
  },
  communityFallback: false,
  instagramProxyFallback: false,
};

const fallbackTools: ToolsReport = {
  ytDlp: {
    name: "yt-dlp",
    available: true,
    path: "yt-dlp",
    managed: true,
    version: "browser demo",
    verified: true,
    sha256: "browser-demo",
  },
  galleryDl: {
    name: "gallery-dl",
    available: true,
    path: "gallery-dl",
    managed: true,
    version: "browser demo",
    verified: true,
    sha256: "browser-demo",
  },
  deno: {
    name: "Deno",
    available: true,
    path: "deno",
    managed: true,
    version: "browser demo",
    verified: true,
    sha256: "browser-demo",
  },
  ffmpeg: {
    name: "ffmpeg",
    available: true,
    path: "ffmpeg",
    managed: false,
    version: "browser demo",
    verified: true,
    sha256: "browser-demo",
  },
  ready: true,
};

const fallbackToolUpdates: ToolUpdatesReport = {
  ytDlp: {
    name: "yt-dlp",
    managed: true,
    updateAvailable: false,
    currentVersion: "browser demo",
  },
  galleryDl: {
    name: "gallery-dl",
    managed: true,
    updateAvailable: false,
    currentVersion: "browser demo",
  },
  deno: {
    name: "Deno",
    managed: true,
    updateAvailable: false,
    currentVersion: "browser demo",
  },
  ffmpeg: {
    name: "FFmpeg",
    managed: false,
    updateAvailable: null,
    currentVersion: "browser demo",
  },
  updatesAvailable: false,
};

/**
 * Browser client: simulates the download engine so the UI can be developed
 * and tested outside the desktop shell.
 */
function createBrowserClient(): TauriClient {
  const listeners = new Set<(update: JobUpdate) => void>();
  const timers = new Map<string, ReturnType<typeof setInterval>>();
  let stored = { ...fallbackSettings };
  let counter = 0;

  const emit = (update: JobUpdate) => {
    listeners.forEach((listener) => listener(update));
  };

  return {
    isDesktop: false,
    async chooseOutputFolder() {
      return FALLBACK_PICKED_FOLDER;
    },
    async chooseCookieFile() {
      return "C:\\Users\\Demo\\cookies.txt";
    },
    async getDefaultOutputDir() {
      return stored.outputFolder || FALLBACK_FOLDER;
    },
    async getSettings() {
      return { ...stored };
    },
    async saveSettings(settings) {
      stored = { ...settings };
      return { ...stored };
    },
    async checkTools() {
      return fallbackTools;
    },
    async setupTools() {
      return fallbackTools;
    },
    async checkToolUpdates() {
      return fallbackToolUpdates;
    },
    async checkAppUpdate() {
      return new URLSearchParams(window.location.search).has("mockUpdate")
        ? {
            currentVersion: "0.1.0",
            version: "0.2.0",
            body: "A signed test update is ready.",
            date: null,
          }
        : null;
    },
    async installAppUpdate(onProgress) {
      onProgress(100);
    },
    async startDownload(request) {
      counter += 1;
      const id = `demo-${counter}`;
      const extension = request.mode === "audio" ? "m4a" : request.mode === "image" ? "jpg" : "mp4";
      let progress = 0;

      setTimeout(() => {
        emit({ id, status: "probing", detail: "Resolving media" });
      }, 0);
      const timer = setInterval(() => {
        progress += 12;
        if (progress < 100) {
          emit({
            id,
            status: "downloading",
            title: "Demo media",
            provider: "yt-dlp",
            progress,
            speed: "4.2 MB/s",
            eta: `${Math.max(1, Math.round((100 - progress) / 12))}s`,
          });
        } else {
          clearInterval(timer);
          timers.delete(id);
          emit({
            id,
            status: "complete",
            title: "Demo media",
            provider: "yt-dlp",
            progress: 100,
            filePath: `${request.outputDir || FALLBACK_FOLDER}\\Demo media.${extension}`,
          });
        }
      }, 90);
      timers.set(id, timer);
      return id;
    },
    async cancelDownload(id) {
      const timer = timers.get(id);
      if (timer) {
        clearInterval(timer);
        timers.delete(id);
        emit({ id, status: "cancelled" });
        return true;
      }
      return false;
    },
    async onDownloadUpdate(callback) {
      listeners.add(callback);
      return () => listeners.delete(callback);
    },
    async onSetupProgress() {
      return () => undefined;
    },
    async openFile() {},
    async showInFolder() {},
    async openUrl(target) {
      window.open(target, "_blank", "noopener");
    },
  };
}

export const browserFallbackClient: TauriClient = createBrowserClient();

export async function createTauriClient(): Promise<TauriClient> {
  if (!("__TAURI_INTERNALS__" in window)) {
    return browserFallbackClient;
  }

  const [{ invoke }, { listen }, { open }] = await Promise.all([
    import("@tauri-apps/api/core"),
    import("@tauri-apps/api/event"),
    import("@tauri-apps/plugin-dialog"),
  ]);
  let pendingAppUpdate: Awaited<ReturnType<(typeof import("@tauri-apps/plugin-updater"))["check"]>> = null;

  return {
    isDesktop: true,
    async chooseOutputFolder() {
      const selected = await open({ directory: true, multiple: false });
      return typeof selected === "string" ? selected : null;
    },
    async chooseCookieFile() {
      const selected = await open({
        directory: false,
        multiple: false,
        filters: [{ name: "Netscape cookie file", extensions: ["txt"] }],
      });
      return typeof selected === "string" ? selected : null;
    },
    getDefaultOutputDir() {
      return invoke("get_default_output_dir");
    },
    getSettings() {
      return invoke("get_app_settings");
    },
    saveSettings(settings) {
      return invoke("save_app_settings", { settings });
    },
    checkTools() {
      return invoke("check_tools");
    },
    setupTools() {
      return invoke("setup_tools");
    },
    checkToolUpdates() {
      return invoke("check_tool_updates");
    },
    async checkAppUpdate() {
      const { check } = await import("@tauri-apps/plugin-updater");
      if (pendingAppUpdate) {
        await pendingAppUpdate.close().catch(() => undefined);
        pendingAppUpdate = null;
      }
      pendingAppUpdate = await check({ timeout: UPDATE_TIMEOUT_MS });
      if (!pendingAppUpdate) return null;
      return {
        currentVersion: pendingAppUpdate.currentVersion,
        version: pendingAppUpdate.version,
        body: pendingAppUpdate.body ?? null,
        date: pendingAppUpdate.date ?? null,
      };
    },
    async installAppUpdate(onProgress) {
      if (!pendingAppUpdate) {
        throw new Error("No app update is ready to install.");
      }
      let downloaded = 0;
      let total: number | undefined;
      await pendingAppUpdate.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength;
        if (event.event === "Progress") downloaded += event.data.chunkLength;
        if (event.event === "Finished") onProgress(100);
        if (event.event === "Finished") return;
        onProgress(total ? Math.min(100, Math.round((downloaded / total) * 100)) : null);
      });
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    },
    startDownload(request) {
      return invoke("start_download", { request });
    },
    cancelDownload(id) {
      return invoke("cancel_download", { id });
    },
    async onDownloadUpdate(callback) {
      return listen<JobUpdate>("download://update", (event) => callback(event.payload));
    },
    async onSetupProgress(callback) {
      return listen<SetupProgress>("setup://progress", (event) => callback(event.payload));
    },
    openFile(path) {
      return invoke("open_file", { path });
    },
    showInFolder(path) {
      return invoke("show_in_folder", { path });
    },
    openUrl(target) {
      return invoke("open_link", { url: target });
    },
  };
}
