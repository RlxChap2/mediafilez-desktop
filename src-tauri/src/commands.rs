use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{AppHandle, Manager, Runtime, State};

use crate::downloader::{run_job, JobManager};
use crate::models::{DownloadRequest, PlannedDownload, PreflightCheck, ProviderAttempt};
use crate::providers::plan_provider_order;
use crate::providers::yt_dlp::{build_ytdlp_args, requires_ffmpeg_for_audio};
use crate::security::{is_safe_download_filename, validate_api_endpoint, validate_remote_url};
use crate::settings::{cobalt_endpoints, default_output_folder, validate_cookie_file, AppSettings};
use crate::storage::validate_output_dir;
use crate::tools::{self, ToolUpdatesReport, ToolsReport};

pub struct AppState {
    pub settings: Mutex<AppSettings>,
    pub settings_path: PathBuf,
    pub jobs: JobManager,
}

impl AppState {
    pub fn init<R: Runtime>(app: &AppHandle<R>) -> Self {
        let settings_path = app
            .path()
            .app_config_dir()
            .map(|dir| dir.join("settings.json"))
            .unwrap_or_else(|_| PathBuf::from("settings.json"));
        Self {
            settings: Mutex::new(AppSettings::load(&settings_path)),
            settings_path,
            jobs: JobManager::default(),
        }
    }
}

#[tauri::command]
pub fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .settings
        .lock()
        .map(|settings| settings.clone())
        .map_err(|_| "Could not read app settings.".to_string())
}

#[tauri::command]
pub fn save_app_settings(
    mut settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<AppSettings, String> {
    settings.normalize();
    if settings.api_provider.enabled {
        for endpoint in cobalt_endpoints(&settings.api_provider.base_url) {
            validate_api_endpoint(&endpoint)?;
        }
    }
    validate_cookie_file(&settings.cookie_file)?;
    let mut stored = state
        .settings
        .lock()
        .map_err(|_| "Could not save app settings.".to_string())?;
    *stored = settings.clone();
    stored.save(&state.settings_path)?;
    Ok(settings)
}

#[tauri::command]
pub fn get_default_output_dir(state: State<'_, AppState>) -> String {
    let configured = state
        .settings
        .lock()
        .map(|settings| settings.output_folder.clone())
        .unwrap_or_default();
    if !configured.trim().is_empty() && Path::new(&configured).is_dir() {
        configured
    } else {
        default_output_folder()
    }
}

#[tauri::command]
pub fn check_tools<R: Runtime>(app: AppHandle<R>) -> ToolsReport {
    tools::tools_report(&app)
}

#[tauri::command]
pub async fn setup_tools<R: Runtime>(app: AppHandle<R>) -> Result<ToolsReport, String> {
    tools::refresh_tools(&app).await
}

#[tauri::command]
pub async fn check_tool_updates<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ToolUpdatesReport, String> {
    tools::check_tool_updates(&app).await
}

/// Starts a real download job. Returns the job id; progress arrives over the
/// `download://update` event channel.
#[tauri::command]
pub fn start_download<R: Runtime>(
    mut request: DownloadRequest,
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    if request.url.trim().is_empty() {
        return Err("Paste a link first.".to_string());
    }
    validate_remote_url(&request.url)?;
    if request.output_dir.trim().is_empty() {
        request.output_dir = get_default_output_dir(state.clone());
    }
    validate_output_dir(&request.output_dir)?;

    let settings = state
        .settings
        .lock()
        .map_err(|_| "Could not read settings.".to_string())?
        .clone();

    let id = uuid::Uuid::new_v4().to_string();
    let cancel = state.jobs.register(&id);
    let job_id = id.clone();
    let jobs = state.jobs.clone();
    tauri::async_runtime::spawn(async move {
        let _slot = jobs.wait_for_slot().await;
        run_job(app, job_id.clone(), request, settings, cancel).await;
        jobs.finish(&job_id);
    });
    Ok(id)
}

#[tauri::command]
pub fn cancel_download(id: String, state: State<'_, AppState>) -> bool {
    state.jobs.cancel(&id)
}

#[tauri::command]
pub fn open_file<R: Runtime>(path: String, app: AppHandle<R>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let target = Path::new(&path);
    if !target.is_file() {
        return Err("The downloaded file no longer exists.".to_string());
    }
    let filename = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "The file name is invalid.".to_string())?;
    if !is_safe_download_filename(filename) {
        return Err("Opening executable or shortcut files is blocked.".to_string());
    }
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|error| format!("Could not open the file: {error}"))
}

#[tauri::command]
pub fn show_in_folder<R: Runtime>(path: String, app: AppHandle<R>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if !Path::new(&path).exists() {
        return Err("The downloaded file no longer exists.".to_string());
    }
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|error| format!("Could not show the file: {error}"))
}

#[tauri::command]
pub fn open_link<R: Runtime>(url: String, app: AppHandle<R>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if url != "https://github.com/RlxChap2/mediafilez-desktop" {
        return Err("This external link is not allowed.".to_string());
    }
    app.opener()
        .open_url(url, None::<String>)
        .map_err(|error| format!("Could not open the link: {error}"))
}

#[tauri::command]
pub fn preflight<R: Runtime>(app: AppHandle<R>) -> Vec<PreflightCheck> {
    let report = tools::tools_report(&app);
    let tool_check = |status: &tools::ToolStatus, label: &str, hint: &str| PreflightCheck {
        id: status.name.clone(),
        label: label.to_string(),
        status: if status.available { "pass" } else { "warn" }.to_string(),
        detail: if status.available {
            let source = if status.managed {
                "managed by MediaFilez Desktop"
            } else {
                "system"
            };
            format!(
                "{} ({source})",
                status
                    .version
                    .clone()
                    .unwrap_or_else(|| "available".to_string())
            )
        } else {
            hint.to_string()
        },
    };

    vec![
        tool_check(
            &report.yt_dlp,
            "Download engine (yt-dlp)",
            "Will be downloaded automatically on first use.",
        ),
        tool_check(
            &report.gallery_dl,
            "Gallery engine (gallery-dl)",
            "Will be downloaded automatically when an image or gallery needs it.",
        ),
        tool_check(
            &report.deno,
            "YouTube challenge runtime (Deno)",
            "Will be downloaded automatically for full YouTube support.",
        ),
        tool_check(
            &report.ffmpeg,
            "Media converter (FFmpeg)",
            "Will be downloaded automatically when conversion is needed.",
        ),
    ]
}

#[tauri::command]
pub fn plan_download(
    request: DownloadRequest,
    state: State<'_, AppState>,
) -> Result<PlannedDownload, String> {
    validate_output_dir(&request.output_dir)?;

    let settings = state
        .settings
        .lock()
        .map_err(|_| "Could not read provider settings.".to_string())?
        .clone();
    let api_enabled =
        settings.api_provider.enabled && !settings.api_provider.base_url.trim().is_empty();
    let provider_order = plan_provider_order(api_enabled);
    let output_template = Path::new(&request.output_dir)
        .join("%(title)s.%(ext)s")
        .to_string_lossy()
        .to_string();
    let ytdlp_args = build_ytdlp_args(
        &request.url,
        request.mode.clone(),
        request.video_quality,
        request.audio_format,
        &request.audio_bitrate,
        &output_template,
        settings.concurrency,
    );

    let attempts = provider_order
        .iter()
        .map(|provider| ProviderAttempt {
            provider: *provider,
            status: "planned".to_string(),
            detail: "Provider will be tried in this order.".to_string(),
        })
        .collect();

    Ok(PlannedDownload {
        url: request.url,
        output_dir: request.output_dir,
        provider_order,
        ytdlp_args,
        requires_ffmpeg: requires_ffmpeg_for_audio(request.audio_format),
        attempts,
    })
}
