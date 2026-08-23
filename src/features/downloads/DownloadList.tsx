import {
  CheckCircle2,
  CircleAlert,
  Clock3,
  FolderOpen,
  Image,
  KeyRound,
  ListVideo,
  Music,
  Play,
  RotateCcw,
  Trash2,
  Video,
  X,
} from "lucide-react";

import { Progress } from "../../components/ui/progress";
import { getProviderLabel } from "../../lib/download-options";
import type { JobItem, JobStatus } from "../../lib/types";

type JobListProps = {
  jobs: JobItem[];
  onCancel: (id: string) => void;
  onRetry: (job: JobItem) => void;
  onClearFinished: () => void;
  onOpenFile: (path: string) => void;
  onShowInFolder: (path: string) => void;
  onOpenSettings: () => void;
};

const AUTH_ERROR_CODES = new Set([
  "authentication-required",
  "browser-cookies-locked",
  "cookie-decryption-failed",
  "forbidden",
]);

const STATUS_LABELS: Record<JobStatus, string> = {
  queued: "Queued",
  probing: "Finding media",
  downloading: "Downloading",
  converting: "Converting",
  complete: "Complete",
  failed: "Failed",
  cancelled: "Cancelled",
};

function statusIcon(status: JobStatus) {
  if (status === "complete") return <CheckCircle2 aria-hidden="true" />;
  if (status === "failed") return <CircleAlert aria-hidden="true" />;
  return <Clock3 aria-hidden="true" />;
}

export function DownloadList({
  jobs,
  onCancel,
  onRetry,
  onClearFinished,
  onOpenFile,
  onShowInFolder,
  onOpenSettings,
}: JobListProps) {
  const finishedCount = jobs.filter((job) =>
    ["complete", "failed", "cancelled"].includes(job.status),
  ).length;

  return (
    <section className="download-history" aria-labelledby="downloads-title" aria-live="polite">
      <header className="downloads-heading">
        <div>
          <h2 id="downloads-title">Downloads</h2>
          <p>{jobs.length ? `${jobs.length} item${jobs.length === 1 ? "" : "s"}` : "Queue is clear"}</p>
        </div>
        {finishedCount > 0 && (
          <button type="button" className="text-button" onClick={onClearFinished}>
            <Trash2 aria-hidden="true" />
            Clear finished
          </button>
        )}
      </header>

      {jobs.length === 0 ? (
        <div className="empty-state">
          <ListVideo aria-hidden="true" />
          <strong>No downloads yet.</strong>
          <p>Paste a link above. Progress and saved files will appear here.</p>
        </div>
      ) : (
        <ul className="job-list">
          {jobs.map((job) => {
            const running = ["queued", "probing", "downloading", "converting"].includes(
              job.status,
            );
            const indeterminate =
              job.status === "queued" ||
              job.status === "probing" ||
              job.status === "converting" ||
              (job.status === "downloading" && job.progress === 0);

            return (
              <li className="job-row" key={job.id} data-status={job.status}>
                <span className="job-media-icon" aria-hidden="true">
                  {job.mode === "audio" ? <Music /> : job.mode === "image" ? <Image /> : <Video />}
                </span>

                <div className="job-content">
                  <div className="job-title-line">
                    <strong title={job.title || job.url}>{job.title || job.url}</strong>
                    <span className={`status-label ${job.status}`}>
                      {statusIcon(job.status)}
                      {STATUS_LABELS[job.status]}
                    </span>
                  </div>

                  <div className="job-detail">
                    {job.provider && <span>{getProviderLabel(job.provider)}</span>}
                    {job.status === "downloading" && job.speed && <span>{job.speed}</span>}
                    {job.status === "downloading" && job.eta && <span>{job.eta} left</span>}
                    {(job.status === "queued" || job.status === "probing") && (
                      <span>{job.detail || "Preparing the local engine"}</span>
                    )}
                    {job.status === "converting" && <span>{job.detail || "Processing media"}</span>}
                    {job.status === "failed" && (
                      <span className="job-error">{job.error || "This provider chain could not resolve the link."}</span>
                    )}
                    {job.status === "complete" && job.filePath && (
                      <span className="job-path" title={job.filePath}>{job.filePath}</span>
                    )}
                  </div>

                  {running && (
                    <div className={indeterminate ? "job-progress indeterminate" : "job-progress"}>
                      <Progress
                        value={indeterminate ? 100 : job.progress}
                        label={`${job.title || job.url} progress`}
                      />
                    </div>
                  )}
                </div>

                <div className="job-actions">
                  {job.status === "failed" && job.errorCode && AUTH_ERROR_CODES.has(job.errorCode) && (
                    <button
                      type="button"
                      className="text-button job-settings-button"
                      onClick={onOpenSettings}
                    >
                      <KeyRound aria-hidden="true" />
                      Sign-in settings
                    </button>
                  )}
                  {job.status === "complete" && job.filePath && (
                    <>
                      <button
                        type="button"
                        className="icon-button quiet"
                        aria-label={`Open ${job.title || "file"}`}
                        onClick={() => onOpenFile(job.filePath!)}
                      >
                        <Play aria-hidden="true" />
                      </button>
                      <button
                        type="button"
                        className="icon-button quiet"
                        aria-label={`Show ${job.title || "file"} in folder`}
                        onClick={() => onShowInFolder(job.filePath!)}
                      >
                        <FolderOpen aria-hidden="true" />
                      </button>
                    </>
                  )}
                  {(job.status === "failed" || job.status === "cancelled") && (
                    <button
                      type="button"
                      className="icon-button quiet"
                      aria-label={`Retry ${job.title || "download"}`}
                      onClick={() => onRetry(job)}
                    >
                      <RotateCcw aria-hidden="true" />
                    </button>
                  )}
                  {running && (
                    <button
                      type="button"
                      className="icon-button quiet"
                      aria-label={`Cancel ${job.title || "download"}`}
                      onClick={() => onCancel(job.id)}
                    >
                      <X aria-hidden="true" />
                    </button>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
