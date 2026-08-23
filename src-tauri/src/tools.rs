//! Finds and provisions yt-dlp, gallery-dl, Deno, and FFmpeg with publisher checksums.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

pub const SETUP_EVENT: &str = "setup://progress";
const APP_USER_AGENT: &str = concat!("MediaFilez Desktop/", env!("CARGO_PKG_VERSION"));

const YTDLP_WINDOWS_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
const YTDLP_MACOS_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos";
const YTDLP_LINUX_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux";
const YTDLP_CHECKSUMS_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS";
const GALLERY_DL_RELEASE_API: &str =
    "https://codeberg.org/api/v1/repos/mikf/gallery-dl/releases/latest";
const DENO_WINDOWS_X64_ASSET: &str = "deno-x86_64-pc-windows-msvc.zip";
const DENO_WINDOWS_ARM64_ASSET: &str = "deno-aarch64-pc-windows-msvc.zip";
const DENO_MACOS_X64_ASSET: &str = "deno-x86_64-apple-darwin.zip";
const DENO_MACOS_ARM64_ASSET: &str = "deno-aarch64-apple-darwin.zip";
const DENO_LINUX_X64_ASSET: &str = "deno-x86_64-unknown-linux-gnu.zip";
const DENO_LINUX_ARM64_ASSET: &str = "deno-aarch64-unknown-linux-gnu.zip";
// ffmpeg.org's recommended Windows builds; "essentials" carries everything
// yt-dlp needs for merging and audio conversion.
const FFMPEG_WINDOWS_ZIP_URL: &str =
    "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
const FFMPEG_WINDOWS_SHA256_URL: &str =
    "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip.sha256";
const MAX_METADATA_BYTES: usize = 1024 * 1024;
const TOOL_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub name: String,
    pub available: bool,
    pub path: Option<String>,
    pub managed: bool,
    pub version: Option<String>,
    pub verified: bool,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsReport {
    pub yt_dlp: ToolStatus,
    pub gallery_dl: ToolStatus,
    pub deno: ToolStatus,
    pub ffmpeg: ToolStatus,
    pub ready: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUpdateStatus {
    pub name: String,
    pub managed: bool,
    pub update_available: Option<bool>,
    pub current_version: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUpdatesReport {
    pub yt_dlp: ToolUpdateStatus,
    pub gallery_dl: ToolUpdateStatus,
    pub deno: ToolUpdateStatus,
    pub ffmpeg: ToolUpdateStatus,
    pub updates_available: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupProgress {
    pub tool: String,
    pub phase: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub message: String,
}

/// Progress callback for provisioning; wired to `setup://progress` in the app.
pub type SetupSink<'a> = &'a (dyn Fn(SetupProgress) + Send + Sync);

#[derive(serde::Deserialize)]
struct GalleryRelease {
    tag_name: String,
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

fn probe_version(command: &str, args: &[&str]) -> Option<String> {
    let mut cmd = std::process::Command::new(command);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|line| line.trim().to_string())
}

fn checksum_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("tool");
    path.with_file_name(format!("{file_name}.sha256"))
}

fn ffmpeg_release_checksum_path(tools_dir: &Path) -> PathBuf {
    tools_dir.join("ffmpeg-release.sha256")
}

fn deno_release_checksum_path(tools_dir: &Path) -> PathBuf {
    tools_dir.join("deno-release.sha256")
}

fn gallery_release_checksum_path(tools_dir: &Path) -> PathBuf {
    tools_dir.join("gallery-dl-release.sha256")
}

fn gallery_release_asset() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86")) {
        Some("gallery-dl_x86.exe")
    } else if cfg!(windows) {
        Some("gallery-dl.exe")
    } else {
        None
    }
}

fn deno_release_asset() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some(DENO_WINDOWS_X64_ASSET)
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        Some(DENO_WINDOWS_ARM64_ASSET)
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some(DENO_MACOS_X64_ASSET)
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(DENO_MACOS_ARM64_ASSET)
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(DENO_LINUX_X64_ASSET)
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some(DENO_LINUX_ARM64_ASSET)
    } else {
        None
    }
}

fn deno_release_url(asset: &str) -> String {
    format!("https://github.com/denoland/deno/releases/latest/download/{asset}")
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    use std::fmt::Write as _;

    let bytes = bytes.as_ref();
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("Could not read downloaded tool: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|error| format!("Could not verify downloaded tool: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_digest(hasher.finalize()))
}

fn managed_integrity(path: &Path) -> Option<String> {
    let expected = std::fs::read_to_string(checksum_path(path)).ok()?;
    let expected = expected.split_whitespace().next()?.to_ascii_lowercase();
    let actual = sha256_file(path).ok()?;
    (actual == expected).then_some(actual)
}

fn write_local_checksum(path: &Path) -> Result<String, String> {
    let digest = sha256_file(path)?;
    std::fs::write(checksum_path(path), format!("{digest}\n"))
        .map_err(|error| format!("Could not store tool checksum: {error}"))?;
    Ok(digest)
}

/// Finds a tool, preferring PATH, then the managed copy in `tools_dir`.
pub fn locate_tool_in(tools_dir: Option<&Path>, base_name: &str) -> ToolStatus {
    let version_args: &[&str] = if base_name == "ffmpeg" {
        &["-version"]
    } else {
        &["--version"]
    };

    if let Some(version) = probe_version(base_name, version_args) {
        return ToolStatus {
            name: base_name.to_string(),
            available: true,
            path: Some(base_name.to_string()),
            managed: false,
            version: Some(version),
            verified: false,
            sha256: None,
        };
    }

    if let Some(dir) = tools_dir {
        let managed = dir.join(exe_name(base_name));
        if managed.exists() {
            let checksum = managed_integrity(&managed);
            if checksum.is_none() {
                return ToolStatus {
                    name: base_name.to_string(),
                    available: false,
                    path: None,
                    managed: true,
                    version: None,
                    verified: false,
                    sha256: None,
                };
            }
            let version = probe_version(&managed.to_string_lossy(), version_args);
            return ToolStatus {
                name: base_name.to_string(),
                available: true,
                path: Some(managed.to_string_lossy().to_string()),
                managed: true,
                version,
                verified: true,
                sha256: checksum,
            };
        }
    }

    ToolStatus {
        name: base_name.to_string(),
        available: false,
        path: None,
        managed: false,
        version: None,
        verified: false,
        sha256: None,
    }
}

async fn fetch_text(url: &str, label: &str) -> Result<String, String> {
    let response = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("HTTP client error: {error}"))?
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Could not download {label}: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Download of {label} failed: {error}"))?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_METADATA_BYTES as u64)
    {
        return Err(format!("{label} exceeded the metadata safety limit."));
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("Could not read {label}: {error}"))?;
        if body.len().saturating_add(chunk.len()) > MAX_METADATA_BYTES {
            return Err(format!("{label} exceeded the metadata safety limit."));
        }
        body.extend_from_slice(&chunk);
    }
    String::from_utf8(body).map_err(|_| format!("{label} was not valid UTF-8."))
}

fn checksum_for_asset(checksums: &str, asset_name: &str) -> Option<String> {
    checksums.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let digest = fields.next()?;
        let name = fields.next()?.trim_start_matches('*');
        (name == asset_name && digest.len() == 64).then(|| digest.to_ascii_lowercase())
    })
}

async fn gallery_release() -> Result<(String, String, String), String> {
    let metadata = fetch_text(GALLERY_DL_RELEASE_API, "gallery-dl release metadata").await?;
    let release: GalleryRelease = serde_json::from_str(&metadata)
        .map_err(|error| format!("gallery-dl release metadata was invalid: {error}"))?;
    let asset = gallery_release_asset()
        .ok_or_else(|| "Managed gallery-dl is not available for this platform.".to_string())?;
    let base = format!(
        "https://codeberg.org/mikf/gallery-dl/releases/download/{}",
        release.tag_name
    );
    let checksums = fetch_text(&format!("{base}/SHA256SUMS"), "gallery-dl checksums").await?;
    let expected = checksum_for_asset(&checksums, asset).ok_or_else(|| {
        "The gallery-dl release did not publish a checksum for this platform.".to_string()
    })?;
    Ok((format!("{base}/{asset}"), asset.to_string(), expected))
}

fn report(
    sink: SetupSink<'_>,
    tool: &str,
    phase: &str,
    downloaded: u64,
    total: Option<u64>,
    message: &str,
) {
    sink(SetupProgress {
        tool: tool.to_string(),
        phase: phase.to_string(),
        downloaded_bytes: downloaded,
        total_bytes: total,
        message: message.to_string(),
    });
}

async fn download_file(
    sink: SetupSink<'_>,
    tool: &str,
    url: &str,
    destination: &Path,
    expected_sha256: &str,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .timeout(TOOL_DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|error| format!("HTTP client error: {error}"))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Could not download {tool}: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Download of {tool} failed: {error}"))?;

    let total = response.content_length();
    let destination_name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(tool);
    let temp_path =
        destination.with_file_name(format!(".{destination_name}.{}.part", Uuid::new_v4()));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .await
        .map_err(|error| format!("Could not write {tool}: {error}"))?;

    let mut downloaded: u64 = 0;
    let mut hasher = Sha256::new();
    let mut last_emit = std::time::Instant::now();
    let mut stream = response.bytes_stream();
    let write_result: Result<(), String> = async {
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|error| format!("Download of {tool} interrupted: {error}"))?;
            file.write_all(&chunk)
                .await
                .map_err(|error| format!("Could not write {tool}: {error}"))?;
            downloaded += chunk.len() as u64;
            hasher.update(&chunk);
            if last_emit.elapsed().as_millis() > 250 {
                report(sink, tool, "downloading", downloaded, total, "Downloading");
                last_emit = std::time::Instant::now();
            }
        }
        file.flush()
            .await
            .map_err(|error| format!("Could not finish {tool}: {error}"))
    }
    .await;
    drop(file);
    if let Err(error) = write_result {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(error);
    }

    report(
        sink,
        tool,
        "verifying",
        downloaded,
        total,
        "Verifying SHA-256",
    );
    let actual_sha256 = hex_digest(hasher.finalize());
    if actual_sha256 != expected_sha256.to_ascii_lowercase() {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(format!(
            "{tool} checksum did not match its publisher. The file was deleted."
        ));
    }

    let backup_path =
        destination.with_file_name(format!(".{destination_name}.{}.backup", Uuid::new_v4()));
    let had_previous = destination.exists();
    if had_previous {
        if let Err(error) = tokio::fs::rename(destination, &backup_path).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(format!("Could not preserve old {tool}: {error}"));
        }
    }

    if let Err(error) = tokio::fs::rename(&temp_path, destination).await {
        let restore_error = if had_previous {
            tokio::fs::rename(&backup_path, destination).await.err()
        } else {
            None
        };
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(match restore_error {
            Some(restore_error) => format!(
                "Could not finalize {tool}: {error}. The previous file is at {} because restoration also failed: {restore_error}",
                backup_path.display()
            ),
            None => format!("Could not finalize {tool}: {error}"),
        });
    }
    if had_previous {
        let _ = tokio::fs::remove_file(&backup_path).await;
    }
    tokio::fs::write(checksum_path(destination), format!("{actual_sha256}\n"))
        .await
        .map_err(|error| format!("Could not store {tool} checksum: {error}"))?;
    report(
        sink,
        tool,
        "downloaded",
        downloaded,
        total,
        "Download complete",
    );
    Ok(())
}

async fn ensure_yt_dlp(
    tools_dir: &Path,
    sink: SetupSink<'_>,
    refresh_managed: bool,
) -> Result<ToolStatus, String> {
    let existing = locate_tool_in(Some(tools_dir), "yt-dlp");
    if existing.available && (!refresh_managed || !existing.managed) {
        return Ok(existing);
    }

    let (url, asset_name) = if cfg!(windows) {
        (YTDLP_WINDOWS_URL, "yt-dlp.exe")
    } else if cfg!(target_os = "macos") {
        (YTDLP_MACOS_URL, "yt-dlp_macos")
    } else {
        (YTDLP_LINUX_URL, "yt-dlp_linux")
    };

    report(
        sink,
        "yt-dlp",
        "verifying",
        0,
        None,
        "Fetching publisher checksum",
    );
    let checksums = fetch_text(YTDLP_CHECKSUMS_URL, "yt-dlp checksums").await?;
    let expected = checksum_for_asset(&checksums, asset_name).ok_or_else(|| {
        "The yt-dlp release did not publish a checksum for this platform.".to_string()
    })?;

    let destination = tools_dir.join(exe_name("yt-dlp"));
    download_file(sink, "yt-dlp", url, &destination, &expected).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o755));
    }

    report(sink, "yt-dlp", "ready", 0, None, "yt-dlp is ready");
    Ok(locate_tool_in(Some(tools_dir), "yt-dlp"))
}

async fn ensure_gallery_dl(
    tools_dir: &Path,
    sink: SetupSink<'_>,
    refresh_managed: bool,
) -> Result<ToolStatus, String> {
    let existing = locate_tool_in(Some(tools_dir), "gallery-dl");
    if existing.available && (!refresh_managed || !existing.managed) {
        return Ok(existing);
    }
    if gallery_release_asset().is_none() {
        return Ok(existing);
    }

    report(
        sink,
        "gallery-dl",
        "verifying",
        0,
        None,
        "Fetching publisher checksum",
    );
    let (url, _, expected) = gallery_release().await?;
    let destination = tools_dir.join(exe_name("gallery-dl"));
    download_file(sink, "gallery-dl", &url, &destination, &expected).await?;
    tokio::fs::write(
        gallery_release_checksum_path(tools_dir),
        format!("{expected}\n"),
    )
    .await
    .map_err(|error| format!("Could not store gallery-dl release checksum: {error}"))?;
    report(sink, "gallery-dl", "ready", 0, None, "gallery-dl is ready");
    Ok(locate_tool_in(Some(tools_dir), "gallery-dl"))
}

fn extract_deno_from_zip(zip_path: &Path, target_dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path)
        .map_err(|error| format!("Could not open Deno archive: {error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Could not read Deno archive: {error}"))?;
    let expected_name = exe_name("deno");
    let mut found = false;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Bad Deno archive entry: {error}"))?;
        let Some(path) = entry.enclosed_name() else {
            return Err("Deno archive contained an unsafe path.".to_string());
        };
        if path.file_name().and_then(|name| name.to_str()) != Some(expected_name.as_str()) {
            continue;
        }
        if entry.size() > 300 * 1024 * 1024 {
            return Err("Deno executable is unexpectedly large.".to_string());
        }

        let destination = target_dir.join(&expected_name);
        let temp = target_dir.join(format!(".{expected_name}.{}.part", Uuid::new_v4()));
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| format!("Could not write Deno: {error}"))?;
        if let Err(error) = std::io::copy(&mut entry, &mut output) {
            drop(output);
            let _ = std::fs::remove_file(&temp);
            return Err(format!("Could not extract Deno: {error}"));
        }
        drop(output);
        replace_local_file(&temp, &destination, "Deno")?;
        write_local_checksum(&destination)?;
        found = true;
        break;
    }

    if !found {
        return Err("Deno archive did not contain the expected executable.".to_string());
    }
    Ok(())
}

async fn ensure_deno(
    tools_dir: &Path,
    sink: SetupSink<'_>,
    refresh_managed: bool,
) -> Result<ToolStatus, String> {
    let existing = locate_tool_in(Some(tools_dir), "deno");
    if existing.available && (!refresh_managed || !existing.managed) {
        return Ok(existing);
    }

    let Some(asset) = deno_release_asset() else {
        return Ok(existing);
    };
    let url = deno_release_url(asset);
    let checksum_url = format!("{url}.sha256sum");
    report(
        sink,
        "Deno",
        "verifying",
        0,
        None,
        "Fetching publisher checksum",
    );
    let checksum_text = fetch_text(&checksum_url, "Deno checksum").await?;
    let expected = checksum_text
        .split_whitespace()
        .find(|value| value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()))
        .ok_or_else(|| "Deno publisher checksum was invalid.".to_string())?;

    let zip_path = tools_dir.join("deno-download.zip");
    download_file(sink, "Deno", &url, &zip_path, expected).await?;
    report(sink, "Deno", "extracting", 0, None, "Unpacking Deno");
    let dir_clone = tools_dir.to_path_buf();
    let zip_clone = zip_path.clone();
    tokio::task::spawn_blocking(move || extract_deno_from_zip(&zip_clone, &dir_clone))
        .await
        .map_err(|error| format!("Deno extraction crashed: {error}"))??;
    let _ = tokio::fs::remove_file(&zip_path).await;
    let _ = tokio::fs::remove_file(checksum_path(&zip_path)).await;
    tokio::fs::write(
        deno_release_checksum_path(tools_dir),
        format!("{}\n", expected.to_ascii_lowercase()),
    )
    .await
    .map_err(|error| format!("Could not store Deno release checksum: {error}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let destination = tools_dir.join(exe_name("deno"));
        let _ = std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o755));
    }

    report(sink, "Deno", "ready", 0, None, "Deno is ready");
    Ok(locate_tool_in(Some(tools_dir), "deno"))
}

fn extract_ffmpeg_from_zip(zip_path: &Path, target_dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path)
        .map_err(|error| format!("Could not open FFmpeg archive: {error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Could not read FFmpeg archive: {error}"))?;

    let wanted = ["ffmpeg.exe", "ffprobe.exe"];
    let mut extracted = 0;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Bad FFmpeg archive entry: {error}"))?;
        let name = entry.name().replace('\\', "/");
        let Some(file_name) = name.rsplit('/').next() else {
            continue;
        };
        if !name.contains("/bin/") || !wanted.contains(&file_name) {
            continue;
        }
        if entry.size() > 300 * 1024 * 1024 {
            return Err(format!(
                "FFmpeg archive entry {file_name} is unexpectedly large."
            ));
        }
        let destination = target_dir.join(file_name);
        let temp = target_dir.join(format!(".{file_name}.{}.part", Uuid::new_v4()));
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| format!("Could not write {file_name}: {error}"))?;
        if let Err(error) = std::io::copy(&mut entry, &mut output) {
            drop(output);
            let _ = std::fs::remove_file(&temp);
            return Err(format!("Could not extract {file_name}: {error}"));
        }
        drop(output);
        replace_local_file(&temp, &destination, file_name)?;
        extracted += 1;
    }

    if extracted != wanted.len() {
        return Err("FFmpeg archive did not contain expected binaries.".to_string());
    }
    for file_name in wanted {
        let path = target_dir.join(file_name);
        if path.exists() {
            write_local_checksum(&path)?;
        }
    }
    Ok(())
}

fn replace_local_file(source: &Path, destination: &Path, label: &str) -> Result<(), String> {
    let backup = destination.with_file_name(format!(".{label}.{}.backup", Uuid::new_v4()));
    let had_previous = destination.exists();
    if had_previous {
        if let Err(error) = std::fs::rename(destination, &backup) {
            let _ = std::fs::remove_file(source);
            return Err(format!("Could not preserve old {label}: {error}"));
        }
    }

    if let Err(error) = std::fs::rename(source, destination) {
        let restore_error = had_previous
            .then(|| std::fs::rename(&backup, destination).err())
            .flatten();
        let _ = std::fs::remove_file(source);
        return Err(match restore_error {
            Some(restore_error) => format!(
                "Could not install {label}: {error}. The previous file is at {} because restoration also failed: {restore_error}",
                backup.display()
            ),
            None => format!("Could not install {label}: {error}"),
        });
    }
    if had_previous {
        let _ = std::fs::remove_file(backup);
    }
    Ok(())
}

async fn ensure_ffmpeg(
    tools_dir: &Path,
    sink: SetupSink<'_>,
    refresh_managed: bool,
) -> Result<ToolStatus, String> {
    let existing = locate_tool_in(Some(tools_dir), "ffmpeg");
    if existing.available && (!refresh_managed || !existing.managed) {
        return Ok(existing);
    }

    if !cfg!(windows) {
        // On macOS and Linux, use FFmpeg installed by the system package manager.
        return Ok(existing);
    }

    let zip_path = tools_dir.join("ffmpeg-download.zip");
    report(
        sink,
        "ffmpeg",
        "verifying",
        0,
        None,
        "Fetching publisher checksum",
    );
    let checksum_text = fetch_text(FFMPEG_WINDOWS_SHA256_URL, "FFmpeg checksum").await?;
    let expected = checksum_text
        .split_whitespace()
        .find(|value| value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()))
        .ok_or_else(|| "FFmpeg publisher checksum was invalid.".to_string())?;
    download_file(sink, "ffmpeg", FFMPEG_WINDOWS_ZIP_URL, &zip_path, expected).await?;

    report(sink, "ffmpeg", "extracting", 0, None, "Unpacking FFmpeg");
    let dir_clone = tools_dir.to_path_buf();
    let zip_clone = zip_path.clone();
    tokio::task::spawn_blocking(move || extract_ffmpeg_from_zip(&zip_clone, &dir_clone))
        .await
        .map_err(|error| format!("FFmpeg extraction crashed: {error}"))??;
    let _ = tokio::fs::remove_file(&zip_path).await;
    let _ = tokio::fs::remove_file(checksum_path(&zip_path)).await;
    tokio::fs::write(
        ffmpeg_release_checksum_path(tools_dir),
        format!("{}\n", expected.to_ascii_lowercase()),
    )
    .await
    .map_err(|error| format!("Could not store FFmpeg release checksum: {error}"))?;

    report(sink, "ffmpeg", "ready", 0, None, "FFmpeg is ready");
    Ok(locate_tool_in(Some(tools_dir), "ffmpeg"))
}

/// Makes sure every tool is present, downloading missing ones into
/// `tools_dir`. Safe to call repeatedly; only missing tools trigger downloads.
pub async fn ensure_tools_in(tools_dir: &Path, sink: SetupSink<'_>) -> Result<ToolsReport, String> {
    std::fs::create_dir_all(tools_dir)
        .map_err(|error| format!("Could not create tools dir: {error}"))?;
    let yt_dlp = ensure_yt_dlp(tools_dir, sink, false).await?;
    let gallery_dl = ensure_gallery_dl(tools_dir, sink, false)
        .await
        .unwrap_or_else(|error| {
            report(sink, "gallery-dl", "error", 0, None, &error);
            locate_tool_in(Some(tools_dir), "gallery-dl")
        });
    let deno = ensure_deno(tools_dir, sink, false)
        .await
        .unwrap_or_else(|error| {
            report(sink, "Deno", "error", 0, None, &error);
            locate_tool_in(Some(tools_dir), "deno")
        });
    let ffmpeg = ensure_ffmpeg(tools_dir, sink, false)
        .await
        .unwrap_or_else(|error| {
            // FFmpeg failing to provision must not block plain downloads.
            report(sink, "ffmpeg", "error", 0, None, &error);
            locate_tool_in(Some(tools_dir), "ffmpeg")
        });
    let ready = yt_dlp.available && deno.available;
    Ok(ToolsReport {
        yt_dlp,
        gallery_dl,
        deno,
        ffmpeg,
        ready,
    })
}

pub async fn refresh_tools_in(
    tools_dir: &Path,
    sink: SetupSink<'_>,
) -> Result<ToolsReport, String> {
    std::fs::create_dir_all(tools_dir)
        .map_err(|error| format!("Could not create tools dir: {error}"))?;
    let yt_dlp = ensure_yt_dlp(tools_dir, sink, true).await?;
    let gallery_dl = ensure_gallery_dl(tools_dir, sink, true)
        .await
        .unwrap_or_else(|error| {
            report(sink, "gallery-dl", "error", 0, None, &error);
            locate_tool_in(Some(tools_dir), "gallery-dl")
        });
    let deno = ensure_deno(tools_dir, sink, true)
        .await
        .unwrap_or_else(|error| {
            report(sink, "Deno", "error", 0, None, &error);
            locate_tool_in(Some(tools_dir), "deno")
        });
    let ffmpeg = ensure_ffmpeg(tools_dir, sink, true)
        .await
        .unwrap_or_else(|error| {
            report(sink, "ffmpeg", "error", 0, None, &error);
            locate_tool_in(Some(tools_dir), "ffmpeg")
        });
    Ok(ToolsReport {
        ready: yt_dlp.available && deno.available,
        yt_dlp,
        gallery_dl,
        deno,
        ffmpeg,
    })
}

pub async fn check_tool_updates_in(tools_dir: &Path) -> Result<ToolUpdatesReport, String> {
    let yt_dlp = locate_tool_in(Some(tools_dir), "yt-dlp");
    let gallery_dl = locate_tool_in(Some(tools_dir), "gallery-dl");
    let deno = locate_tool_in(Some(tools_dir), "deno");
    let ffmpeg = locate_tool_in(Some(tools_dir), "ffmpeg");

    let yt_update = if yt_dlp.managed && yt_dlp.available {
        let (_, asset_name) = if cfg!(windows) {
            (YTDLP_WINDOWS_URL, "yt-dlp.exe")
        } else if cfg!(target_os = "macos") {
            (YTDLP_MACOS_URL, "yt-dlp_macos")
        } else {
            (YTDLP_LINUX_URL, "yt-dlp_linux")
        };
        let checksums = fetch_text(YTDLP_CHECKSUMS_URL, "yt-dlp checksums").await?;
        let latest = checksum_for_asset(&checksums, asset_name).ok_or_else(|| {
            "The yt-dlp release did not publish a checksum for this platform.".to_string()
        })?;
        yt_dlp.sha256.as_ref().map(|current| current != &latest)
    } else {
        None
    };

    let deno_update = if deno.managed && deno.available {
        let asset = deno_release_asset()
            .ok_or_else(|| "Deno does not publish an asset for this platform.".to_string())?;
        let checksum_url = format!("{}.sha256sum", deno_release_url(asset));
        let latest = fetch_text(&checksum_url, "Deno checksum").await?;
        let latest = latest
            .split_whitespace()
            .find(|value| value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()))
            .ok_or_else(|| "Deno publisher checksum was invalid.".to_string())?;
        std::fs::read_to_string(deno_release_checksum_path(tools_dir))
            .ok()
            .and_then(|value| value.split_whitespace().next().map(str::to_owned))
            .map(|current| !current.eq_ignore_ascii_case(latest))
    } else {
        None
    };

    let gallery_update = if gallery_dl.managed && gallery_dl.available {
        let (_, _, latest) = gallery_release().await?;
        std::fs::read_to_string(gallery_release_checksum_path(tools_dir))
            .ok()
            .and_then(|value| value.split_whitespace().next().map(str::to_owned))
            .map(|current| !current.eq_ignore_ascii_case(&latest))
    } else {
        None
    };

    let ffmpeg_update = if cfg!(windows) && ffmpeg.managed && ffmpeg.available {
        let latest = fetch_text(FFMPEG_WINDOWS_SHA256_URL, "FFmpeg checksum").await?;
        let latest = latest
            .split_whitespace()
            .find(|value| value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()))
            .ok_or_else(|| "FFmpeg publisher checksum was invalid.".to_string())?;
        std::fs::read_to_string(ffmpeg_release_checksum_path(tools_dir))
            .ok()
            .and_then(|value| value.split_whitespace().next().map(str::to_owned))
            .map(|current| !current.eq_ignore_ascii_case(latest))
    } else {
        None
    };

    Ok(ToolUpdatesReport {
        updates_available: yt_update == Some(true)
            || gallery_update == Some(true)
            || deno_update == Some(true)
            || ffmpeg_update == Some(true),
        yt_dlp: ToolUpdateStatus {
            name: "yt-dlp".to_string(),
            managed: yt_dlp.managed,
            update_available: yt_update,
            current_version: yt_dlp.version,
        },
        gallery_dl: ToolUpdateStatus {
            name: "gallery-dl".to_string(),
            managed: gallery_dl.managed,
            update_available: gallery_update,
            current_version: gallery_dl.version,
        },
        deno: ToolUpdateStatus {
            name: "Deno".to_string(),
            managed: deno.managed,
            update_available: deno_update,
            current_version: deno.version,
        },
        ffmpeg: ToolUpdateStatus {
            name: "FFmpeg".to_string(),
            managed: ffmpeg.managed,
            update_available: ffmpeg_update,
            current_version: ffmpeg.version,
        },
    })
}

// Tauri adapters

pub fn tools_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Could not resolve app data dir: {error}"))?
        .join("bin");
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not create tools dir: {error}"))?;
    Ok(dir)
}

pub fn locate_tool<R: Runtime>(app: &AppHandle<R>, base_name: &str) -> ToolStatus {
    locate_tool_in(tools_dir(app).ok().as_deref(), base_name)
}

pub fn tools_report<R: Runtime>(app: &AppHandle<R>) -> ToolsReport {
    let yt_dlp = locate_tool(app, "yt-dlp");
    let gallery_dl = locate_tool(app, "gallery-dl");
    let deno = locate_tool(app, "deno");
    let ffmpeg = locate_tool(app, "ffmpeg");
    let ready = yt_dlp.available && deno.available;
    ToolsReport {
        yt_dlp,
        gallery_dl,
        deno,
        ffmpeg,
        ready,
    }
}

pub async fn ensure_tools<R: Runtime>(app: &AppHandle<R>) -> Result<ToolsReport, String> {
    let _guard = tool_operation_lock().lock().await;
    let dir = tools_dir(app)?;
    let sink = |progress: SetupProgress| {
        let _ = app.emit(SETUP_EVENT, progress);
    };
    ensure_tools_in(&dir, &sink).await
}

pub async fn refresh_tools<R: Runtime>(app: &AppHandle<R>) -> Result<ToolsReport, String> {
    let _guard = tool_operation_lock().lock().await;
    let dir = tools_dir(app)?;
    let sink = |progress: SetupProgress| {
        let _ = app.emit(SETUP_EVENT, progress);
    };
    refresh_tools_in(&dir, &sink).await
}

pub async fn ensure_gallery_dl_tool<R: Runtime>(app: &AppHandle<R>) -> Result<ToolStatus, String> {
    let _guard = tool_operation_lock().lock().await;
    let dir = tools_dir(app)?;
    let sink = |progress: SetupProgress| {
        let _ = app.emit(SETUP_EVENT, progress);
    };
    ensure_gallery_dl(&dir, &sink, false).await
}

pub async fn check_tool_updates<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<ToolUpdatesReport, String> {
    let _guard = tool_operation_lock().lock().await;
    check_tool_updates_in(&tools_dir(app)?).await
}

fn tool_operation_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_digest_bytes_as_lowercase_hex() {
        assert_eq!(hex_digest([0x00, 0x01, 0xab, 0xff]), "0001abff");
    }

    #[test]
    fn selects_only_the_exact_release_asset_checksum() {
        let checksums = concat!(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  yt-dlp\n",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb *yt-dlp.exe\n",
        );
        assert_eq!(
            checksum_for_asset(checksums, "yt-dlp.exe").as_deref(),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
        assert!(checksum_for_asset(checksums, "missing.exe").is_none());
    }

    #[tokio::test]
    #[ignore = "network: downloads the official gallery-dl executable"]
    async fn provisions_gallery_dl_with_publisher_checksum() {
        let directory =
            std::env::temp_dir().join(format!("mediafilez-gallery-tool-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("creates tool test directory");

        let status = ensure_gallery_dl(&directory, &|_progress| {}, false)
            .await
            .expect("gallery-dl provision succeeds");
        assert!(status.available, "gallery-dl is available after provision");
        if status.managed {
            assert!(status.verified, "managed gallery-dl passes SHA-256");
        }

        std::fs::remove_dir_all(&directory).expect("removes tool test directory");
    }
}
