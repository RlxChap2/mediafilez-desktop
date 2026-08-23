import { useCallback, useEffect, useState } from "react";
import { Toaster, toast } from "sonner";
import { version } from "../../package.json";

import { AppHeader } from "../components/layout/AppHeader";
import { DownloadComposer } from "../features/downloads/DownloadComposer";
import { DownloadList } from "../features/downloads/DownloadList";
import { SetupStatus } from "../features/downloads/SetupStatus";
import { SettingsDialog } from "../features/settings/SettingsDialog";
import { UpdateDialog } from "../features/updates/UpdateDialog";
import {
  browserFallbackClient,
  createTauriClient,
  type TauriClient,
} from "../lib/tauri-client";
import type {
  ApiProviderSettings,
  AppUpdateInfo,
  AudioBitrate,
  AudioFormat,
  DownloadMode,
  JobItem,
  JobUpdate,
  SetupProgress,
  ToolsReport,
  ToolUpdatesReport,
  VideoQuality,
} from "../lib/types";
import { useTheme } from "../lib/use-theme";
import { isProbablyUrl } from "../lib/utils";
import "../styles/app.css";

const REPO_URL = "https://github.com/RlxChap2/rsdownit";
const HISTORY_KEY = "mediafilez-desktop-history-v1";
const LEGACY_HISTORY_KEY = "rsdownit-history-v1";
const AUTOSTART_KEY = "mediafilez-desktop-autostart";
const ADVANCED_KEY = "mediafilez-desktop-advanced";

const initialApiProvider: ApiProviderSettings = {
  enabled: false,
  baseUrl: "",
  authType: "none",
  token: "",
  timeoutSeconds: 20,
};

function readPref(key: string, legacyKey: string, fallback: boolean) {
  const stored = window.localStorage.getItem(key) ?? window.localStorage.getItem(legacyKey);
  return stored === null ? fallback : stored === "true";
}

function readHistory(): JobItem[] {
  try {
    const value = JSON.parse(
      window.localStorage.getItem(HISTORY_KEY) ??
        window.localStorage.getItem(LEGACY_HISTORY_KEY) ??
        "[]",
    );
    if (!Array.isArray(value)) return [];
    return value
      .filter(
        (job): job is JobItem =>
          typeof job?.id === "string" &&
          typeof job?.url === "string" &&
          ["complete", "failed", "cancelled"].includes(job?.status),
      )
      .slice(0, 30);
  } catch {
    return [];
  }
}

async function lookupUpdates(target: TauriClient) {
  const [appResult, toolsResult] = await Promise.allSettled([
    target.checkAppUpdate(),
    target.checkToolUpdates(),
  ]);
  return {
    app: appResult.status === "fulfilled" ? appResult.value : null,
    tools: toolsResult.status === "fulfilled" ? toolsResult.value : null,
    appFailed: appResult.status === "rejected",
    toolsFailed: toolsResult.status === "rejected",
    failed: appResult.status === "rejected" && toolsResult.status === "rejected",
  };
}

function App() {
  const { theme, toggle: toggleTheme } = useTheme();
  const [client, setClient] = useState<TauriClient>(browserFallbackClient);
  const [url, setUrl] = useState("");
  const [mode, setMode] = useState<DownloadMode>("video");
  const [removeAudio, setRemoveAudio] = useState(false);
  const [videoQuality, setVideoQuality] = useState<VideoQuality>("best");
  const [audioFormat, setAudioFormat] = useState<AudioFormat>("best");
  const [audioBitrate, setAudioBitrate] = useState<AudioBitrate>("best");
  const [outputFolder, setOutputFolder] = useState("");
  const [jobs, setJobs] = useState<JobItem[]>(readHistory);
  const [apiProvider, setApiProvider] = useState(initialApiProvider);
  const [communityFallback, setCommunityFallback] = useState(false);
  const [instagramProxyFallback, setInstagramProxyFallback] = useState(false);
  const [concurrency, setConcurrency] = useState(2);
  const [cookiesFromBrowser, setCookiesFromBrowser] = useState(false);
  const [cookieBrowser, setCookieBrowser] = useState<
    "edge" | "chrome" | "firefox" | "brave" | "vivaldi" | "opera"
  >("firefox");
  const [cookieBrowserProfile, setCookieBrowserProfile] = useState("");
  const [cookieFile, setCookieFile] = useState("");
  const [autoStart, setAutoStart] = useState(() =>
    readPref(AUTOSTART_KEY, "rsdownit-autostart", false),
  );
  const [showAdvanced, setShowAdvanced] = useState(() =>
    readPref(ADVANCED_KEY, "rsdownit-advanced", false),
  );
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [tools, setTools] = useState<ToolsReport | null>(null);
  const [checkingTools, setCheckingTools] = useState(false);
  const [setupProgress, setSetupProgress] = useState<SetupProgress | null>(null);
  const [appUpdate, setAppUpdate] = useState<AppUpdateInfo | null>(null);
  const [toolUpdates, setToolUpdates] = useState<ToolUpdatesReport | null>(null);
  const [updateOpen, setUpdateOpen] = useState(false);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<number | null>(null);

  useEffect(() => {
    const finished = jobs
      .filter((job) => ["complete", "failed", "cancelled"].includes(job.status))
      .slice(0, 30);
    window.localStorage.setItem(HISTORY_KEY, JSON.stringify(finished));
  }, [jobs]);

  const applyDownloadUpdate = useCallback((update: JobUpdate) => {
    setJobs((previous) =>
      previous.map((job) =>
        job.id === update.id
          ? {
              ...job,
              status: update.status,
              title: update.title ?? job.title,
              provider: update.provider ?? job.provider,
              progress: update.progress ?? job.progress,
              speed: update.speed ?? job.speed,
              eta: update.eta ?? job.eta,
              detail: update.detail ?? job.detail,
              filePath: update.filePath ?? job.filePath,
              error: update.error ?? job.error,
              errorCode: update.errorCode ?? job.errorCode,
            }
          : job,
      ),
    );
    if (update.status === "failed") {
      toast.error("Download failed", { description: update.error });
    }
  }, []);

  useEffect(() => {
    let mounted = true;
    let unlistenDownload: (() => void) | undefined;
    let unlistenSetup: (() => void) | undefined;

    void (async () => {
      const tauriClient = await createTauriClient();
      if (!mounted) return;
      setClient(tauriClient);

      const updatesPromise = lookupUpdates(tauriClient);
      const [folderResult, settingsResult, reportResult] = await Promise.allSettled([
        tauriClient.getDefaultOutputDir(),
        tauriClient.getSettings(),
        tauriClient.checkTools(),
      ]);
      if (!mounted) return;
      if (folderResult.status === "fulfilled") setOutputFolder(folderResult.value);
      if (settingsResult.status === "fulfilled") {
        const settings = settingsResult.value;
        setApiProvider(settings.apiProvider);
        setCommunityFallback(settings.communityFallback);
        setInstagramProxyFallback(settings.instagramProxyFallback);
        setConcurrency(settings.concurrency);
        setCookiesFromBrowser(settings.cookiesFromBrowser);
        setCookieBrowser(settings.cookieBrowser);
        setCookieBrowserProfile(settings.cookieBrowserProfile);
        setCookieFile(settings.cookieFile);
      }
      if (reportResult.status === "fulfilled") setTools(reportResult.value);

      try {
        unlistenDownload = await tauriClient.onDownloadUpdate(applyDownloadUpdate);
        unlistenSetup = await tauriClient.onSetupProgress((progress) => {
          if (progress.phase === "ready" || progress.phase === "error") {
            setSetupProgress(null);
            void tauriClient
              .checkTools()
              .then(setTools)
              .catch((error) => {
                toast.error("Engine status could not be refreshed", {
                  description: String(error),
                });
              });
            if (progress.phase === "error") {
              toast.error("Engine setup failed", { description: progress.message });
            }
          } else {
            setSetupProgress(progress);
          }
        });
      } catch (error) {
        toast.error("App events could not be connected", { description: String(error) });
      }

      const updates = await updatesPromise;
      if (!mounted) return;
      setAppUpdate(updates.app);
      setToolUpdates(updates.tools);
      if (updates.app || updates.tools?.updatesAvailable) setUpdateOpen(true);
    })().catch((error) => {
      if (mounted) toast.error("App initialization failed", { description: String(error) });
    });

    return () => {
      mounted = false;
      unlistenDownload?.();
      unlistenSetup?.();
    };
  }, [applyDownloadUpdate]);

  const startDownload = useCallback(
    async (overrides?: { url?: string; mode?: DownloadMode }) => {
      const targetUrl = (overrides?.url ?? url).trim();
      if (!isProbablyUrl(targetUrl)) {
        toast.error("Enter a complete http or https media link.");
        return;
      }
      const jobMode = overrides?.mode ?? (mode === "video" && removeAudio ? "muted-video" : mode);

      try {
        const id = await client.startDownload({
          url: targetUrl,
          outputDir: outputFolder,
          mode: jobMode,
          videoQuality,
          audioFormat,
          audioBitrate,
        });
        setJobs((previous) => [
          { id, url: targetUrl, title: targetUrl, mode: jobMode, status: "queued", progress: 0 },
          ...previous,
        ]);
        if (!overrides?.url) setUrl("");
      } catch (error) {
        toast.error("Download could not start", { description: String(error) });
      }
    },
    [client, url, mode, removeAudio, outputFolder, videoQuality, audioFormat, audioBitrate],
  );

  async function handlePaste() {
    try {
      const text = (await navigator.clipboard.readText()).trim();
      if (!text) return;
      setUrl(text);
      if (autoStart && isProbablyUrl(text)) {
        void startDownload({ url: text });
        setUrl("");
      }
    } catch {
      toast.error("Clipboard access is unavailable. Paste with Ctrl+V.");
    }
  }

  async function chooseFolder() {
    const selected = await client.chooseOutputFolder();
    if (selected) setOutputFolder(selected);
  }

  async function chooseCookieFile() {
    const selected = await client.chooseCookieFile();
    if (!selected) return;
    setCookieFile(selected);
    setCookiesFromBrowser(false);
  }

  function openAuthenticationSettings() {
    updatePref(ADVANCED_KEY, true, setShowAdvanced);
    setSettingsOpen(true);
  }

  function retryJob(job: JobItem) {
    setJobs((previous) => previous.filter((item) => item.id !== job.id));
    void startDownload({ url: job.url, mode: job.mode });
  }

  function updatePref(key: string, value: boolean, setter: (value: boolean) => void) {
    setter(value);
    window.localStorage.setItem(key, String(value));
  }

  async function saveSettings() {
    try {
      await client.saveSettings({
        outputFolder,
        concurrency,
        cookiesFromBrowser,
        cookieBrowser,
        cookieBrowserProfile,
        cookieFile,
        ffmpegPath: "",
        apiProvider: { ...apiProvider, enabled: Boolean(apiProvider.baseUrl.trim()) },
        communityFallback,
        instagramProxyFallback,
      });
      setSettingsOpen(false);
    } catch (error) {
      toast.error("Settings could not be saved", { description: String(error) });
    }
  }

  async function repairTools() {
    setCheckingTools(true);
    try {
      setTools(await client.setupTools());
      setToolUpdates(await client.checkToolUpdates());
    } catch (error) {
      toast.error("Engine setup failed", { description: String(error) });
    } finally {
      setCheckingTools(false);
    }
  }

  async function checkForUpdates() {
    setCheckingUpdates(true);
    setSettingsOpen(false);
    const updates = await lookupUpdates(client);
    setAppUpdate(updates.app);
    setToolUpdates(updates.tools);
    setCheckingUpdates(false);
    if (updates.failed) {
      toast.error("Update check failed", { description: "Check your connection and try again." });
      return;
    }
    if (updates.appFailed && !updates.tools?.updatesAvailable) {
      toast.error("App update check failed", { description: "Check your connection and try again." });
      return;
    }
    if (updates.toolsFailed && !updates.app) {
      toast.error("Tool update check failed", { description: "Check your connection and try again." });
      return;
    }
    if (updates.appFailed || updates.toolsFailed) {
      toast.warning("Some updates could not be checked.");
    }
    setUpdateOpen(true);
  }

  async function installUpdates() {
    setInstallingUpdate(true);
    setUpdateProgress(null);
    try {
      if (toolUpdates?.updatesAvailable) {
        try {
          setTools(await client.setupTools());
          setToolUpdates(await client.checkToolUpdates());
        } catch (error) {
          if (!appUpdate) throw error;
          toast.error("Download tools could not be updated", { description: String(error) });
        }
      }
      if (appUpdate) {
        await client.installAppUpdate(setUpdateProgress);
        setUpdateOpen(false);
      } else {
        setUpdateOpen(false);
        toast.success("Download tools updated.");
      }
    } catch (error) {
      toast.error("Update failed", { description: String(error) });
    } finally {
      setInstallingUpdate(false);
    }
  }

  return (
    <div className="app-shell">
      <AppHeader
        theme={theme}
        tools={tools}
        engineBusy={setupProgress !== null}
        isDesktop={client.isDesktop}
        onToggleTheme={toggleTheme}
        onOpenSettings={() => setSettingsOpen(true)}
        onOpenSource={() => void client.openUrl(REPO_URL)}
      />

      <main className="workspace">
        <DownloadComposer
          url={url}
          mode={mode}
          videoQuality={videoQuality}
          audioFormat={audioFormat}
          audioBitrate={audioBitrate}
          outputFolder={outputFolder}
          showAdvanced={showAdvanced}
          removeAudio={removeAudio}
          onUrlChange={setUrl}
          onModeChange={(value) => {
            setMode(value);
            if (value !== "video") setRemoveAudio(false);
          }}
          onVideoQualityChange={setVideoQuality}
          onAudioFormatChange={setAudioFormat}
          onAudioBitrateChange={setAudioBitrate}
          onRemoveAudioChange={setRemoveAudio}
          onPaste={() => void handlePaste()}
          onChooseFolder={() => void chooseFolder()}
          onDownload={() => void startDownload()}
          activity={
            <div className="activity-pane">
              {setupProgress && <SetupStatus progress={setupProgress} />}
              <DownloadList
                jobs={jobs}
                onCancel={(id) => void client.cancelDownload(id)}
                onRetry={retryJob}
                onClearFinished={() =>
                  setJobs((previous) =>
                    previous.filter(
                      (job) => !["complete", "failed", "cancelled"].includes(job.status),
                    ),
                  )
                }
                onOpenFile={(path) => void client.openFile(path)}
                onShowInFolder={(path) => void client.showInFolder(path)}
                onOpenSettings={openAuthenticationSettings}
              />
            </div>
          }
        />
      </main>

      <footer className="app-footer">
        <span>MediaFilez Desktop {version}</span>
        <span>Local first · no telemetry · MIT</span>
      </footer>

      <SettingsDialog
        open={settingsOpen}
        outputFolder={outputFolder}
        autoStart={autoStart}
        showAdvanced={showAdvanced}
        communityFallback={communityFallback}
        instagramProxyFallback={instagramProxyFallback}
        concurrency={concurrency}
        cookiesFromBrowser={cookiesFromBrowser}
        cookieBrowser={cookieBrowser}
        cookieBrowserProfile={cookieBrowserProfile}
        cookieFile={cookieFile}
        apiProvider={apiProvider}
        tools={tools}
        checkingTools={checkingTools}
        checkingUpdates={checkingUpdates}
        onClose={() => setSettingsOpen(false)}
        onChooseFolder={() => void chooseFolder()}
        onAutoStartChange={(value) => updatePref(AUTOSTART_KEY, value, setAutoStart)}
        onShowAdvancedChange={(value) => updatePref(ADVANCED_KEY, value, setShowAdvanced)}
        onCommunityFallbackChange={setCommunityFallback}
        onInstagramProxyFallbackChange={setInstagramProxyFallback}
        onConcurrencyChange={setConcurrency}
        onCookiesFromBrowserChange={(value) => {
          setCookiesFromBrowser(value);
          if (value) setCookieFile("");
        }}
        onCookieBrowserChange={setCookieBrowser}
        onCookieBrowserProfileChange={setCookieBrowserProfile}
        onChooseCookieFile={() => void chooseCookieFile()}
        onRemoveCookieFile={() => setCookieFile("")}
        onApiProviderChange={setApiProvider}
        onSave={() => void saveSettings()}
        onRepairTools={() => void repairTools()}
        onCheckUpdates={() => void checkForUpdates()}
      />

      <UpdateDialog
        open={updateOpen}
        appUpdate={appUpdate}
        toolUpdates={toolUpdates}
        installing={installingUpdate}
        progress={updateProgress}
        onInstall={() => void installUpdates()}
        onClose={() => setUpdateOpen(false)}
      />

      <Toaster richColors theme={theme} position="bottom-right" />
    </div>
  );
}

export default App;
