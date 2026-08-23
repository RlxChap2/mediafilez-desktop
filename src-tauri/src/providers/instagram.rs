use url::Url;

const INSTAGRAM_HOSTS: [&str; 3] = ["instagram.com", "www.instagram.com", "m.instagram.com"];
const SUPPORTED_PATHS: [&str; 4] = ["reel", "reels", "p", "tv"];

pub fn proxy_url(input: &str) -> Option<String> {
    let mut url = Url::parse(input).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    if !INSTAGRAM_HOSTS.contains(&host.as_str()) {
        return None;
    }
    let first_segment = url.path_segments()?.find(|segment| !segment.is_empty())?;
    if !SUPPORTED_PATHS.contains(&first_segment) {
        return None;
    }
    url.set_host(Some("www.kkkinstagram.com")).ok()?;
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_only_supported_instagram_posts() {
        assert_eq!(
            proxy_url("https://www.instagram.com/reels/DcV3RyRz0sq/?utm_source=x").as_deref(),
            Some("https://www.kkkinstagram.com/reels/DcV3RyRz0sq/")
        );
        assert!(proxy_url("https://www.instagram.com/accounts/login/").is_none());
        assert!(proxy_url("https://example.com/reels/DcV3RyRz0sq/").is_none());
    }
}
