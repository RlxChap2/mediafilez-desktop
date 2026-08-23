use scraper::{Html, Selector};
use url::Url;

const MEDIA_EXTENSIONS: [&str; 18] = [
    "mp4", "webm", "m3u8", "mpd", "mov", "m4v", "mkv", "m4a", "mp3", "opus", "wav", "jpg", "jpeg",
    "png", "webp", "gif", "avif", "bmp",
];

pub fn is_direct_media_url(input: &str) -> bool {
    Url::parse(input)
        .ok()
        .and_then(|url| {
            url.path_segments()
                .and_then(Iterator::last)
                .map(str::to_ascii_lowercase)
        })
        .and_then(|file_name| {
            file_name
                .rsplit_once('.')
                .map(|(_, extension)| extension.to_string())
        })
        .is_some_and(|extension| MEDIA_EXTENSIONS.contains(&extension.as_str()))
}

pub fn is_stream_manifest_url(input: &str) -> bool {
    Url::parse(input)
        .map(|url| {
            let path = url.path().to_ascii_lowercase();
            path.ends_with(".m3u8") || path.ends_with(".mpd")
        })
        .unwrap_or(false)
}

pub fn extract_media_links_from_html(base_url: &str, html: &str) -> Vec<String> {
    let document = Html::parse_document(html);
    let base = Url::parse(base_url).ok();
    let selectors = [
        ("meta[property='og:video']", "content"),
        ("meta[property='og:video:url']", "content"),
        ("meta[property='og:image']", "content"),
        ("meta[property='og:image:url']", "content"),
        ("meta[property='og:audio']", "content"),
        ("meta[name='twitter:player:stream']", "content"),
        ("meta[name='twitter:image']", "content"),
        ("video", "src"),
        ("audio", "src"),
        ("source", "src"),
        ("img", "src"),
        ("a", "href"),
    ];
    let mut links = Vec::new();

    for (selector, attribute) in selectors {
        let selector = Selector::parse(selector).expect("valid static selector");
        for element in document.select(&selector) {
            let Some(raw) = element.value().attr(attribute) else {
                continue;
            };
            let resolved = resolve_media_url(base.as_ref(), raw);
            if let Some(resolved) = resolved {
                if is_direct_media_url(&resolved) && !links.contains(&resolved) {
                    links.push(resolved);
                }
            }
        }
    }

    links
}

fn resolve_media_url(base: Option<&Url>, raw: &str) -> Option<String> {
    if let Ok(url) = Url::parse(raw) {
        return Some(url.to_string());
    }

    base.and_then(|base| base.join(raw).ok())
        .map(|url| url.to_string())
}
