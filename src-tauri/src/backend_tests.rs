use std::path::PathBuf;

use crate::models::{
    AudioFormat, DownloadMode, DownloadRequest, PlannedDownload, ProviderKind, VideoQuality,
};
use crate::providers::cobalt::{build_cobalt_request, CobaltResponse, CobaltStatus};
use crate::providers::direct::{extract_media_links_from_html, is_direct_media_url};
use crate::providers::gallery_dl::build_gallery_dl_args;
use crate::providers::instagram::proxy_url as instagram_proxy_url;
use crate::providers::plan_provider_order;
use crate::providers::yt_dlp::{build_ytdlp_args, requires_ffmpeg_for_audio};
use crate::settings::AppSettings;
use crate::storage::{next_available_path, sanitize_filename};

#[test]
fn sanitizes_windows_hostile_filenames() {
    let sanitized = sanitize_filename("bad:/\\*?\"<>| name.mp4");
    assert_eq!(sanitized, "bad name.mp4");
}

#[test]
fn protects_windows_device_names_and_keeps_long_extensions() {
    assert_eq!(sanitize_filename("CON.mp4"), "_CON.mp4");
    assert_eq!(sanitize_filename("lpt9"), "_lpt9");

    let sanitized = sanitize_filename(&format!("{}.mp4", "a".repeat(250)));
    assert_eq!(sanitized.encode_utf16().count(), 180);
    assert!(sanitized.ends_with(".mp4"));

    let sanitized = sanitize_filename(&format!("{}.mp4", "🎬".repeat(150)));
    assert!(sanitized.encode_utf16().count() <= 180);
    assert!(sanitized.ends_with(".mp4"));
}

#[test]
fn adds_suffix_when_output_path_exists() {
    let existing = [PathBuf::from("C:/Downloads/video.mp4")];
    let next = next_available_path(PathBuf::from("C:/Downloads/video.mp4"), |path| {
        existing.iter().any(|candidate| candidate == path)
    });

    assert_eq!(next, PathBuf::from("C:/Downloads/video (1).mp4"));
}

#[test]
fn orders_providers_fastest_safe_path_first() {
    assert_eq!(
        plan_provider_order(true),
        vec![
            ProviderKind::Direct,
            ProviderKind::YtDlp,
            ProviderKind::ConfiguredApi,
            ProviderKind::GalleryDl,
            ProviderKind::HtmlProbe,
        ],
    );
}

#[test]
fn skips_configured_api_when_disabled() {
    assert_eq!(
        plan_provider_order(false),
        vec![
            ProviderKind::Direct,
            ProviderKind::YtDlp,
            ProviderKind::GalleryDl,
            ProviderKind::HtmlProbe
        ],
    );
}

#[test]
fn ranks_community_instances_by_service_coverage() {
    let directory: crate::providers::cobalt::InstanceDirectory = serde_json::from_str(
        r#"{
            "lastUpdatedUTC": "2026-07-15T20:23:37.515Z",
            "data": {
                "youtube": ["https://a.example", "https://b.example"],
                "tiktok": ["https://b.example", "http://insecure.example"],
                "Frontend": ["https://ignored.example"],
                "reddit": ["https://b.example", "https://c.example"]
            }
        }"#,
    )
    .expect("directory parses");

    let ranked = crate::providers::cobalt::rank_instances(&directory);
    assert_eq!(ranked[0], "https://b.example");
    assert!(ranked.contains(&"https://a.example".to_string()));
    assert!(!ranked.iter().any(|url| url.contains("insecure")));
    assert!(!ranked.iter().any(|url| url.contains("ignored")));
}

#[test]
fn builds_cobalt_audio_request_payload() {
    let request = build_cobalt_request(
        "https://example.com/watch/1",
        DownloadMode::Audio,
        AudioFormat::Mp3,
        Some("320"),
    );

    assert_eq!(request.url, "https://example.com/watch/1");
    assert_eq!(request.download_mode.as_deref(), Some("audio"));
    assert_eq!(request.audio_format.as_deref(), Some("mp3"));
    assert_eq!(request.audio_bitrate.as_deref(), Some("320"));
    assert_eq!(request.local_processing, "disabled");
}

#[test]
fn models_cobalt_tunnel_response() {
    let response = CobaltResponse {
        status: CobaltStatus::Tunnel,
        url: Some("https://cdn.example/video.mp4".to_string()),
        filename: Some("video.mp4".to_string()),
        picker: Vec::new(),
        text: None,
        error: None,
    };

    assert!(response.is_downloadable());
}

#[test]
fn detects_direct_media_urls_and_html_links() {
    assert!(is_direct_media_url("https://cdn.example.com/video.mp4"));
    assert!(is_direct_media_url(
        "https://cdn.example.com/playlist.m3u8?token=1"
    ));
    assert!(!is_direct_media_url("https://example.com/watch/abc"));

    let html = r#"
      <html>
        <meta property="og:video" content="https://cdn.example.com/social.mp4">
        <video><source src="/clip.webm"></video>
      </html>
    "#;

    let links = extract_media_links_from_html("https://example.com/post", html);
    assert!(links.contains(&"https://cdn.example.com/social.mp4".to_string()));
    assert!(links.contains(&"https://example.com/clip.webm".to_string()));
}

#[test]
fn wire_format_matches_frontend_types() {
    // The React client sends camelCase keys and short provider names; keep
    // this contract locked so `invoke` payloads keep deserializing.
    let request: DownloadRequest = serde_json::from_str(
        r#"{
            "url": "https://example.com/watch/1",
            "outputDir": "C:/Downloads",
            "mode": "muted-video",
            "videoQuality": "1080p",
            "audioFormat": "m4a",
            "audioBitrate": "128"
        }"#,
    )
    .expect("camelCase request deserializes");
    assert_eq!(request.output_dir, "C:/Downloads");
    assert_eq!(request.mode, DownloadMode::MutedVideo);
    assert_eq!(request.video_quality, VideoQuality::P1080);

    let provider_json = serde_json::to_string(&vec![
        ProviderKind::Direct,
        ProviderKind::ConfiguredApi,
        ProviderKind::PublicApi,
        ProviderKind::YtDlp,
        ProviderKind::GalleryDl,
        ProviderKind::InstagramProxy,
        ProviderKind::HtmlProbe,
    ])
    .expect("providers serialize");
    assert_eq!(
        provider_json,
        r#"["direct","api","public-api","yt-dlp","gallery-dl","instagram-proxy","html"]"#
    );

    let planned = PlannedDownload {
        url: "https://example.com/watch/1".to_string(),
        output_dir: "C:/Downloads".to_string(),
        provider_order: vec![ProviderKind::YtDlp],
        ytdlp_args: Vec::new(),
        requires_ffmpeg: true,
        attempts: Vec::new(),
    };
    let planned_json = serde_json::to_string(&planned).expect("plan serializes");
    assert!(planned_json.contains("\"outputDir\""));
    assert!(planned_json.contains("\"providerOrder\""));
    assert!(planned_json.contains("\"ytdlpArgs\""));
    assert!(planned_json.contains("\"requiresFfmpeg\""));

    let settings: AppSettings = serde_json::from_str(
        r#"{
            "outputFolder": "C:/Downloads",
            "concurrency": 2,
            "cookiesFromBrowser": false,
            "ffmpegPath": "",
            "apiProvider": {
                "enabled": true,
                "baseUrl": "https://api.example.com",
                "authType": "api-key",
                "token": "secret",
                "timeoutSeconds": 20
            }
        }"#,
    )
    .expect("frontend settings shape deserializes");
    assert_eq!(settings.output_folder, "C:/Downloads");
    assert!(settings.api_provider.enabled);
    assert_eq!(settings.api_provider.base_url, "https://api.example.com");
}

#[test]
fn builds_ytdlp_audio_arguments_without_conversion_when_native() {
    let args = build_ytdlp_args(
        "https://example.com/watch/1",
        DownloadMode::Audio,
        VideoQuality::Best,
        AudioFormat::Best,
        "best",
        "C:/Downloads/%(title)s.%(ext)s",
        2,
    );

    assert!(args.contains(&"--extract-audio".to_string()));
    assert!(!args.contains(&"--audio-format".to_string()));
}

#[test]
fn builds_ytdlp_audio_arguments_with_conversion_when_requested() {
    let args = build_ytdlp_args(
        "https://example.com/watch/1",
        DownloadMode::Audio,
        VideoQuality::Best,
        AudioFormat::Mp3,
        "320",
        "C:/Downloads/%(title)s.%(ext)s",
        4,
    );

    assert!(args
        .windows(2)
        .any(|pair| pair == ["--audio-format", "mp3"]));
    assert!(args
        .windows(2)
        .any(|pair| pair == ["--audio-quality", "320K"]));
    assert!(requires_ffmpeg_for_audio(AudioFormat::Mp3));
    assert!(!requires_ffmpeg_for_audio(AudioFormat::Best));
    assert!(args
        .windows(2)
        .any(|pair| pair == ["--concurrent-fragments", "4"]));
    assert!(args
        .windows(2)
        .any(|pair| pair == ["--extractor-retries", "3"]));
    assert!(args
        .windows(2)
        .any(|pair| pair == ["--file-access-retries", "3"]));
}

#[test]
fn caps_video_quality_in_ytdlp_format_selection() {
    let args = build_ytdlp_args(
        "https://example.com/watch/1",
        DownloadMode::Video,
        VideoQuality::P1080,
        AudioFormat::Best,
        "best",
        "C:/Downloads/%(title)s.%(ext)s",
        8,
    );
    assert!(args.iter().any(|arg| arg.contains("height<=1080")));
}

#[test]
fn builds_gallery_arguments_for_images_and_cookie_files() {
    let cookies = crate::settings::CookieSource::File("C:/cookies.txt".to_string());
    let args = build_gallery_dl_args(
        "https://www.pinterest.com/pin/123/",
        "C:/Downloads/staging",
        DownloadMode::Image,
        Some(&cookies),
    );

    assert!(args
        .windows(2)
        .any(|pair| pair == ["--directory", "C:/Downloads/staging"]));
    assert!(args
        .windows(2)
        .any(|pair| pair == ["--cookies", "C:/cookies.txt"]));
    assert!(args.iter().any(|arg| arg.contains("extension in")));
}

#[test]
fn rewrites_only_public_instagram_post_routes() {
    assert_eq!(
        instagram_proxy_url("https://www.instagram.com/reels/DcV3RyRz0sq/?utm_source=share")
            .as_deref(),
        Some("https://www.kkkinstagram.com/reels/DcV3RyRz0sq/")
    );
    assert!(instagram_proxy_url("https://www.instagram.com/accounts/login/").is_none());
    assert!(instagram_proxy_url("https://example.com/reels/DcV3RyRz0sq/").is_none());
}
