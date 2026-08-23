import { describe, expect, it } from "vitest";

import { browserFallbackClient } from "./tauri-client";
import type { JobUpdate } from "./types";

describe("browser fallback client", () => {
  it("chooses a deterministic output folder in browser tests", async () => {
    await expect(browserFallbackClient.chooseOutputFolder()).resolves.toBe(
      "Downloads\\MediaFilez",
    );
  });

  it("reports the engine as ready so the UI needs no setup", async () => {
    const report = await browserFallbackClient.checkTools();
    expect(report.ready).toBe(true);
    expect(report.ytDlp.available).toBe(true);
    expect(report.galleryDl.available).toBe(true);
    expect(report.deno.available).toBe(true);
  });

  it("chooses a Netscape cookie file in browser tests", async () => {
    await expect(browserFallbackClient.chooseCookieFile()).resolves.toBe(
      "C:\\Users\\Demo\\cookies.txt",
    );
  });

  it("simulates a download through to completion", async () => {
    const updates: JobUpdate[] = [];
    const unlisten = await browserFallbackClient.onDownloadUpdate((update) => {
      updates.push(update);
    });

    const id = await browserFallbackClient.startDownload({
      url: "https://example.com/watch/demo",
      outputDir: "C:\\Users\\alfas\\Downloads",
      mode: "audio",
      videoQuality: "best",
      audioFormat: "best",
      audioBitrate: "best",
    });

    await new Promise((resolve) => setTimeout(resolve, 1500));
    unlisten();

    const statuses = updates.filter((update) => update.id === id).map((u) => u.status);
    expect(statuses[0]).toBe("probing");
    expect(statuses).toContain("downloading");
    expect(statuses.at(-1)).toBe("complete");

    const done = updates.at(-1);
    expect(done?.filePath).toMatch(/Demo media\.m4a$/);
  });

  it("cancels a simulated download", async () => {
    const updates: JobUpdate[] = [];
    const unlisten = await browserFallbackClient.onDownloadUpdate((update) => {
      updates.push(update);
    });

    const id = await browserFallbackClient.startDownload({
      url: "https://example.com/watch/demo",
      outputDir: "",
      mode: "video",
      videoQuality: "best",
      audioFormat: "best",
      audioBitrate: "best",
    });
    const cancelled = await browserFallbackClient.cancelDownload(id);
    unlisten();

    expect(cancelled).toBe(true);
    expect(updates.filter((u) => u.id === id).at(-1)?.status).toBe("cancelled");
  });
});
