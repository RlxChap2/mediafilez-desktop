import { chromium } from "playwright";
import { createServer } from "vite";

const externalUrl = process.env.RSDOWNIT_TEST_URL;
const server = externalUrl
  ? null
  : await createServer({ server: { port: 4173, strictPort: false, hmr: false } });
if (server) await server.listen();
const url = externalUrl ?? server.resolvedUrls.local[0];

let browser;
let failed = false;

try {
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1180, height: 800 } });
  const browserErrors = [];
  page.on("console", (message) => {
    if (message.type() === "error") browserErrors.push(message.text());
  });
  page.on("pageerror", (error) => browserErrors.push(error.message));
  await page.goto(url);
  await page.waitForLoadState("networkidle");

  await expectVisible(page.getByRole("heading", { name: "MediaFilez" }), "brand");
  await expectVisible(
    page.getByRole("heading", { name: "Download media" }),
    "download heading",
  );
  await expectVisible(page.getByLabel("Media link"), "URL input");
  await expectVisible(page.getByText("No downloads yet."), "empty queue");
  await page.screenshot({ path: "docs/screenshot.png", fullPage: true });

  await page.getByLabel("Media link").fill("https://example.com/watch/demo");
  await page.getByRole("button", { name: "Download video" }).click();
  await expectVisible(page.getByText("https://example.com/watch/demo"), "queued job");
  await expectVisible(page.getByText("Complete"), "completed status");

  const wasDark = await page.evaluate(() => document.documentElement.classList.contains("dark"));
  await page
    .getByRole("button", { name: wasDark ? "Switch to light mode" : "Switch to dark mode" })
    .click();
  const isDark = await page.evaluate(() => document.documentElement.classList.contains("dark"));
  if (isDark === wasDark) throw new Error("theme toggle did not switch themes");

  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expectVisible(page.getByRole("dialog", { name: "Settings" }), "settings dialog");
  await expectVisible(page.getByText("Download folder"), "download folder");
  await expectVisible(page.getByRole("switch", { name: "Auto-start downloads" }), "auto-start");
  await page.getByRole("button", { name: "Check updates" }).click();
  await expectVisible(page.getByRole("dialog", { name: "You are up to date" }), "current version prompt");
  await page.getByRole("button", { name: "Done" }).click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("switch", { name: "Show advanced options" }).click();
  await expectVisible(page.getByText("Site sign-in"), "site sign-in settings");
  await page.getByRole("switch", { name: "Use browser session" }).click();
  const selectedBrowser = await page.locator(".inset-field select").first().inputValue();
  if (selectedBrowser !== "firefox") throw new Error("Firefox is not the default sign-in browser");
  await page.getByRole("button", { name: "Choose file" }).click();
  await expectVisible(page.getByText("C:\\Users\\Demo\\cookies.txt"), "cookie file selection");
  await expectVisible(page.getByText("Authorized Cobalt pool"), "authorized Cobalt pool");
  await expectVisible(page.getByRole("switch", { name: "Community fallback servers" }), "community fallback");
  await expectVisible(page.getByRole("switch", { name: "Instagram embed fallback" }), "Instagram fallback");
  await page.getByRole("button", { name: "Close settings" }).click();

  for (const width of [320, 375, 414, 768, 820, 1024, 1180, 1440]) {
    await page.setViewportSize({ width, height: width === 820 ? 1180 : 900 });
    const horizontalOverflow = await page.evaluate(
      () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
    );
    if (horizontalOverflow) throw new Error(`horizontal overflow at ${width}px`);
    await expectVisible(page.getByLabel("Media link"), `responsive input ${width}px`);

    const layout = await page.evaluate(() => {
      const controls = document.querySelector(".download-options")?.getBoundingClientRect();
      const queue = document.querySelector(".activity-pane")?.getBoundingClientRect();
      return controls && queue
        ? { controlsRight: controls.right, controlsBottom: controls.bottom, queueLeft: queue.left, queueTop: queue.top }
        : null;
    });
    if (!layout) throw new Error(`download layout missing at ${width}px`);
    if (width >= 1180 && layout.queueLeft <= layout.controlsRight) {
      throw new Error(`queue is not on the right at ${width}px`);
    }
    if (width <= 1024 && layout.queueTop < layout.controlsBottom) {
      throw new Error(`queue overlaps controls at ${width}px`);
    }
  }

  await page.setViewportSize({ width: 820, height: 1180 });
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const dialogBounds = await page.getByRole("dialog", { name: "Settings" }).locator(".dialog-panel").boundingBox();
  if (!dialogBounds || dialogBounds.x < 0 || dialogBounds.x + dialogBounds.width > 820) {
    throw new Error("settings dialog does not fit the iPad viewport");
  }
  await page.getByRole("button", { name: "Close settings" }).click();

  if (browserErrors.length) {
    throw new Error(`browser console errors: ${browserErrors.join(" | ")}`);
  }

  const updatePage = await browser.newPage({ viewport: { width: 390, height: 844 } });
  await updatePage.goto(`${url}?mockUpdate=1`);
  await expectVisible(
    updatePage.getByRole("dialog", { name: "Update available" }),
    "signed update prompt",
  );
  await expectVisible(updatePage.getByRole("button", { name: "Update now" }), "update action");
  await updatePage.getByRole("button", { name: "Later" }).click();
  await updatePage.close();

  console.log("smoke test passed");
} catch (error) {
  failed = true;
  console.error("smoke test failed:", error.message ?? error);
} finally {
  await browser?.close();
  await server?.close();
}

process.exit(failed ? 1 : 0);

async function expectVisible(locator, label) {
  await locator.first().waitFor({ state: "visible", timeout: 10_000 });
  console.log(`ok: ${label}`);
}
