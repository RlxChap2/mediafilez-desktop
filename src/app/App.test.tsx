import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";

import App from "./App";

beforeEach(() => {
  window.localStorage.clear();
  window.history.replaceState({}, "", "/");
});

describe("MediaFilez Desktop app shell", () => {
  it("renders the focused downloader workbench", async () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "MediaFilez" })).toBeVisible();
    expect(
      screen.getByRole("heading", { name: "Download media" }),
    ).toBeVisible();
    expect(screen.getByLabelText("Media link")).toBeVisible();
    expect(screen.getByRole("button", { name: "Paste" })).toBeVisible();
    expect(screen.getByRole("group", { name: "Download mode" })).toBeVisible();
    expect(screen.getByRole("button", { name: /Video/ })).toBeVisible();
    expect(screen.getByRole("button", { name: /Image/ })).toBeVisible();
    expect(screen.getByRole("button", { name: /Audio/ })).toBeVisible();
    expect(screen.queryByRole("button", { name: /Thumbnail|Muted/ })).not.toBeInTheDocument();
    expect(screen.getByText("No downloads yet.")).toBeVisible();
    expect((await screen.findAllByText("Downloads")).length).toBeGreaterThan(0);
    expect(await screen.findByText("Engine ready")).toBeVisible();
  });

  it("keeps network and session controls behind advanced settings", async () => {
    const user = userEvent.setup();
    render(<App />);

    expect(screen.queryByText("Authorized Cobalt pool")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Settings" }));

    expect(screen.getByRole("dialog", { name: "Settings" })).toBeVisible();
    expect(screen.getByText("Download folder")).toBeVisible();
    expect(screen.getByRole("switch", { name: "Auto-start downloads" })).toBeVisible();
    expect(screen.getByRole("switch", { name: "Show advanced options" })).toBeVisible();
    expect(screen.queryByRole("switch", { name: "Community fallback servers" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("switch", { name: "Show advanced options" }));
    expect(await screen.findByText("Authorized Cobalt pool")).toBeVisible();
    expect(screen.getByPlaceholderText(/https:\/\/cobalt-one\.example/)).toBeVisible();
    expect(screen.getByRole("switch", { name: "Use browser session" })).toBeVisible();
    expect(screen.getByRole("switch", { name: "Community fallback servers" })).toBeVisible();
    expect(screen.getByRole("switch", { name: "Instagram embed fallback" })).toBeVisible();

    await user.click(screen.getByRole("switch", { name: "Use browser session" }));
    expect(screen.getByRole("option", { name: "Mozilla Firefox (recommended)" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Choose file" }));
    expect(await screen.findByText("C:\\Users\\Demo\\cookies.txt")).toBeVisible();
  });

  it("switches between light and dark themes", async () => {
    const user = userEvent.setup();
    render(<App />);

    const initialDark = document.documentElement.classList.contains("dark");
    const toggleName = initialDark ? "Switch to light mode" : "Switch to dark mode";
    await user.click(screen.getByRole("button", { name: toggleName }));
    expect(document.documentElement.classList.contains("dark")).toBe(!initialDark);
  });

  it("offers a discovered app update now or later", async () => {
    const user = userEvent.setup();
    window.history.replaceState({}, "", "/?mockUpdate=1");
    render(<App />);

    const dialog = await screen.findByRole("dialog", { name: "Update available" });
    expect(within(dialog).getByText("0.1.0 to 0.2.0")).toBeVisible();
    expect(within(dialog).getByRole("button", { name: "Update now" })).toBeVisible();
    expect(within(dialog).getByRole("button", { name: "Later" })).toBeVisible();

    await user.click(within(dialog).getByRole("button", { name: "Later" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: "Update available" })).not.toBeInTheDocument();
    });
  });

  it("completes the browser update flow", async () => {
    const user = userEvent.setup();
    window.history.replaceState({}, "", "/?mockUpdate=1");
    render(<App />);

    const dialog = await screen.findByRole("dialog", { name: "Update available" });
    await user.click(within(dialog).getByRole("button", { name: "Update now" }));

    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: "Update available" })).not.toBeInTheDocument();
    });
  });
});
