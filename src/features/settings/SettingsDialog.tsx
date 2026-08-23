import {
  CircleAlert,
  FileText,
  FolderOpen,
  Gauge,
  KeyRound,
  RefreshCcw,
  ShieldCheck,
  X,
} from "lucide-react";
import { useEffect, useRef } from "react";

import type { ApiProviderSettings, AppSettings, ToolsReport } from "../../lib/types";
import { Input } from "../../components/ui/input";
import { Select } from "../../components/ui/select";
import { Switch } from "../../components/ui/switch";

type CookieBrowser = AppSettings["cookieBrowser"];

type SettingsDialogProps = {
  open: boolean;
  outputFolder: string;
  autoStart: boolean;
  showAdvanced: boolean;
  communityFallback: boolean;
  instagramProxyFallback: boolean;
  concurrency: number;
  cookiesFromBrowser: boolean;
  cookieBrowser: CookieBrowser;
  cookieBrowserProfile: string;
  cookieFile: string;
  apiProvider: ApiProviderSettings;
  tools: ToolsReport | null;
  checkingTools: boolean;
  checkingUpdates: boolean;
  onClose: () => void;
  onChooseFolder: () => void;
  onAutoStartChange: (value: boolean) => void;
  onShowAdvancedChange: (value: boolean) => void;
  onCommunityFallbackChange: (value: boolean) => void;
  onInstagramProxyFallbackChange: (value: boolean) => void;
  onConcurrencyChange: (value: number) => void;
  onCookiesFromBrowserChange: (value: boolean) => void;
  onCookieBrowserChange: (value: CookieBrowser) => void;
  onCookieBrowserProfileChange: (value: string) => void;
  onChooseCookieFile: () => void;
  onRemoveCookieFile: () => void;
  onApiProviderChange: (provider: ApiProviderSettings) => void;
  onSave: () => void;
  onRepairTools: () => void;
  onCheckUpdates: () => void;
};

function toolSummary(tool: ToolsReport["ytDlp"] | undefined) {
  if (!tool) return "Checking";
  if (!tool.available) return "Not installed yet";
  if (tool.verified) return "Ready · SHA-256 verified";
  return "Ready · provided by your system";
}

export function SettingsDialog({
  open,
  outputFolder,
  autoStart,
  showAdvanced,
  communityFallback,
  instagramProxyFallback,
  concurrency,
  cookiesFromBrowser,
  cookieBrowser,
  cookieBrowserProfile,
  cookieFile,
  apiProvider,
  tools,
  checkingTools,
  checkingUpdates,
  onClose,
  onChooseFolder,
  onAutoStartChange,
  onShowAdvancedChange,
  onCommunityFallbackChange,
  onInstagramProxyFallbackChange,
  onConcurrencyChange,
  onCookiesFromBrowserChange,
  onCookieBrowserChange,
  onCookieBrowserProfileChange,
  onChooseCookieFile,
  onRemoveCookieFile,
  onApiProviderChange,
  onSave,
  onRepairTools,
  onCheckUpdates,
}: SettingsDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;

    if (open && !dialog.open) {
      if (typeof dialog.showModal === "function") dialog.showModal();
      else dialog.setAttribute("open", "");
      window.setTimeout(() => {
        dialog.querySelector<HTMLElement>("button:not([aria-label='Close settings'])")?.focus();
      }, 0);
    } else if (!open && dialog.open) {
      if (typeof dialog.close === "function") dialog.close();
      else dialog.removeAttribute("open");
    }
  }, [open]);

  return (
    <dialog
      ref={dialogRef}
      className="settings-dialog"
      aria-labelledby="settings-title"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="dialog-panel">
        <header className="dialog-header">
          <div>
            <h2 id="settings-title">Settings</h2>
            <p>Download location, engine, privacy, and updates.</p>
          </div>
          <button type="button" className="icon-button quiet" aria-label="Close settings" onClick={onClose}>
            <X aria-hidden="true" />
          </button>
        </header>

        <div className="settings-body">
          <section className="settings-section">
            <div className="section-title-row">
              <div>
                <h3>Download folder</h3>
                <p>MediaFilez never overwrites an existing file.</p>
              </div>
              <button type="button" className="secondary-button" onClick={onChooseFolder}>
                <FolderOpen aria-hidden="true" />
                Choose
              </button>
            </div>
            <p className="path-display" title={outputFolder}>{outputFolder || "Downloads"}</p>
          </section>

          <section className="settings-section settings-grid">
            <label className="field-group compact">
              <span>Parallel fragments</span>
              <Select
                value={String(concurrency)}
                onChange={(event) => onConcurrencyChange(Number(event.currentTarget.value))}
              >
                <option value="1">1 · lowest resource use</option>
                <option value="2">2 · efficient</option>
                <option value="4">4 · balanced</option>
                <option value="8">8 · fast connection</option>
              </Select>
            </label>
            <div className="setting-note">
              <Gauge aria-hidden="true" />
              <p>Two works well on older PCs. Four or eight can speed up HLS and DASH downloads.</p>
            </div>
          </section>

          <section className="settings-section toggle-list">
            <label className="toggle-row">
              <span>
                <strong>Auto-start pasted links</strong>
                <small>Start only when clipboard text is a complete URL.</small>
              </span>
              <Switch checked={autoStart} onCheckedChange={onAutoStartChange} aria-label="Auto-start downloads" />
            </label>
            <label className="toggle-row">
              <span>
                <strong>Show advanced options</strong>
                <small>Show muted video, bitrate, browser session, and fallback controls.</small>
              </span>
              <Switch checked={showAdvanced} onCheckedChange={onShowAdvancedChange} aria-label="Show advanced options" />
            </label>
          </section>

          {showAdvanced && (
            <>
              <section className="settings-section">
                <div className="section-title-row auth-heading">
                  <div>
                    <h3>Site sign-in</h3>
                    <p>For private or age-restricted links you can already open in your browser.</p>
                  </div>
                  <KeyRound aria-hidden="true" />
                </div>
                <label className="toggle-row">
                  <span>
                    <strong>Use browser session</strong>
                    <small>Firefox is the most reliable choice on Windows. Cookies stay on this device.</small>
                  </span>
                  <Switch
                    checked={cookiesFromBrowser}
                    onCheckedChange={onCookiesFromBrowserChange}
                    aria-label="Use browser session"
                  />
                </label>
                {cookiesFromBrowser && (
                  <>
                    <div className="settings-grid two-fields inset-field">
                      <label className="field-group compact">
                        <span>Browser</span>
                        <Select
                          value={cookieBrowser}
                          onChange={(event) => onCookieBrowserChange(event.currentTarget.value as CookieBrowser)}
                        >
                          <option value="firefox">Mozilla Firefox (recommended)</option>
                          <option value="edge">Microsoft Edge</option>
                          <option value="chrome">Google Chrome</option>
                          <option value="brave">Brave</option>
                          <option value="vivaldi">Vivaldi</option>
                          <option value="opera">Opera</option>
                        </Select>
                      </label>
                      <label className="field-group compact">
                        <span>Profile (optional)</span>
                        <Input
                          value={cookieBrowserProfile}
                          onChange={(event) => onCookieBrowserProfileChange(event.currentTarget.value)}
                          placeholder="Default profile"
                          autoComplete="off"
                        />
                      </label>
                    </div>
                    {cookieBrowser !== "firefox" && (
                      <p className="warning-copy browser-warning">
                        <CircleAlert aria-hidden="true" />
                        Chromium may block cookie access on Windows. Close the browser first, or use Firefox or cookies.txt.
                      </p>
                    )}
                  </>
                )}

                <div className="cookie-file-row">
                  <FileText aria-hidden="true" />
                  <div>
                    <strong>Netscape cookies.txt</strong>
                    <small>Use an exported cookie file when direct browser access is blocked.</small>
                    {cookieFile && <span className="cookie-file-path" title={cookieFile}>{cookieFile}</span>}
                  </div>
                  <div className="cookie-file-actions">
                    <button type="button" className="secondary-button" onClick={onChooseCookieFile}>
                      Choose file
                    </button>
                    {cookieFile && (
                      <button type="button" className="text-button" onClick={onRemoveCookieFile}>
                        Remove
                      </button>
                    )}
                  </div>
                </div>
              </section>

              <section className="settings-section">
                <div className="section-title-row">
                  <div>
                    <h3>Authorized Cobalt pool</h3>
                    <p>Try self-hosted or owner-approved v11 instances in this order.</p>
                  </div>
                  <KeyRound aria-hidden="true" />
                </div>
                <label className="field-group compact">
                  <span>HTTPS endpoints</span>
                  <textarea
                    value={apiProvider.baseUrl}
                    onChange={(event) =>
                      onApiProviderChange({
                        ...apiProvider,
                        baseUrl: event.currentTarget.value,
                        enabled: Boolean(event.currentTarget.value.trim()),
                      })
                    }
                    placeholder={"https://cobalt-one.example\nhttps://cobalt-two.example"}
                    inputMode="url"
                  />
                  <small>One endpoint per line. The first valid media response wins.</small>
                </label>
                <div className="settings-grid two-fields">
                  <label className="field-group compact">
                    <span>Authentication</span>
                    <Select
                      value={apiProvider.authType}
                      onChange={(event) =>
                        onApiProviderChange({
                          ...apiProvider,
                          authType: event.currentTarget.value as ApiProviderSettings["authType"],
                        })
                      }
                    >
                      <option value="none">None</option>
                      <option value="api-key">Api-Key</option>
                      <option value="bearer">Bearer</option>
                    </Select>
                  </label>
                  <label className="field-group compact">
                    <span>Session token</span>
                    <Input
                      type="password"
                      value={apiProvider.token}
                      onChange={(event) =>
                        onApiProviderChange({ ...apiProvider, token: event.currentTarget.value })
                      }
                      placeholder="Not saved to disk"
                      autoComplete="off"
                    />
                  </label>
                </div>
              </section>

              <section className="settings-section warning-section">
                <label className="toggle-row">
                  <span>
                    <strong>Community Cobalt directory</strong>
                    <small>Legacy opt-in. Prefer endpoints whose owners gave you access.</small>
                  </span>
                  <Switch
                    checked={communityFallback}
                    onCheckedChange={onCommunityFallbackChange}
                    aria-label="Community fallback servers"
                  />
                </label>
                <p className="warning-copy">
                  <CircleAlert aria-hidden="true" />
                  Off by default. Use only servers whose owners permit third-party clients.
                </p>
                <label className="toggle-row fallback-toggle">
                  <span>
                    <strong>Instagram embed fallback</strong>
                    <small>Last attempt for public Instagram posts. Cookies and API tokens are never sent.</small>
                  </span>
                  <Switch
                    checked={instagramProxyFallback}
                    onCheckedChange={onInstagramProxyFallbackChange}
                    aria-label="Instagram embed fallback"
                  />
                </label>
                <p className="warning-copy">
                  <CircleAlert aria-hidden="true" />
                  Experimental third-party service. Returned files still pass URL, redirect, type, and content checks.
                </p>
              </section>
            </>
          )}

          <section className="settings-section engine-section">
            <div>
              <ShieldCheck aria-hidden="true" />
              <div>
                <h3>Engine integrity</h3>
                <p>yt-dlp: {toolSummary(tools?.ytDlp)}</p>
                <p>gallery-dl: {toolSummary(tools?.galleryDl)}</p>
                <p>Deno: {toolSummary(tools?.deno)}</p>
                <p>FFmpeg: {toolSummary(tools?.ffmpeg)}</p>
              </div>
            </div>
            <div className="settings-actions">
              <button
                type="button"
                className="secondary-button"
                onClick={onCheckUpdates}
                disabled={checkingUpdates || checkingTools}
              >
                <RefreshCcw aria-hidden="true" className={checkingUpdates ? "spin" : undefined} />
                {checkingUpdates ? "Checking" : "Check updates"}
              </button>
              <button type="button" className="text-button" onClick={onRepairTools} disabled={checkingTools}>
                {checkingTools ? "Repairing" : "Repair engine"}
              </button>
            </div>
          </section>
        </div>

        <footer className="dialog-footer">
          <button type="button" className="text-button" onClick={onClose}>Cancel</button>
          <button type="button" className="primary-button compact" onClick={onSave}>Save settings</button>
        </footer>
      </div>
    </dialog>
  );
}
