import { ArrowDownToLine, Check, RefreshCcw, Wrench, X } from "lucide-react";
import { useEffect, useRef, type CSSProperties } from "react";

import type { AppUpdateInfo, ToolUpdatesReport } from "../../lib/types";

type UpdateDialogProps = {
  open: boolean;
  appUpdate: AppUpdateInfo | null;
  toolUpdates: ToolUpdatesReport | null;
  installing: boolean;
  progress: number | null;
  onInstall: () => void;
  onClose: () => void;
};

function toolStatus(report: ToolUpdatesReport | null) {
  if (!report) return "Tool versions could not be checked.";
  const updates = [report.ytDlp, report.galleryDl, report.deno, report.ffmpeg]
    .filter((tool) => tool.updateAvailable)
    .map((tool) => tool.name);
  if (updates.length === 0) return "Download tools are current.";
  return `${updates.join(" and ")} ${updates.length === 1 ? "has" : "have"} an update.`;
}

export function UpdateDialog({
  open,
  appUpdate,
  toolUpdates,
  installing,
  progress,
  onInstall,
  onClose,
}: UpdateDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) {
      if (typeof dialog.showModal === "function") dialog.showModal();
      else dialog.setAttribute("open", "");
    }
    if (!open && dialog.open) {
      if (typeof dialog.close === "function") dialog.close();
      else dialog.removeAttribute("open");
    }
  }, [open]);

  const hasUpdates = Boolean(appUpdate || toolUpdates?.updatesAvailable);

  return (
    <dialog
      ref={dialogRef}
      className="update-dialog"
      aria-labelledby="update-title"
      onCancel={(event) => {
        event.preventDefault();
        if (!installing) onClose();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget && !installing) onClose();
      }}
    >
      <div className="update-panel">
        <header className="dialog-header">
          <div className="update-symbol" aria-hidden="true">
            {hasUpdates ? <ArrowDownToLine /> : <Check />}
          </div>
          <div>
            <h2 id="update-title">{hasUpdates ? "Update available" : "You are up to date"}</h2>
            <p>{hasUpdates ? "Install now, or come back to it later." : "MediaFilez Desktop and its managed tools are current."}</p>
          </div>
          {!installing && (
            <button type="button" className="icon-button quiet" aria-label="Close updates" onClick={onClose}>
              <X aria-hidden="true" />
            </button>
          )}
        </header>

        <div className="update-list">
          <div className="update-row">
            <RefreshCcw aria-hidden="true" />
            <div>
              <strong>MediaFilez Desktop</strong>
              <span>
                {appUpdate
                  ? `${appUpdate.currentVersion} to ${appUpdate.version}`
                  : "Current version installed"}
              </span>
            </div>
          </div>
          <div className="update-row">
            <Wrench aria-hidden="true" />
            <div>
              <strong>Download tools</strong>
              <span>{toolStatus(toolUpdates)}</span>
            </div>
          </div>
        </div>

        {appUpdate?.body && <p className="release-notes">{appUpdate.body}</p>}

        {installing && (
          <div className="update-progress" role="status">
            <span>{progress === null ? "Downloading update" : `Downloading update ${progress}%`}</span>
            <div>
              <span
                style={{ "--progress": (progress ?? 12) / 100 } as CSSProperties}
              />
            </div>
          </div>
        )}

        <footer className="dialog-footer">
          {hasUpdates ? (
            <>
              <button type="button" className="text-button" onClick={onClose} disabled={installing}>Later</button>
              <button type="button" className="primary-button compact" onClick={onInstall} disabled={installing}>
                {installing ? <RefreshCcw className="spin" aria-hidden="true" /> : <ArrowDownToLine aria-hidden="true" />}
                {installing ? "Updating" : "Update now"}
              </button>
            </>
          ) : (
            <button type="button" className="primary-button compact" onClick={onClose}>Done</button>
          )}
        </footer>
      </div>
    </dialog>
  );
}
