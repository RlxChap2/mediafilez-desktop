export type DownloadMode = "video" | "image" | "audio" | "muted-video";

export type AudioFormat = "best" | "mp3" | "m4a" | "opus" | "wav";

export type AudioBitrate = "best" | "320" | "256" | "128" | "96" | "64";

export type VideoQuality =
  | "best"
  | "2160p"
  | "1440p"
  | "1080p"
  | "720p"
  | "480p"
  | "360p";

export type ProviderKind =
  | "direct"
  | "api"
  | "public-api"
  | "yt-dlp"
  | "gallery-dl"
  | "instagram-proxy"
  | "html";

export type JobStatus =
  | "queued"
  | "probing"
  | "downloading"
  | "converting"
  | "complete"
  | "failed"
  | "cancelled";

export type DownloadOptions = {
  mode: DownloadMode;
  audioFormat: AudioFormat;
  audioBitrate: AudioBitrate;
  providerOrder: ProviderKind[];
};

export type JobItem = {
  id: string;
  url: string;
  title: string;
  mode: DownloadMode;
  status: JobStatus;
  progress: number;
  speed?: string;
  eta?: string;
  provider?: ProviderKind;
  detail?: string;
  filePath?: string;
  error?: string;
  errorCode?: string;
};

export type JobUpdate = {
  id: string;
  status: JobStatus;
  title?: string;
  provider?: ProviderKind;
  progress?: number;
  speed?: string;
  eta?: string;
  detail?: string;
  filePath?: string;
  error?: string;
  errorCode?: string;
};

export type SetupProgress = {
  tool: string;
  phase: "downloading" | "downloaded" | "verifying" | "extracting" | "ready" | "error";
  downloadedBytes: number;
  totalBytes: number | null;
  message: string;
};

export type ToolStatus = {
  name: string;
  available: boolean;
  path: string | null;
  managed: boolean;
  version: string | null;
  verified: boolean;
  sha256: string | null;
};

export type ToolsReport = {
  ytDlp: ToolStatus;
  galleryDl: ToolStatus;
  deno: ToolStatus;
  ffmpeg: ToolStatus;
  ready: boolean;
};

export type ToolUpdateStatus = {
  name: string;
  managed: boolean;
  updateAvailable: boolean | null;
  currentVersion: string | null;
};

export type ToolUpdatesReport = {
  ytDlp: ToolUpdateStatus;
  galleryDl: ToolUpdateStatus;
  deno: ToolUpdateStatus;
  ffmpeg: ToolUpdateStatus;
  updatesAvailable: boolean;
};

export type AppUpdateInfo = {
  currentVersion: string;
  version: string;
  body: string | null;
  date: string | null;
};

export type ApiProviderSettings = {
  enabled: boolean;
  baseUrl: string;
  authType: "none" | "api-key" | "bearer";
  token: string;
  timeoutSeconds: number;
};

export type AppSettings = {
  outputFolder: string;
  concurrency: number;
  cookiesFromBrowser: boolean;
  cookieBrowser: "edge" | "chrome" | "firefox" | "brave" | "vivaldi" | "opera";
  cookieBrowserProfile: string;
  cookieFile: string;
  ffmpegPath: string;
  apiProvider: ApiProviderSettings;
  communityFallback: boolean;
  instagramProxyFallback: boolean;
};

export type PreflightCheck = {
  id: string;
  label: string;
  status: "pass" | "warn" | "fail";
  detail: string;
};
