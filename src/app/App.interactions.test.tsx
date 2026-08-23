import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";

import App from "./App";

beforeEach(() => {
  window.localStorage.clear();
});

describe("MediaFilez Desktop interactions", () => {
  it("enables download only for a complete link", async () => {
    const user = userEvent.setup();
    render(<App />);

    const download = screen.getByRole("button", { name: "Download video" });
    expect(download).toBeDisabled();
    await user.type(screen.getByLabelText("Media link"), "https://example.com/watch/demo");
    expect(download).toBeEnabled();

    await user.click(screen.getByRole("button", { name: /Audio/ }));
    expect(screen.getByRole("button", { name: "Download audio" })).toBeEnabled();

    await user.click(screen.getByRole("button", { name: /Image/ }));
    expect(screen.getByRole("button", { name: "Download image" })).toBeEnabled();
  });

  it("shows audio format and gates bitrate behind advanced options", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: /Audio/ }));
    expect(screen.getByLabelText("Audio format")).toBeVisible();
    expect(screen.queryByLabelText("Bitrate")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("switch", { name: "Show advanced options" }));
    await user.click(screen.getByRole("button", { name: "Close settings" }));

    expect(await screen.findByLabelText("Audio format")).toBeVisible();
    expect(screen.getByLabelText("Bitrate")).toBeVisible();
  });

  it("runs a download to completion through the fallback engine", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.type(screen.getByLabelText("Media link"), "https://example.com/watch/demo");
    await user.click(screen.getByRole("button", { name: "Download video" }));

    expect(await screen.findByText("https://example.com/watch/demo")).toBeVisible();
    await waitFor(() => expect(screen.getByText("Complete")).toBeVisible(), {
      timeout: 4000,
    });
    expect(screen.getAllByText(/Demo media\.mp4$/).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: /Show Demo media in folder/ })).toBeVisible();
  });

  it("saves a self-hosted fallback without a success toast", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("switch", { name: "Show advanced options" }));
    await user.type(
      await screen.findByPlaceholderText(/https:\/\/cobalt-one\.example/),
      "https://media-one.example.net\nhttps://media-two.example.net",
    );
    await user.click(screen.getByRole("button", { name: "Save settings" }));

    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: "Settings" })).not.toBeInTheDocument();
    });
    expect(screen.queryByText("Settings saved")).not.toBeInTheDocument();
  });

  it("updates the chosen output folder", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Choose folder" }));
    expect((await screen.findAllByText("Downloads\\MediaFilez")).length).toBeGreaterThan(0);
  });
});
