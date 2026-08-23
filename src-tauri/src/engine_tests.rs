//! Network integration tests for the download engine. Ignored by default so
//! `cargo test` stays offline; run explicitly with:
//!
//! ```powershell
//! cargo test -- --ignored
//! ```

use std::sync::atomic::AtomicBool;

use crate::downloader::{stream_to_file, ytdlp_download, EngineTools, JobUpdate, StreamTarget};
use crate::models::{AudioFormat, DownloadMode, DownloadRequest, ProviderKind, VideoQuality};
use crate::tools;

fn noop_job_sink(_update: JobUpdate) {}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir()
        .join("mediafilez-desktop-engine-tests")
        .join(name);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
#[ignore = "network: streams a real file to disk"]
fn direct_stream_download_works() {
    let out_dir = temp_dir("direct");
    let cancel = AtomicBool::new(false);

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let path = runtime
        .block_on(stream_to_file(
            &noop_job_sink,
            "test-job",
            StreamTarget {
                provider: ProviderKind::Direct,
                mode: &DownloadMode::Video,
                url: "https://interactive-examples.mdn.mozilla.net/media/cc0-videos/flower.mp4",
                output_dir: &out_dir.to_string_lossy(),
                file_name_hint: None,
            },
            &cancel,
        ))
        .expect("download succeeds");

    let metadata = std::fs::metadata(&path).expect("file exists");
    assert!(metadata.len() > 1_000, "downloaded file has content");
    let _ = std::fs::remove_file(path);
}

#[test]
#[ignore = "network: downloads yt-dlp from official releases and runs it"]
fn provisions_ytdlp_and_runs_it() {
    let tools_dir = temp_dir("tools");

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let report = runtime
        .block_on(tools::ensure_tools_in(&tools_dir, &|_progress| {}))
        .expect("tools provision");

    assert!(report.yt_dlp.available, "yt-dlp available after ensure");
    assert!(report.deno.available, "Deno available after ensure");
    assert!(report.ready, "the full download engine is ready");
    assert!(
        report.yt_dlp.version.is_some(),
        "yt-dlp runs and reports a version: {:?}",
        report.yt_dlp
    );
}

#[test]
#[ignore = "network: full audio-only download through the managed yt-dlp"]
fn downloads_audio_with_managed_ytdlp() {
    let tools_dir = temp_dir("tools");
    let out_dir = temp_dir("media");
    let cancel = AtomicBool::new(false);

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let report = runtime
        .block_on(tools::ensure_tools_in(&tools_dir, &|_progress| {}))
        .expect("tools provision");
    assert!(report.yt_dlp.available);

    let engine_tools = EngineTools {
        yt_dlp_path: report.yt_dlp.path,
        gallery_dl_path: report.gallery_dl.path,
        deno_path: report.deno.managed.then_some(report.deno.path).flatten(),
        ffmpeg_dir: None,
    };
    // A public-domain sample hosted by the Internet Archive.
    let request = DownloadRequest {
        url: "https://archive.org/download/testmp3testfile/mpthreetest.mp3".to_string(),
        output_dir: out_dir.to_string_lossy().to_string(),
        mode: DownloadMode::Audio,
        video_quality: VideoQuality::Best,
        audio_format: AudioFormat::Best,
        audio_bitrate: "best".to_string(),
    };

    let path = runtime
        .block_on(ytdlp_download(
            &noop_job_sink,
            "audio-job",
            &request,
            &engine_tools,
            2,
            None,
            &cancel,
        ))
        .expect("audio download succeeds");

    let metadata = std::fs::metadata(&path).expect("file exists");
    assert!(metadata.len() > 10_000, "audio file has content");
    let _ = std::fs::remove_file(path);
}
