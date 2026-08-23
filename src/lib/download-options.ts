import type {
  AudioBitrate,
  AudioFormat,
  DownloadOptions,
  ProviderKind,
  VideoQuality,
} from "./types";

export const PROVIDER_ORDER: ProviderKind[] = [
  "direct",
  "api",
  "yt-dlp",
  "gallery-dl",
  "instagram-proxy",
  "public-api",
  "html",
];

export const DEFAULT_DOWNLOAD_OPTIONS: DownloadOptions = {
  mode: "video",
  audioFormat: "best",
  audioBitrate: "best",
  providerOrder: PROVIDER_ORDER,
};

export const AUDIO_FORMATS: Array<{ value: AudioFormat; label: string }> = [
  { value: "best", label: "Best native" },
  { value: "mp3", label: "MP3" },
  { value: "m4a", label: "M4A" },
  { value: "opus", label: "Opus" },
  { value: "wav", label: "WAV" },
];

export const AUDIO_BITRATES: Array<{ value: AudioBitrate; label: string }> = [
  { value: "best", label: "Best" },
  { value: "320", label: "320 kbps" },
  { value: "256", label: "256 kbps" },
  { value: "128", label: "128 kbps" },
  { value: "96", label: "96 kbps" },
  { value: "64", label: "64 kbps" },
];

export const VIDEO_QUALITIES: Array<{ value: VideoQuality; label: string }> = [
  { value: "best", label: "Best available" },
  { value: "2160p", label: "Up to 2160p" },
  { value: "1440p", label: "Up to 1440p" },
  { value: "1080p", label: "Up to 1080p" },
  { value: "720p", label: "Up to 720p" },
  { value: "480p", label: "Up to 480p" },
  { value: "360p", label: "Up to 360p" },
];

export function requiresFfmpegForAudio(format: AudioFormat) {
  return format === "mp3" || format === "wav";
}

export function getProviderLabel(provider: ProviderKind) {
  switch (provider) {
    case "direct":
      return "Direct media";
    case "api":
      return "Your API";
    case "public-api":
      return "Community server";
    case "yt-dlp":
      return "yt-dlp";
    case "gallery-dl":
      return "gallery-dl";
    case "instagram-proxy":
      return "Instagram embed fallback";
    case "html":
      return "HTML media scan";
  }
}
