//! Download jobs, provider fallback, cancellation, and progress events.

use std::collections::{HashMap, VecDeque};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures_util::StreamExt;
use percent_encoding::percent_decode_str;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

use crate::models::{DownloadMode, DownloadRequest, ProviderKind};
use crate::providers::cobalt::{
    build_cobalt_request, rank_instances, CobaltResponse, InstanceDirectory,
    INSTANCE_DIRECTORY_URL, SEED_INSTANCES,
};
use crate::providers::direct::{
    extract_media_links_from_html, is_direct_media_url, is_stream_manifest_url,
};
use crate::providers::gallery_dl::{build_gallery_dl_args, supports_mode as gallery_supports_mode};
use crate::providers::instagram::proxy_url as instagram_proxy_url;
use crate::providers::yt_dlp::build_ytdlp_args;
use crate::security::{
    is_safe_download_filename, secure_get, secure_redirect_policy, validate_api_endpoint,
    validate_remote_target, validate_remote_url,
};
use crate::settings::{cobalt_endpoints, ApiProviderSettings, AppSettings, CookieSource};
use crate::storage::{next_available_path, sanitize_filename};
use crate::tools;

pub const DOWNLOAD_EVENT: &str = "download://update";
const APP_USER_AGENT: &str = concat!("MediaFilez Desktop/", env!("CARGO_PKG_VERSION"));
const BROWSER_USER_AGENT: &str = concat!(
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) MediaFilez-Desktop/",
    env!("CARGO_PKG_VERSION")
);
const DIRECTORY_USER_AGENT: &str = concat!(
    "MediaFilez Desktop/",
    env!("CARGO_PKG_VERSION"),
    " (github.com/RlxChap2/mediafilez-desktop)"
);
const MAX_API_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_HTML_BYTES: usize = 5 * 1024 * 1024;
const MAX_STDERR_LINES: usize = 200;
const MAX_PROVIDER_ERROR_CHARS: usize = 360;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobUpdate {
    pub id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

impl JobUpdate {
    fn new(id: &str, status: &str) -> Self {
        Self {
            id: id.to_string(),
            status: status.to_string(),
            title: None,
            provider: None,
            progress: None,
            speed: None,
            eta: None,
            detail: None,
            file_path: None,
            error: None,
            error_code: None,
        }
    }
}

/// Job progress callback; wired to the `download://update` event in the app.
pub type JobSink<'a> = &'a (dyn Fn(JobUpdate) + Send + Sync);

/// Resolved external tool locations for one job run.
pub struct EngineTools {
    pub yt_dlp_path: Option<String>,
    pub gallery_dl_path: Option<String>,
    /// Deno is required by current yt-dlp releases for full YouTube support.
    pub deno_path: Option<String>,
    /// Directory containing a managed ffmpeg.exe, when the system has none.
    pub ffmpeg_dir: Option<String>,
}

#[derive(Clone)]
pub struct JobManager {
    cancel_flags: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    slots: Arc<tokio::sync::Semaphore>,
}

impl Default for JobManager {
    fn default() -> Self {
        Self {
            cancel_flags: Arc::new(Mutex::new(HashMap::new())),
            slots: Arc::new(tokio::sync::Semaphore::new(4)),
        }
    }
}

impl JobManager {
    pub fn register(&self, id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        if let Ok(mut flags) = self.cancel_flags.lock() {
            flags.insert(id.to_string(), flag.clone());
        }
        flag
    }

    pub fn cancel(&self, id: &str) -> bool {
        if let Ok(flags) = self.cancel_flags.lock() {
            if let Some(flag) = flags.get(id) {
                flag.store(true, Ordering::SeqCst);
                return true;
            }
        }
        false
    }

    pub fn finish(&self, id: &str) {
        if let Ok(mut flags) = self.cancel_flags.lock() {
            flags.remove(id);
        }
    }

    pub async fn wait_for_slot(&self) -> tokio::sync::OwnedSemaphorePermit {
        self.slots
            .clone()
            .acquire_owned()
            .await
            .expect("download semaphore remains open")
    }
}

struct Cancelled;

fn check_cancel(cancel: &AtomicBool) -> Result<(), Cancelled> {
    if cancel.load(Ordering::SeqCst) {
        Err(Cancelled)
    } else {
        Ok(())
    }
}

fn format_bytes_per_sec(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1_048_576.0 {
        format!("{:.1} MB/s", bytes_per_sec / 1_048_576.0)
    } else if bytes_per_sec >= 1024.0 {
        format!("{:.0} KB/s", bytes_per_sec / 1024.0)
    } else {
        format!("{bytes_per_sec:.0} B/s")
    }
}

fn format_eta(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "--".to_string();
    }
    let seconds = seconds.round() as u64;
    if seconds >= 3600 {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

fn filename_from_url(url: &str) -> String {
    let base = url::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed.path_segments().and_then(|mut segments| {
                segments
                    .rfind(|segment| !segment.is_empty())
                    .map(str::to_string)
            })
        })
        .map(|segment| percent_decode_str(&segment).decode_utf8_lossy().to_string())
        .unwrap_or_default();
    let cleaned = sanitize_filename(&base);
    if cleaned == "download" && !cleaned.contains('.') {
        "download.bin".to_string()
    } else {
        cleaned
    }
}

async fn read_response_limited(
    response: reqwest::Response,
    limit: usize,
    label: &str,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(format!("{label} exceeded the {limit}-byte safety limit."));
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("Could not read {label}: {error}"))?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(format!("{label} exceeded the {limit}-byte safety limit."));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

async fn publish_without_overwrite(temp: &Path, requested: PathBuf) -> Result<PathBuf, String> {
    loop {
        let target = next_available_path(requested.clone(), |path| path.exists());
        match tokio::fs::hard_link(temp, &target).await {
            Ok(()) => {
                let _ = tokio::fs::remove_file(temp).await;
                return Ok(target);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(_) => {
                let mut destination = match tokio::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                    .await
                {
                    Ok(file) => file,
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(format!("Could not create final file: {error}")),
                };
                let copy_result = async {
                    let mut source = tokio::fs::File::open(temp)
                        .await
                        .map_err(|error| format!("Could not reopen temporary file: {error}"))?;
                    tokio::io::copy(&mut source, &mut destination)
                        .await
                        .map_err(|error| format!("Could not finalize file: {error}"))?;
                    destination
                        .flush()
                        .await
                        .map_err(|error| format!("Could not finish final file: {error}"))
                }
                .await;
                drop(destination);
                if let Err(error) = copy_result {
                    let _ = tokio::fs::remove_file(&target).await;
                    return Err(error);
                }
                let _ = tokio::fs::remove_file(temp).await;
                return Ok(target);
            }
        }
    }
}

async fn validate_downloaded_file(path: &Path) -> Result<(), String> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| format!("Could not inspect downloaded media: {error}"))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("The provider produced an empty file.".to_string());
    }

    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| format!("Could not verify downloaded media: {error}"))?;
    let mut header = [0_u8; 64];
    let read = tokio::io::AsyncReadExt::read(&mut file, &mut header)
        .await
        .map_err(|error| format!("Could not verify downloaded media: {error}"))?;
    let text = String::from_utf8_lossy(&header[..read]);
    let trimmed = text.trim_start().to_ascii_lowercase();
    if trimmed.starts_with('<')
        || trimmed.starts_with('{')
        || trimmed.starts_with('[')
        || trimmed.starts_with("error:")
    {
        return Err("The provider returned a web or error document instead of media.".to_string());
    }
    Ok(())
}

fn response_matches_mode(response: &reqwest::Response, url: &str, mode: &DownloadMode) -> bool {
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    media_type_matches_mode(&content_type, url, mode)
}

fn media_type_matches_mode(content_type: &str, url: &str, mode: &DownloadMode) -> bool {
    if content_type.starts_with("image/") {
        return *mode == DownloadMode::Image;
    }
    if content_type.starts_with("audio/") {
        return *mode == DownloadMode::Audio;
    }
    if content_type.starts_with("video/") {
        return matches!(mode, DownloadMode::Video | DownloadMode::MutedVideo);
    }
    if has_image_extension(url) {
        return *mode == DownloadMode::Image;
    }
    if has_audio_extension(url) {
        return *mode == DownloadMode::Audio;
    }
    if is_direct_media_url(url) && !is_stream_manifest_url(url) {
        return matches!(mode, DownloadMode::Video | DownloadMode::MutedVideo);
    }
    true
}

pub(crate) struct StreamTarget<'a> {
    pub provider: ProviderKind,
    pub mode: &'a DownloadMode,
    pub url: &'a str,
    pub output_dir: &'a str,
    pub file_name_hint: Option<String>,
}

/// Streams a URL straight to the output folder. Used by the direct provider
/// and to fetch resolved links from the API and HTML providers.
pub(crate) async fn stream_to_file(
    sink: JobSink<'_>,
    id: &str,
    target: StreamTarget<'_>,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let response = secure_get(
        target.url,
        BROWSER_USER_AGENT,
        std::time::Duration::from_secs(30),
    )
    .await?
    .error_for_status()
    .map_err(|error| format!("Server rejected the download: {error}"))?;

    if response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            let value = value.to_ascii_lowercase();
            value.starts_with("text/")
                || value.starts_with("application/xhtml")
                || value.starts_with("application/json")
                || value.starts_with("application/xml")
        })
    {
        return Err("The resolved link returned a web page instead of media.".to_string());
    }
    if !response_matches_mode(&response, target.url, target.mode) {
        return Err("The provider returned a different media type than requested.".to_string());
    }

    let total = response.content_length();
    let name = target
        .file_name_hint
        .map(|hint| sanitize_filename(&hint))
        .unwrap_or_else(|| filename_from_url(target.url));
    if !is_safe_download_filename(&name) {
        return Err("The server suggested an unsafe executable filename.".to_string());
    }
    let requested = Path::new(target.output_dir).join(&name);
    let temp = requested.with_file_name(format!(".{name}.{}.part", Uuid::new_v4()));

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .await
        .map_err(|error| format!("Could not create file: {error}"))?;

    let started = Instant::now();
    let mut downloaded: u64 = 0;
    let mut last_emit = Instant::now();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        if check_cancel(cancel).is_err() {
            drop(file);
            let _ = tokio::fs::remove_file(&temp).await;
            return Err("cancelled".to_string());
        }
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => {
                drop(file);
                let _ = tokio::fs::remove_file(&temp).await;
                return Err(format!("Connection interrupted: {error}"));
            }
        };
        if let Err(error) = file.write_all(&chunk).await {
            drop(file);
            let _ = tokio::fs::remove_file(&temp).await;
            return Err(format!("Could not write file: {error}"));
        }
        downloaded += chunk.len() as u64;

        if last_emit.elapsed().as_millis() > 250 {
            let elapsed = started.elapsed().as_secs_f64().max(0.001);
            let speed = downloaded as f64 / elapsed;
            let mut update = JobUpdate::new(id, "downloading");
            update.provider = Some(target.provider);
            update.speed = Some(format_bytes_per_sec(speed));
            if let Some(total) = total.filter(|total| *total > 0) {
                let bounded = downloaded.min(total);
                update.progress = Some((bounded as f64 / total as f64) * 100.0);
                update.eta = Some(format_eta(total.saturating_sub(downloaded) as f64 / speed));
            }
            sink(update);
            last_emit = Instant::now();
        }
    }

    if let Err(error) = file.flush().await {
        drop(file);
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(format!("Could not finish file: {error}"));
    }
    drop(file);
    if let Err(error) = validate_downloaded_file(&temp).await {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(error);
    }
    let published = publish_without_overwrite(&temp, requested).await;
    if published.is_err() {
        let _ = tokio::fs::remove_file(&temp).await;
    }
    published
}

fn parse_progress_line(line: &str) -> Option<(f64, String, String)> {
    let mut parts = line.trim().split('|');
    let percent = parts.next()?.trim().trim_end_matches('%').trim();
    let percent: f64 = percent.parse().ok()?;
    let speed = parts.next().unwrap_or("--").trim().to_string();
    let eta = parts.next().unwrap_or("--").trim().to_string();
    Some((percent, speed, eta))
}

async fn terminate_process_tree(child: &mut tokio::process::Child) {
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        let mut command = tokio::process::Command::new("taskkill");
        command
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(0x0800_0000);
        let _ = command.status().await;
    }

    let _ = child.kill().await;
    let _ = child.wait().await;
}

/// Runs yt-dlp as a child process and translates its output into job events.
struct YtDlpAttemptOptions<'a> {
    cookie_source: Option<&'a CookieSource>,
    impersonate: bool,
}

async fn ytdlp_download_attempt(
    sink: JobSink<'_>,
    id: &str,
    request: &DownloadRequest,
    engine_tools: &EngineTools,
    concurrent_fragments: u8,
    options: YtDlpAttemptOptions<'_>,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let Some(tool_path) = engine_tools.yt_dlp_path.as_ref() else {
        return Err("yt-dlp is not installed yet.".to_string());
    };

    let output_template = Path::new(&request.output_dir)
        .join("%(title)s.%(ext)s")
        .to_string_lossy()
        .to_string();
    let mut args = build_ytdlp_args(
        &request.url,
        request.mode.clone(),
        request.video_quality,
        request.audio_format,
        &request.audio_bitrate,
        &output_template,
        concurrent_fragments,
    );

    if let Some(deno_path) = engine_tools.deno_path.as_ref() {
        let insert_at = args.len().saturating_sub(1);
        args.insert(insert_at, "--js-runtimes".to_string());
        args.insert(insert_at + 1, format!("deno:{deno_path}"));
    }

    if let Some(source) = options.cookie_source {
        let insert_at = args.len().saturating_sub(1);
        match source {
            CookieSource::Browser(browser) => {
                args.insert(insert_at, "--cookies-from-browser".to_string());
                args.insert(insert_at + 1, browser.clone());
            }
            CookieSource::File(path) => {
                args.insert(insert_at, "--cookies".to_string());
                args.insert(insert_at + 1, path.clone());
            }
        }
    }

    if options.impersonate {
        args.insert(0, "--impersonate".to_string());
    }

    // Surface the media title and final file path over stdout markers.
    let extra = [
        "--no-simulate",
        "--no-warnings",
        "--print",
        "before_dl:rsd_title:%(title)s",
        "--print",
        "after_move:rsd_path:%(filepath)s",
    ];
    for (index, value) in extra.iter().enumerate() {
        args.insert(index, value.to_string());
    }

    // Point yt-dlp at the managed FFmpeg when the system has none.
    if let Some(dir) = engine_tools.ffmpeg_dir.as_ref() {
        args.insert(0, "--ffmpeg-location".to_string());
        args.insert(1, dir.clone());
    }

    let mut command = tokio::process::Command::new(tool_path);
    command
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start yt-dlp: {error}"))?;

    let stdout = child.stdout.take().ok_or("yt-dlp produced no output.")?;
    let stderr = child.stderr.take();

    let stderr_task = tokio::spawn(async move {
        let mut lines = VecDeque::with_capacity(MAX_STDERR_LINES);
        if let Some(stderr) = stderr {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if !line.trim().is_empty() {
                    if lines.len() == MAX_STDERR_LINES {
                        lines.pop_front();
                    }
                    lines.push_back(line);
                }
            }
        }
        lines
    });

    let mut final_path: Option<PathBuf> = None;
    let mut reader = BufReader::new(stdout).lines();
    loop {
        let line = tokio::select! {
            line = reader.next_line() => line.map_err(|error| format!("yt-dlp output error: {error}"))?,
            _ = tokio::time::sleep(std::time::Duration::from_millis(300)) => {
                if check_cancel(cancel).is_err() {
                    terminate_process_tree(&mut child).await;
                    return Err("cancelled".to_string());
                }
                continue;
            }
        };
        let Some(line) = line else {
            break;
        };
        if check_cancel(cancel).is_err() {
            terminate_process_tree(&mut child).await;
            return Err("cancelled".to_string());
        }

        if let Some(title) = line.strip_prefix("rsd_title:") {
            let mut update = JobUpdate::new(id, "downloading");
            update.provider = Some(ProviderKind::YtDlp);
            update.title = Some(title.trim().to_string());
            sink(update);
        } else if let Some(path) = line.strip_prefix("rsd_path:") {
            final_path = Some(PathBuf::from(path.trim()));
        } else if line.contains("[ExtractAudio]") || line.contains("[Merger]") {
            let mut update = JobUpdate::new(id, "converting");
            update.provider = Some(ProviderKind::YtDlp);
            update.detail = Some("Processing media".to_string());
            sink(update);
        } else if let Some((percent, speed, eta)) = parse_progress_line(&line) {
            let mut update = JobUpdate::new(id, "downloading");
            update.provider = Some(ProviderKind::YtDlp);
            update.progress = Some(percent);
            update.speed = Some(speed);
            update.eta = Some(eta);
            sink(update);
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|error| format!("yt-dlp crashed: {error}"))?;
    if !status.success() {
        let stderr_lines = stderr_task.await.unwrap_or_default();
        let reason = stderr_lines
            .iter()
            .rev()
            .find(|line| line.contains("ERROR"))
            .or_else(|| stderr_lines.back())
            .cloned()
            .unwrap_or_else(|| "yt-dlp could not process this link.".to_string());
        return Err(reason);
    }

    let final_path = final_path.ok_or_else(|| "yt-dlp finished without a file.".to_string())?;
    let output_root = std::fs::canonicalize(&request.output_dir)
        .map_err(|error| format!("Could not verify output folder: {error}"))?;
    let final_path = std::fs::canonicalize(&final_path)
        .map_err(|error| format!("Could not verify downloaded file: {error}"))?;
    if !final_path.is_file() || !final_path.starts_with(&output_root) {
        return Err("yt-dlp returned a file outside the selected output folder.".to_string());
    }
    let file_name = final_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "yt-dlp returned an invalid filename.".to_string())?;
    if !is_safe_download_filename(file_name) {
        return Err("yt-dlp returned an unsafe executable filename.".to_string());
    }
    validate_downloaded_file(&final_path).await?;
    Ok(final_path)
}

/// Runs yt-dlp and retries a rejected media request once with browser request
/// impersonation. Impersonation is deliberately not forced for normal traffic
/// because yt-dlp warns that it can reduce speed and stability.
pub async fn ytdlp_download(
    sink: JobSink<'_>,
    id: &str,
    request: &DownloadRequest,
    engine_tools: &EngineTools,
    concurrent_fragments: u8,
    cookie_source: Option<&CookieSource>,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    match ytdlp_download_attempt(
        sink,
        id,
        request,
        engine_tools,
        concurrent_fragments,
        YtDlpAttemptOptions {
            cookie_source,
            impersonate: false,
        },
        cancel,
    )
    .await
    {
        Err(error) if is_forbidden_error(&error) => {
            let mut update = JobUpdate::new(id, "probing");
            update.provider = Some(ProviderKind::YtDlp);
            update.detail = Some("Retrying with browser-compatible networking".to_string());
            sink(update);
            match ytdlp_download_attempt(
                sink,
                id,
                request,
                engine_tools,
                concurrent_fragments,
                YtDlpAttemptOptions {
                    cookie_source,
                    impersonate: true,
                },
                cancel,
            )
            .await
            {
                Ok(path) => Ok(path),
                Err(retry_error) => {
                    Err(format!("{error}\nBrowser-compatible retry: {retry_error}"))
                }
            }
        }
        result => result,
    }
}

fn files_below(root: &Path) -> Result<Vec<PathBuf>, String> {
    let canonical_root = std::fs::canonicalize(root)
        .map_err(|error| format!("Could not inspect gallery output: {error}"))?;
    let mut directories = vec![canonical_root.clone()];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        let entries = std::fs::read_dir(&directory)
            .map_err(|error| format!("Could not inspect gallery output: {error}"))?;
        for entry in entries {
            let entry =
                entry.map_err(|error| format!("Could not inspect gallery output: {error}"))?;
            let metadata = entry
                .file_type()
                .map_err(|error| format!("Could not inspect gallery output: {error}"))?;
            if metadata.is_symlink() {
                continue;
            }
            let path = entry.path();
            if metadata.is_dir() {
                directories.push(path);
            } else if metadata.is_file() {
                let canonical = std::fs::canonicalize(&path)
                    .map_err(|error| format!("Could not inspect gallery output: {error}"))?;
                if canonical.starts_with(&canonical_root)
                    && !canonical
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("part"))
                {
                    files.push(canonical);
                }
            }
        }
    }
    files.sort();
    Ok(files)
}

async fn gallery_dl_download(
    sink: JobSink<'_>,
    id: &str,
    request: &DownloadRequest,
    engine_tools: &EngineTools,
    cookie_source: Option<&CookieSource>,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    if !gallery_supports_mode(&request.mode) {
        return Err("gallery-dl does not extract audio tracks.".to_string());
    }
    let Some(tool_path) = engine_tools.gallery_dl_path.as_ref() else {
        return Err("gallery-dl could not be installed for this platform.".to_string());
    };

    let staging =
        Path::new(&request.output_dir).join(format!(".mediafilez-gallery-{}", Uuid::new_v4()));
    tokio::fs::create_dir(&staging)
        .await
        .map_err(|error| format!("Could not create gallery staging folder: {error}"))?;
    let args = build_gallery_dl_args(
        &request.url,
        &staging.to_string_lossy(),
        request.mode.clone(),
        cookie_source,
    );

    let mut command = tokio::process::Command::new(tool_path);
    command
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = tokio::fs::remove_dir_all(&staging).await;
            return Err(format!("Could not start gallery-dl: {error}"));
        }
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            terminate_process_tree(&mut child).await;
            let _ = tokio::fs::remove_dir_all(&staging).await;
            return Err("gallery-dl produced no output.".to_string());
        }
    };
    let stderr = child.stderr.take();
    let stderr_task = tokio::spawn(async move {
        let mut lines = VecDeque::with_capacity(MAX_STDERR_LINES);
        if let Some(stderr) = stderr {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if !line.trim().is_empty() {
                    if lines.len() == MAX_STDERR_LINES {
                        lines.pop_front();
                    }
                    lines.push_back(line);
                }
            }
        }
        lines
    });

    let read_result: Result<(), String> = async {
        let mut downloaded = 0_u32;
        let mut reader = BufReader::new(stdout).lines();
        loop {
            let line = tokio::select! {
                line = reader.next_line() => line.map_err(|error| format!("gallery-dl output error: {error}"))?,
                _ = tokio::time::sleep(std::time::Duration::from_millis(300)) => {
                    check_cancel(cancel).map_err(|_| "cancelled".to_string())?;
                    continue;
                }
            };
            let Some(line) = line else { break };
            if !line.trim().is_empty() {
                downloaded += 1;
                let mut update = JobUpdate::new(id, "downloading");
                update.provider = Some(ProviderKind::GalleryDl);
                update.detail = Some(format!(
                    "Saved {downloaded} gallery item{}",
                    if downloaded == 1 { "" } else { "s" }
                ));
                sink(update);
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = read_result {
        terminate_process_tree(&mut child).await;
        let _ = stderr_task.await;
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(error);
    }

    let status = match child.wait().await {
        Ok(status) => status,
        Err(error) => {
            let _ = stderr_task.await;
            let _ = tokio::fs::remove_dir_all(&staging).await;
            return Err(format!("gallery-dl crashed: {error}"));
        }
    };
    let stderr_lines = stderr_task.await.unwrap_or_default();
    if !status.success() {
        let reason = stderr_lines
            .iter()
            .rev()
            .find(|line| line.contains("error") || line.contains("unsupported"))
            .or_else(|| stderr_lines.back())
            .cloned()
            .unwrap_or_else(|| "gallery-dl could not process this link.".to_string());
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(reason);
    }

    let publish_result: Result<PathBuf, String> = async {
        let staged_files = files_below(&staging)?;
        if staged_files.is_empty() {
            return Err("gallery-dl finished without a media file.".to_string());
        }
        let mut validated = Vec::with_capacity(staged_files.len());
        for staged in staged_files {
            validate_downloaded_file(&staged).await?;
            let file_name = staged
                .file_name()
                .and_then(|value| value.to_str())
                .map(str::to_owned)
                .ok_or_else(|| "gallery-dl returned an invalid filename.".to_string())?;
            if !is_safe_download_filename(&file_name) {
                return Err("gallery-dl returned an unsafe executable filename.".to_string());
            }
            validated.push((staged, file_name));
        }

        let mut published = Vec::with_capacity(validated.len());
        for (staged, file_name) in validated {
            let target = Path::new(&request.output_dir).join(file_name);
            match publish_without_overwrite(&staged, target).await {
                Ok(path) => published.push(path),
                Err(error) => {
                    for path in &published {
                        let _ = tokio::fs::remove_file(path).await;
                    }
                    return Err(error);
                }
            }
        }
        Ok(published.remove(0))
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&staging).await;
    publish_result
}

fn is_instagram_media_host(host: &str) -> bool {
    host == "cdninstagram.com"
        || host.ends_with(".cdninstagram.com")
        || host == "fbcdn.net"
        || host.ends_with(".fbcdn.net")
}

async fn instagram_proxy_download(
    sink: JobSink<'_>,
    id: &str,
    request: &DownloadRequest,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let proxy_url = instagram_proxy_url(&request.url)
        .ok_or_else(|| "This is not a supported public Instagram post URL.".to_string())?;
    let response = secure_get(
        &proxy_url,
        "Discordbot/2.0",
        std::time::Duration::from_secs(15),
    )
    .await?
    .error_for_status()
    .map_err(|error| format!("Instagram embed resolver rejected the link: {error}"))?;
    let media_url = response.url().clone();
    let host = media_url
        .host_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !is_instagram_media_host(&host) {
        return Err("Instagram embed resolver did not return an Instagram media file.".to_string());
    }
    drop(response);
    stream_to_file(
        sink,
        id,
        StreamTarget {
            provider: ProviderKind::InstagramProxy,
            mode: &request.mode,
            url: media_url.as_str(),
            output_dir: &request.output_dir,
            file_name_hint: None,
        },
        cancel,
    )
    .await
}

fn is_forbidden_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("http error 403") || lower.contains("403: forbidden")
}

fn friendly_failure(failures: &[String], settings: &AppSettings) -> (String, Option<String>) {
    let combined = failures.join(" | ");
    let lower = combined.to_ascii_lowercase();

    if lower.contains("could not copy") && lower.contains("cookie database") {
        return (
            "Chrome has locked its cookie database. Close every Chrome window and background process, or use Firefox or a Netscape cookies.txt file in Settings."
                .to_string(),
            Some("browser-cookies-locked".to_string()),
        );
    }
    if lower.contains("failed to decrypt with dpapi") {
        return (
            "Windows could not decrypt this Chromium browser session. Use Firefox or select a Netscape cookies.txt file in Settings."
                .to_string(),
            Some("cookie-decryption-failed".to_string()),
        );
    }
    if lower.contains("account authentication is required")
        || lower.contains("login required")
        || lower.contains("empty media response")
    {
        let message = match settings.cookie_source() {
            Some(CookieSource::File(_)) => {
                "The site rejected the selected cookie file. Export a fresh Netscape cookies.txt file from a signed-in session, then retry."
            }
            Some(CookieSource::Browser(_)) => {
                "The site rejected the selected browser session. Sign in again; on Windows, Firefox is the most reliable option."
            }
            None => {
                "This link requires sign-in. Open Settings, then use a Firefox session or select a Netscape cookies.txt file and retry."
            }
        };
        return (
            message.to_string(),
            Some("authentication-required".to_string()),
        );
    }
    if is_forbidden_error(&combined) {
        return (
            "The site refused the media request after an automatic browser-compatible retry. Refresh the site session in Settings or try again later."
                .to_string(),
            Some("forbidden".to_string()),
        );
    }

    let details = failures
        .iter()
        .rev()
        .take(4)
        .map(|failure| {
            let mut detail: String = failure.chars().take(MAX_PROVIDER_ERROR_CHARS).collect();
            if failure.chars().count() > MAX_PROVIDER_ERROR_CHARS {
                detail.push('…');
            }
            detail
        })
        .collect::<Vec<_>>()
        .join(" · ");
    (
        format!(
            "All compatible engines were tried, but none returned valid media. {details} Repair the engines or add a signed-in browser session, then retry."
        ),
        Some("provider-chain-exhausted".to_string()),
    )
}

/// Calls one Cobalt-compatible endpoint and streams the file it resolves.
struct CobaltEndpoint<'a> {
    provider: ProviderKind,
    base_url: &'a str,
    auth_header: Option<String>,
    timeout_seconds: u64,
}

async fn cobalt_fetch(
    sink: JobSink<'_>,
    id: &str,
    request: &DownloadRequest,
    endpoint: CobaltEndpoint<'_>,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    validate_api_endpoint(endpoint.base_url)?;
    let payload = build_cobalt_request(
        &request.url,
        request.mode.clone(),
        request.audio_format,
        Some(request.audio_bitrate.as_str()),
    );

    let client = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .redirect(secure_redirect_policy())
        .timeout(std::time::Duration::from_secs(
            endpoint.timeout_seconds.max(5),
        ))
        .build()
        .map_err(|error| format!("HTTP client error: {error}"))?;

    let mut http_request = client
        .post(endpoint.base_url)
        .header("Accept", "application/json")
        .json(&payload);
    if let Some(header_value) = endpoint.auth_header {
        http_request = http_request.header("Authorization", header_value);
    }

    let response = http_request
        .send()
        .await
        .map_err(|error| format!("API request failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("API rejected the request: {error}"))?;
    let body = read_response_limited(response, MAX_API_RESPONSE_BYTES, "API response").await?;
    let response: CobaltResponse = serde_json::from_slice(&body)
        .map_err(|error| format!("API returned an unexpected response: {error}"))?;

    let media_url = if response.is_downloadable() {
        response.url.clone().unwrap_or_default()
    } else if let Some(item) = response.picker.iter().find(|item| match request.mode {
        DownloadMode::Image => matches!(item.r#type.as_str(), "photo" | "image" | "gif"),
        DownloadMode::Video | DownloadMode::MutedVideo => item.r#type == "video",
        DownloadMode::Audio => true,
    }) {
        item.url.clone()
    } else {
        let code = response
            .error
            .map(|error| error.code)
            .unwrap_or_else(|| "api.unsupported".to_string());
        return Err(format!("API could not handle this link ({code})."));
    };
    if media_url.is_empty() {
        return Err("API returned an empty media URL.".to_string());
    }
    validate_remote_url(&media_url)?;

    stream_to_file(
        sink,
        id,
        StreamTarget {
            provider: endpoint.provider,
            mode: &request.mode,
            url: &media_url,
            output_dir: &request.output_dir,
            file_name_hint: response.filename.clone(),
        },
        cancel,
    )
    .await
}

/// The user-configured Cobalt-compatible endpoint (highest API priority).
async fn cobalt_download(
    sink: JobSink<'_>,
    id: &str,
    request: &DownloadRequest,
    settings: &AppSettings,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let api = &settings.api_provider;
    let endpoints = cobalt_endpoints(&api.base_url);
    if !api.enabled || endpoints.is_empty() {
        return Err("No API endpoint configured.".to_string());
    }

    let auth_header = cobalt_authorization(api);
    let mut failures = Vec::new();
    for base_url in endpoints.iter().take(8) {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".to_string());
        }
        let mut update = JobUpdate::new(id, "probing");
        update.provider = Some(ProviderKind::ConfiguredApi);
        update.detail = Some(format!(
            "Trying Cobalt {}",
            base_url.trim_start_matches("https://")
        ));
        sink(update);
        match cobalt_fetch(
            sink,
            id,
            request,
            CobaltEndpoint {
                provider: ProviderKind::ConfiguredApi,
                base_url,
                auth_header: auth_header.clone(),
                timeout_seconds: api.timeout_seconds,
            },
            cancel,
        )
        .await
        {
            Ok(path) => return Ok(path),
            Err(error) if error == "cancelled" => return Err(error),
            Err(error) => failures.push(error),
        }
    }
    Err(format!(
        "Configured Cobalt endpoints failed: {}",
        failures.join(" | ")
    ))
}

fn cobalt_authorization(settings: &ApiProviderSettings) -> Option<String> {
    let token = settings.token.trim();
    match (settings.auth_type.as_str(), token.is_empty()) {
        ("bearer", false) => Some(format!("Bearer {token}")),
        ("api-key", false) => Some(format!("Api-Key {token}")),
        _ => None,
    }
}

/// Fetches the live list of open community instances, falling back to a
/// bundled seed list. Cached for the lifetime of the process.
async fn public_instances() -> Vec<String> {
    static CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    if let Some(cached) = CACHE.get() {
        return cached.clone();
    }

    let fetched: Option<Vec<String>> = async {
        let client = reqwest::Client::builder()
            .user_agent(DIRECTORY_USER_AGENT)
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .ok()?;
        let response = client
            .get(INSTANCE_DIRECTORY_URL)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        let body = read_response_limited(response, MAX_API_RESPONSE_BYTES, "instance directory")
            .await
            .ok()?;
        let directory: InstanceDirectory = serde_json::from_slice(&body).ok()?;
        let ranked = rank_instances(&directory);
        (!ranked.is_empty()).then_some(ranked)
    }
    .await;

    let instances =
        fetched.unwrap_or_else(|| SEED_INSTANCES.iter().map(|url| url.to_string()).collect());
    CACHE.get_or_init(|| instances).clone()
}

/// Free community Cobalt servers, tried in ranked order without any auth.
async fn public_api_download(
    sink: JobSink<'_>,
    id: &str,
    request: &DownloadRequest,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let instances = public_instances().await;
    let mut last_error = "No community servers are reachable right now.".to_string();

    for base_url in instances.iter().take(4) {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".to_string());
        }
        let mut update = JobUpdate::new(id, "downloading");
        update.provider = Some(ProviderKind::PublicApi);
        update.detail = Some(format!(
            "Trying community server {}",
            base_url.trim_start_matches("https://")
        ));
        sink(update);

        match cobalt_fetch(
            sink,
            id,
            request,
            CobaltEndpoint {
                provider: ProviderKind::PublicApi,
                base_url,
                auth_header: None,
                timeout_seconds: 30,
            },
            cancel,
        )
        .await
        {
            Ok(path) => return Ok(path),
            Err(error) if error == "cancelled" => return Err(error),
            Err(error) => last_error = error,
        }
    }

    Err(last_error)
}

/// Fetches the page HTML and downloads the first direct media link found.
async fn html_probe_download(
    sink: JobSink<'_>,
    id: &str,
    request: &DownloadRequest,
    engine_tools: &EngineTools,
    settings: &AppSettings,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let response = secure_get(
        &request.url,
        BROWSER_USER_AGENT,
        std::time::Duration::from_secs(20),
    )
    .await
    .map_err(|error| format!("Could not fetch the page: {error}"))?
    .error_for_status()
    .map_err(|error| format!("The page rejected the request: {error}"))?;
    let html = read_response_limited(response, MAX_HTML_BYTES, "page HTML").await?;
    let html = String::from_utf8_lossy(&html);

    let links = extract_media_links_from_html(&request.url, &html);
    let media_url = links.iter().find(|url| match request.mode {
        DownloadMode::Image => has_image_extension(url),
        DownloadMode::Video => !has_image_extension(url) && !has_audio_extension(url),
        DownloadMode::Audio | DownloadMode::MutedVideo => false,
    });
    let Some(media_url) = media_url else {
        return Err("The page did not expose media matching the selected format.".to_string());
    };

    if is_stream_manifest_url(media_url) {
        let mut manifest_request = request.clone();
        manifest_request.url = media_url.clone();
        let cookie_source = settings.cookie_source();
        ytdlp_download(
            sink,
            id,
            &manifest_request,
            engine_tools,
            settings.concurrency,
            cookie_source.as_ref(),
            cancel,
        )
        .await
    } else {
        stream_to_file(
            sink,
            id,
            StreamTarget {
                provider: ProviderKind::HtmlProbe,
                mode: &request.mode,
                url: media_url,
                output_dir: &request.output_dir,
                file_name_hint: None,
            },
            cancel,
        )
        .await
    }
}

fn has_audio_extension(url: &str) -> bool {
    let audio = [".mp3", ".m4a", ".opus", ".wav"];
    url::Url::parse(url)
        .map(|parsed| {
            let path = parsed.path().to_ascii_lowercase();
            audio.iter().any(|ext| path.ends_with(ext))
        })
        .unwrap_or(false)
}

fn has_image_extension(url: &str) -> bool {
    let image = [".jpg", ".jpeg", ".png", ".webp", ".gif", ".avif", ".bmp"];
    url::Url::parse(url)
        .map(|parsed| {
            let path = parsed.path().to_ascii_lowercase();
            image.iter().any(|extension| path.ends_with(extension))
        })
        .unwrap_or(false)
}

fn is_gallery_focused_url(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| {
            host == "pinterest.com"
                || host.ends_with(".pinterest.com")
                || host == "pin.it"
                || host == "instagram.com"
                || host.ends_with(".instagram.com")
        })
}

fn provider_chain(request: &DownloadRequest, settings: &AppSettings) -> Vec<ProviderKind> {
    // A direct file is only a valid fast path when it already matches the
    // requested mode; extracting audio from a raw file needs yt-dlp/FFmpeg.
    let direct_ok = is_direct_media_url(&request.url)
        && !is_stream_manifest_url(&request.url)
        && match request.mode {
            DownloadMode::Audio => has_audio_extension(&request.url),
            DownloadMode::Image => has_image_extension(&request.url),
            DownloadMode::Video => !has_audio_extension(&request.url),
            DownloadMode::MutedVideo => false,
        };

    let mut chain = Vec::new();
    if direct_ok {
        chain.push(ProviderKind::Direct);
    }
    if request.mode == DownloadMode::Image && is_gallery_focused_url(&request.url) {
        chain.push(ProviderKind::GalleryDl);
    }
    chain.push(ProviderKind::YtDlp);
    let api = &settings.api_provider;
    if api.enabled && !api.base_url.trim().is_empty() {
        chain.push(ProviderKind::ConfiguredApi);
    }
    if settings.community_fallback {
        chain.push(ProviderKind::PublicApi);
    }
    if gallery_supports_mode(&request.mode) && !chain.contains(&ProviderKind::GalleryDl) {
        chain.push(ProviderKind::GalleryDl);
    }
    if settings.instagram_proxy_fallback
        && matches!(request.mode, DownloadMode::Video | DownloadMode::Image)
        && instagram_proxy_url(&request.url).is_some()
    {
        chain.push(ProviderKind::InstagramProxy);
    }
    if matches!(request.mode, DownloadMode::Video | DownloadMode::Image) {
        chain.push(ProviderKind::HtmlProbe);
    }
    chain
}

/// Runs the provider chain for one job, reporting through `sink`.
pub async fn run_chain(
    sink: JobSink<'_>,
    id: &str,
    request: &DownloadRequest,
    settings: &AppSettings,
    engine_tools: &EngineTools,
    cancel: &AtomicBool,
) {
    let chain = provider_chain(request, settings);
    let mut failures: Vec<String> = Vec::new();

    for provider in chain {
        if cancel.load(Ordering::SeqCst) {
            sink(JobUpdate::new(id, "cancelled"));
            return;
        }

        let mut update = JobUpdate::new(id, "downloading");
        update.provider = Some(provider);
        update.progress = Some(0.0);
        sink(update);

        let result = match provider {
            ProviderKind::Direct => {
                stream_to_file(
                    sink,
                    id,
                    StreamTarget {
                        provider: ProviderKind::Direct,
                        mode: &request.mode,
                        url: &request.url,
                        output_dir: &request.output_dir,
                        file_name_hint: None,
                    },
                    cancel,
                )
                .await
            }
            ProviderKind::ConfiguredApi => {
                cobalt_download(sink, id, request, settings, cancel).await
            }
            ProviderKind::PublicApi => public_api_download(sink, id, request, cancel).await,
            ProviderKind::YtDlp => {
                let cookie_source = settings.cookie_source();
                ytdlp_download(
                    sink,
                    id,
                    request,
                    engine_tools,
                    settings.concurrency,
                    cookie_source.as_ref(),
                    cancel,
                )
                .await
            }
            ProviderKind::GalleryDl => {
                let cookie_source = settings.cookie_source();
                gallery_dl_download(
                    sink,
                    id,
                    request,
                    engine_tools,
                    cookie_source.as_ref(),
                    cancel,
                )
                .await
            }
            ProviderKind::InstagramProxy => {
                instagram_proxy_download(sink, id, request, cancel).await
            }
            ProviderKind::HtmlProbe => {
                html_probe_download(sink, id, request, engine_tools, settings, cancel).await
            }
        };

        match result {
            Ok(path) => {
                let mut update = JobUpdate::new(id, "complete");
                update.provider = Some(provider);
                update.progress = Some(100.0);
                update.file_path = Some(path.to_string_lossy().to_string());
                update.title = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string());
                sink(update);
                return;
            }
            Err(error) if error == "cancelled" => {
                sink(JobUpdate::new(id, "cancelled"));
                return;
            }
            Err(error) => {
                failures.push(format!("{}: {error}", provider_name(provider)));
                let mut update = JobUpdate::new(id, "probing");
                update.detail = Some("Trying the next provider".to_string());
                sink(update);
            }
        }
    }

    let mut update = JobUpdate::new(id, "failed");
    let (message, error_code) = friendly_failure(&failures, settings);
    update.error = Some(message);
    update.error_code = error_code;
    sink(update);
}

fn provider_name(provider: ProviderKind) -> &'static str {
    match provider {
        ProviderKind::Direct => "Direct media",
        ProviderKind::ConfiguredApi => "Cobalt pool",
        ProviderKind::PublicApi => "Community Cobalt",
        ProviderKind::YtDlp => "yt-dlp",
        ProviderKind::GalleryDl => "gallery-dl",
        ProviderKind::InstagramProxy => "Instagram embed fallback",
        ProviderKind::HtmlProbe => "Page media scan",
    }
}

/// Tauri adapter: resolves tools, wires events, and runs the chain.
pub async fn run_job<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    request: DownloadRequest,
    settings: AppSettings,
    cancel: Arc<AtomicBool>,
) {
    let sink = |update: JobUpdate| {
        let _ = app.emit(DOWNLOAD_EVENT, update);
    };

    if cancel.load(Ordering::SeqCst) {
        sink(JobUpdate::new(&id, "cancelled"));
        return;
    }

    if let Err(error) = validate_remote_target(&request.url).await {
        let mut update = JobUpdate::new(&id, "failed");
        update.error = Some(error);
        sink(update);
        return;
    }

    let mut update = JobUpdate::new(&id, "probing");
    update.detail = Some("Resolving the best way to download".to_string());
    sink(update);

    // Make sure the engine tools exist before the first real download.
    if !tools::tools_report(&app).ready {
        let mut update = JobUpdate::new(&id, "probing");
        update.detail = Some("Setting up the download engine".to_string());
        sink(update);
        if let Err(error) = tools::ensure_tools(&app).await {
            let mut update = JobUpdate::new(&id, "failed");
            update.error = Some(format!("Could not set up the download engine: {error}"));
            sink(update);
            return;
        }
    }

    let yt_dlp = tools::locate_tool(&app, "yt-dlp");
    let mut gallery_dl = tools::locate_tool(&app, "gallery-dl");
    if provider_chain(&request, &settings).contains(&ProviderKind::GalleryDl)
        && !gallery_dl.available
    {
        if let Ok(installed) = tools::ensure_gallery_dl_tool(&app).await {
            gallery_dl = installed;
        }
    }
    let deno = tools::locate_tool(&app, "deno");
    let ffmpeg = tools::locate_tool(&app, "ffmpeg");
    let engine_tools = EngineTools {
        yt_dlp_path: yt_dlp.path,
        gallery_dl_path: gallery_dl.path,
        deno_path: deno.managed.then_some(deno.path).flatten(),
        ffmpeg_dir: match (ffmpeg.managed, ffmpeg.path) {
            (true, Some(path)) => Path::new(&path)
                .parent()
                .map(|dir| dir.to_string_lossy().to_string()),
            _ => None,
        },
    };

    run_chain(&sink, &id, &request, &settings, &engine_tools, &cancel).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ytdlp_progress_lines() {
        let parsed = parse_progress_line("  12.3%|2.35MiB/s|00:12");
        let (percent, speed, eta) = parsed.expect("parses");
        assert!((percent - 12.3).abs() < f64::EPSILON);
        assert_eq!(speed, "2.35MiB/s");
        assert_eq!(eta, "00:12");
        assert!(parse_progress_line("[download] Destination: x").is_none());
    }

    #[test]
    fn derives_filenames_from_urls() {
        assert_eq!(
            filename_from_url("https://cdn.example.com/media/My%20Clip.mp4?sig=1"),
            "My Clip.mp4"
        );
        assert_eq!(filename_from_url("https://example.com/"), "download.bin");
    }

    #[test]
    fn formats_speed_and_eta() {
        assert_eq!(format_bytes_per_sec(2_097_152.0), "2.0 MB/s");
        assert_eq!(format_eta(75.0), "1m 15s");
        assert_eq!(format_eta(f64::NAN), "--");
    }

    #[test]
    fn classifies_browser_cookie_failures() {
        let settings = AppSettings::default();
        let (_, locked_code) = friendly_failure(
            &["YtDlp: ERROR: Could not copy Chrome cookie database".to_string()],
            &settings,
        );
        assert_eq!(locked_code.as_deref(), Some("browser-cookies-locked"));

        let (_, decryption_code) = friendly_failure(
            &["YtDlp: ERROR: Failed to decrypt with DPAPI".to_string()],
            &settings,
        );
        assert_eq!(decryption_code.as_deref(), Some("cookie-decryption-failed"));
    }

    #[test]
    fn classifies_authentication_and_forbidden_failures() {
        let settings = AppSettings::default();
        let (message, auth_code) = friendly_failure(
            &["YtDlp: Account authentication is required".to_string()],
            &settings,
        );
        assert!(message.contains("requires sign-in"));
        assert_eq!(auth_code.as_deref(), Some("authentication-required"));

        let (_, forbidden_code) =
            friendly_failure(&["YtDlp: HTTP Error 403: Forbidden".to_string()], &settings);
        assert_eq!(forbidden_code.as_deref(), Some("forbidden"));
    }

    #[test]
    fn sends_api_credentials_only_for_the_selected_authentication_type() {
        let mut settings = ApiProviderSettings {
            token: "secret".to_string(),
            ..ApiProviderSettings::default()
        };
        assert_eq!(cobalt_authorization(&settings), None);

        settings.auth_type = "bearer".to_string();
        assert_eq!(
            cobalt_authorization(&settings),
            Some("Bearer secret".to_string())
        );

        settings.auth_type = "api-key".to_string();
        assert_eq!(
            cobalt_authorization(&settings),
            Some("Api-Key secret".to_string())
        );
    }

    #[tokio::test]
    async fn publishes_without_replacing_an_existing_download() {
        let directory =
            std::env::temp_dir().join(format!("mediafilez-desktop-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("creates test directory");
        let requested = directory.join("video.mp4");
        let temp = directory.join("video.part");
        std::fs::write(&requested, b"existing").expect("writes existing file");
        std::fs::write(&temp, b"new").expect("writes temporary file");

        let published = publish_without_overwrite(&temp, requested.clone())
            .await
            .expect("publishes safely");

        assert_eq!(
            std::fs::read(&requested).expect("reads old file"),
            b"existing"
        );
        assert_eq!(published, directory.join("video (1).mp4"));
        assert_eq!(std::fs::read(&published).expect("reads new file"), b"new");
        std::fs::remove_dir_all(&directory).expect("removes test directory");
    }

    #[test]
    fn skips_direct_provider_when_audio_wanted_from_video_file() {
        let settings = AppSettings::default();
        let mut request = DownloadRequest {
            url: "https://cdn.example.com/clip.mp4".to_string(),
            output_dir: "C:/Downloads".to_string(),
            mode: DownloadMode::Audio,
            video_quality: crate::models::VideoQuality::Best,
            audio_format: crate::models::AudioFormat::Mp3,
            audio_bitrate: "best".to_string(),
        };
        assert_eq!(
            provider_chain(&request, &settings),
            vec![ProviderKind::YtDlp]
        );

        request.mode = DownloadMode::Video;
        assert_eq!(
            provider_chain(&request, &settings),
            vec![
                ProviderKind::Direct,
                ProviderKind::YtDlp,
                ProviderKind::GalleryDl,
                ProviderKind::HtmlProbe
            ]
        );

        request.url = "https://cdn.example.com/master.m3u8".to_string();
        assert_eq!(
            provider_chain(&request, &settings),
            vec![
                ProviderKind::YtDlp,
                ProviderKind::GalleryDl,
                ProviderKind::HtmlProbe
            ]
        );

        let with_fallback = AppSettings {
            community_fallback: true,
            ..AppSettings::default()
        };
        assert!(provider_chain(&request, &with_fallback).contains(&ProviderKind::PublicApi));

        request.mode = DownloadMode::Image;
        request.url = "https://www.pinterest.com/pin/123/".to_string();
        assert_eq!(
            provider_chain(&request, &settings),
            vec![
                ProviderKind::GalleryDl,
                ProviderKind::YtDlp,
                ProviderKind::HtmlProbe
            ]
        );

        request.url = "https://www.instagram.com/reels/DcV3RyRz0sq/".to_string();
        let with_instagram_fallback = AppSettings {
            instagram_proxy_fallback: true,
            ..AppSettings::default()
        };
        assert!(provider_chain(&request, &with_instagram_fallback)
            .contains(&ProviderKind::InstagramProxy));
    }

    #[test]
    fn rejects_provider_media_that_does_not_match_the_selected_mode() {
        assert!(media_type_matches_mode(
            "image/jpeg",
            "https://cdn.example/item",
            &DownloadMode::Image
        ));
        assert!(!media_type_matches_mode(
            "video/mp4",
            "https://cdn.example/item",
            &DownloadMode::Image
        ));
        assert!(!media_type_matches_mode(
            "application/octet-stream",
            "https://cdn.example/audio.m4a",
            &DownloadMode::Video
        ));
        assert!(media_type_matches_mode(
            "application/octet-stream",
            "https://cdn.example/item",
            &DownloadMode::Video
        ));
    }
}
