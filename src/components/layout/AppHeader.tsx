import { Code2, Moon, Settings, ShieldCheck, Sun } from "lucide-react";

import type { ToolsReport } from "../../lib/types";
import type { Theme } from "../../lib/use-theme";

type AppHeaderProps = {
  theme: Theme;
  tools: ToolsReport | null;
  engineBusy: boolean;
  isDesktop: boolean;
  onToggleTheme: () => void;
  onOpenSettings: () => void;
  onOpenSource: () => void;
};

export function AppHeader({
  theme,
  tools,
  engineBusy,
  isDesktop,
  onToggleTheme,
  onOpenSettings,
  onOpenSource,
}: AppHeaderProps) {
  const engineLabel = engineBusy
    ? "Preparing engine"
    : tools?.ready
      ? "Engine ready"
      : isDesktop
        ? "Installs on first use"
        : "Browser preview";

  return (
    <header className="app-header">
      <div className="brand-lockup">
        <img src="/logo.svg" alt="" className="brand-mark" width="52" height="52" draggable={false} />
        <div>
          <h1>MediaFilez</h1>
          <p>Desktop downloader</p>
        </div>
      </div>

      <div className="header-actions">
        <div className="engine-status" role="status">
          <ShieldCheck aria-hidden="true" />
          <span>{engineLabel}</span>
        </div>
        <button
          type="button"
          className="icon-button"
          aria-label={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
          onClick={onToggleTheme}
        >
          {theme === "dark" ? <Sun aria-hidden="true" /> : <Moon aria-hidden="true" />}
        </button>
        <button type="button" className="icon-button" aria-label="Settings" onClick={onOpenSettings}>
          <Settings aria-hidden="true" />
        </button>
        <button type="button" className="icon-button" aria-label="View source on GitHub" onClick={onOpenSource}>
          <Code2 aria-hidden="true" />
        </button>
      </div>
    </header>
  );
}
