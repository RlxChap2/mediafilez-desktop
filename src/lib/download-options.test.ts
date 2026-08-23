import { describe, expect, it } from "vitest";

import {
  AUDIO_BITRATES,
  AUDIO_FORMATS,
  DEFAULT_DOWNLOAD_OPTIONS,
  PROVIDER_ORDER,
  getProviderLabel,
  requiresFfmpegForAudio,
} from "./download-options";

describe("download options", () => {
  it("defaults to video mode with best native audio settings ready", () => {
    expect(DEFAULT_DOWNLOAD_OPTIONS.mode).toBe("video");
    expect(DEFAULT_DOWNLOAD_OPTIONS.audioFormat).toBe("best");
    expect(DEFAULT_DOWNLOAD_OPTIONS.audioBitrate).toBe("best");
    expect(DEFAULT_DOWNLOAD_OPTIONS.providerOrder).toEqual(PROVIDER_ORDER);
  });

  it("requires ffmpeg only for audio conversion formats", () => {
    expect(requiresFfmpegForAudio("best")).toBe(false);
    expect(requiresFfmpegForAudio("m4a")).toBe(false);
    expect(requiresFfmpegForAudio("opus")).toBe(false);
    expect(requiresFfmpegForAudio("mp3")).toBe(true);
    expect(requiresFfmpegForAudio("wav")).toBe(true);
  });

  it("keeps the fast fallback provider labels in product order", () => {
    expect(PROVIDER_ORDER.map(getProviderLabel)).toEqual([
      "Direct media",
      "Your API",
      "yt-dlp",
      "gallery-dl",
      "Instagram embed fallback",
      "Community server",
      "HTML media scan",
    ]);
  });

  it("lists user-facing audio formats and bitrates", () => {
    expect(AUDIO_FORMATS.map((format) => format.value)).toEqual([
      "best",
      "mp3",
      "m4a",
      "opus",
      "wav",
    ]);
    expect(AUDIO_BITRATES.map((bitrate) => bitrate.value)).toContain("320");
  });
});
