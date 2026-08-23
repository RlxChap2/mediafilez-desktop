use crate::models::DownloadMode;
use crate::settings::CookieSource;

pub fn build_gallery_dl_args(
    url: &str,
    output_dir: &str,
    mode: DownloadMode,
    cookie_source: Option<&CookieSource>,
) -> Vec<String> {
    let mut args = vec![
        "--config-ignore".to_string(),
        "--no-colors".to_string(),
        "--windows-filenames".to_string(),
        "--retries".to_string(),
        "8".to_string(),
        "--sleep-retries".to_string(),
        "exp=1-12".to_string(),
        "--sleep-429".to_string(),
        "exp=10-90".to_string(),
        "--http-timeout".to_string(),
        "30".to_string(),
        "--directory".to_string(),
        output_dir.to_string(),
        "--Print".to_string(),
        "after:{_path}".to_string(),
    ];

    let filter = match mode {
        DownloadMode::Image => Some("extension in ('jpg','jpeg','png','webp','gif','avif','bmp')"),
        DownloadMode::Video | DownloadMode::MutedVideo => {
            Some("extension in ('mp4','webm','mov','m4v','mkv')")
        }
        DownloadMode::Audio => None,
    };
    if let Some(filter) = filter {
        args.push("--filter".to_string());
        args.push(filter.to_string());
    }

    if let Some(source) = cookie_source {
        match source {
            CookieSource::Browser(browser) => {
                args.push("--cookies-from-browser".to_string());
                args.push(browser.clone());
            }
            CookieSource::File(path) => {
                args.push("--cookies".to_string());
                args.push(path.clone());
            }
        }
    }

    args.push(url.to_string());
    args
}

pub fn supports_mode(mode: &DownloadMode) -> bool {
    matches!(mode, DownloadMode::Image | DownloadMode::Video)
}
