use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use url::{Host, Url};

const BLOCKED_DOWNLOAD_SUFFIXES: [&str; 39] = [
    ".appref-ms",
    ".application",
    ".bat",
    ".chm",
    ".cmd",
    ".com",
    ".cpl",
    ".desktop",
    ".dll",
    ".exe",
    ".gadget",
    ".hta",
    ".inf",
    ".ins",
    ".isp",
    ".jar",
    ".js",
    ".jse",
    ".lnk",
    ".mde",
    ".msc",
    ".msi",
    ".msp",
    ".mst",
    ".pif",
    ".ps1",
    ".reg",
    ".scf",
    ".scr",
    ".sct",
    ".shb",
    ".shs",
    ".url",
    ".vb",
    ".vbe",
    ".vbs",
    ".webloc",
    ".wsf",
    ".wsh",
];

const OPENABLE_MEDIA_SUFFIXES: [&str; 22] = [
    ".aac", ".avif", ".avi", ".bmp", ".flac", ".gif", ".jpeg", ".jpg", ".m4a", ".m4v", ".mkv",
    ".mov", ".mp3", ".mp4", ".oga", ".ogg", ".opus", ".png", ".wav", ".webm", ".webp", ".wmv",
];

pub fn validate_remote_url(input: &str) -> Result<Url, String> {
    let parsed =
        Url::parse(input.trim()).map_err(|_| "Enter a valid http or https URL.".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Only http and https links are supported.".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Links containing embedded usernames or passwords are blocked.".to_string());
    }
    let host = parsed
        .host()
        .ok_or_else(|| "The link must include a host name.".to_string())?;
    if is_private_host(host) {
        return Err("Local-network and private-address links are blocked for safety.".to_string());
    }
    Ok(parsed)
}

pub async fn validate_remote_target(input: &str) -> Result<Url, String> {
    resolve_remote_target(input).await.map(|(url, _, _)| url)
}

async fn resolve_remote_target(input: &str) -> Result<(Url, String, Vec<SocketAddr>), String> {
    let parsed = validate_remote_url(input)?;
    let domain = parsed
        .host_str()
        .ok_or_else(|| "The link must include a host name.".to_string())?
        .to_string();
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| "The link uses an unsupported port.".to_string())?;
    let addresses = if let Ok(address) = domain.parse::<IpAddr>() {
        vec![SocketAddr::new(address, port)]
    } else {
        tokio::net::lookup_host((domain.as_str(), port))
            .await
            .map_err(|_| "The link's host name could not be resolved.".to_string())?
            .collect()
    };
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err("The link resolves to a local or private network address.".to_string());
    }
    Ok((parsed, domain, addresses))
}

async fn resolve_api_target(
    input: &str,
    original: &Url,
) -> Result<(Url, String, Vec<SocketAddr>), String> {
    let parsed = validate_api_endpoint(input)?;
    let original_loopback = original.host().is_some_and(is_loopback_host);
    let explicit_loopback = parsed.host().is_some_and(is_loopback_host);
    let domain = parsed
        .host_str()
        .ok_or_else(|| "The API endpoint must include a host name.".to_string())?
        .to_string();
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| "The API endpoint uses an unsupported port.".to_string())?;
    let addresses = if let Ok(address) = domain.parse::<IpAddr>() {
        vec![SocketAddr::new(address, port)]
    } else {
        tokio::net::lookup_host((domain.as_str(), port))
            .await
            .map_err(|_| "The API endpoint could not be resolved.".to_string())?
            .collect()
    };
    let allowed = |address: &SocketAddr| {
        is_public_ip(address.ip())
            || (original_loopback && explicit_loopback && address.ip().is_loopback())
    };
    if addresses.is_empty() || addresses.iter().any(|address| !allowed(address)) {
        return Err("The API endpoint resolves to a local or private network address.".to_string());
    }
    Ok((parsed, domain, addresses))
}

pub async fn secure_get(
    input: &str,
    user_agent: &str,
    timeout: Duration,
) -> Result<reqwest::Response, String> {
    let mut current = input.to_string();
    for redirect_count in 0..=8 {
        let (url, host, addresses) = resolve_remote_target(&current).await?;
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(&host, &addresses)
            .timeout(timeout)
            .build()
            .map_err(|error| format!("HTTP client error: {error}"))?;
        let response = client
            .get(url.clone())
            .send()
            .await
            .map_err(|error| format!("Request failed: {error}"))?;

        if !response.status().is_redirection() {
            return Ok(response);
        }
        if redirect_count == 8 {
            return Err("The link redirected too many times.".to_string());
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| "The server returned an invalid redirect.".to_string())?;
        current = url
            .join(location)
            .map_err(|_| "The server returned an invalid redirect URL.".to_string())?
            .to_string();
    }
    Err("The link redirected too many times.".to_string())
}

pub fn validate_api_endpoint(input: &str) -> Result<Url, String> {
    let parsed = Url::parse(input.trim()).map_err(|_| "Enter a valid API endpoint.".to_string())?;
    let loopback = parsed.host().is_some_and(is_loopback_host);
    if parsed.scheme() != "https" && !(parsed.scheme() == "http" && loopback) {
        return Err("API endpoints must use https; http is allowed only on localhost.".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("API endpoints cannot contain embedded credentials.".to_string());
    }
    Ok(parsed)
}

pub async fn secure_post_json<T: serde::Serialize + ?Sized>(
    input: &str,
    user_agent: &str,
    timeout: Duration,
    authorization: Option<&str>,
    body: &T,
) -> Result<reqwest::Response, String> {
    let original = validate_api_endpoint(input)?;
    let original_origin = original.origin();
    let mut current = original.to_string();

    for redirect_count in 0..=8 {
        let (url, host, addresses) = resolve_api_target(&current, &original).await?;
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(&host, &addresses)
            .timeout(timeout)
            .build()
            .map_err(|error| format!("HTTP client error: {error}"))?;
        let mut request = client
            .post(url.clone())
            .header(reqwest::header::ACCEPT, "application/json")
            .json(body);
        if url.origin() == original_origin {
            if let Some(value) = authorization {
                request = request.header(reqwest::header::AUTHORIZATION, value);
            }
        }
        let response = request
            .send()
            .await
            .map_err(|error| format!("API request failed: {error}"))?;

        if !response.status().is_redirection() {
            return Ok(response);
        }
        if redirect_count == 8 {
            return Err("The API endpoint redirected too many times.".to_string());
        }
        if !matches!(
            response.status(),
            reqwest::StatusCode::TEMPORARY_REDIRECT | reqwest::StatusCode::PERMANENT_REDIRECT
        ) {
            return Err("The API endpoint returned an unsafe redirect status.".to_string());
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| "The API endpoint returned an invalid redirect.".to_string())?;
        current = url
            .join(location)
            .map_err(|_| "The API endpoint returned an invalid redirect URL.".to_string())?
            .to_string();
    }
    Err("The API endpoint redirected too many times.".to_string())
}

pub fn is_safe_download_filename(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    !BLOCKED_DOWNLOAD_SUFFIXES
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

pub fn is_openable_media_filename(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    is_safe_download_filename(&lower)
        && OPENABLE_MEDIA_SUFFIXES
            .iter()
            .any(|suffix| lower.ends_with(suffix))
}

fn is_private_host(host: Host<&str>) -> bool {
    match host {
        Host::Domain(domain) => {
            let domain = domain.trim_end_matches('.').to_ascii_lowercase();
            domain == "localhost"
                || domain.ends_with(".localhost")
                || domain.ends_with(".local")
                || domain.ends_with(".lan")
                || domain.ends_with(".internal")
                || domain.ends_with(".home.arpa")
        }
        Host::Ipv4(address) => !is_public_ipv4(address),
        Host::Ipv6(address) => !is_public_ipv6(address),
    }
}

fn is_loopback_host(host: Host<&str>) -> bool {
    match host {
        Host::Domain(domain) => domain.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(address) => address.is_loopback(),
        Host::Ipv6(address) => address.is_loopback(),
    }
}

fn is_public_ipv4(address: Ipv4Addr) -> bool {
    let octets = address.octets();
    !(address.is_private()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_unspecified()
        || address.is_broadcast()
        || address.is_documentation()
        || address.is_multicast()
        || octets[0] == 0
        || octets[0] >= 240
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 198 && (18..=19).contains(&octets[1])))
}

fn is_public_ipv6(address: Ipv6Addr) -> bool {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    let segments = address.segments();
    let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
    let site_local = segments[0] & 0xffc0 == 0xfec0;
    let ipv4_compatible = segments[..6].iter().all(|segment| *segment == 0);
    !(address.is_loopback()
        || address.is_unspecified()
        || address.is_multicast()
        || address.is_unique_local()
        || address.is_unicast_link_local()
        || documentation
        || site_local
        || ipv4_compatible)
}

fn is_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_ipv4(address),
        IpAddr::V6(address) => is_public_ipv6(address),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_local_and_non_http_urls() {
        assert!(validate_remote_url("file:///C:/secret.txt").is_err());
        assert!(validate_remote_url("http://127.0.0.1/video.mp4").is_err());
        assert!(validate_remote_url("http://192.168.1.2/video.mp4").is_err());
        assert!(validate_remote_url("https://example.com/video.mp4").is_ok());
        assert!(validate_remote_url("http://[::ffff:127.0.0.1]/video.mp4").is_err());
        assert!(validate_remote_url("http://[2001:db8::1]/video.mp4").is_err());
        assert!(validate_remote_url("http://[fec0::1]/video.mp4").is_err());
    }

    #[test]
    fn blocks_executable_download_names() {
        assert!(!is_safe_download_filename("movie.mp4.exe"));
        assert!(!is_safe_download_filename("payload.cpl"));
        assert!(!is_safe_download_filename("payload.appref-ms"));
        assert!(!is_safe_download_filename("payload.chm"));
        assert!(!is_safe_download_filename("payload.scf"));
        assert!(!is_safe_download_filename("payload.wsf"));
        assert!(!is_safe_download_filename("shortcut.url"));
        assert!(is_safe_download_filename("movie.mp4"));
    }

    #[test]
    fn opens_only_known_media_files() {
        assert!(is_openable_media_filename("movie.mp4"));
        assert!(is_openable_media_filename("track.flac"));
        assert!(!is_openable_media_filename("payload.cpl"));
        assert!(!is_openable_media_filename("unknown.bin"));
    }

    #[tokio::test]
    async fn api_post_blocks_private_targets() {
        let payload = serde_json::json!({ "url": "https://example.com/video" });
        let error = secure_post_json(
            "https://192.168.1.1/api",
            "MediaFilez security test",
            Duration::from_secs(1),
            None,
            &payload,
        )
        .await
        .expect_err("API requests must reject private-network targets");
        assert!(error.contains("local or private"));
    }

    #[tokio::test]
    async fn public_api_cannot_redirect_to_loopback() {
        let public = validate_api_endpoint("https://example.com").expect("valid public endpoint");
        let error = resolve_api_target("http://127.0.0.1/api", &public)
            .await
            .expect_err("public endpoints must not cross into loopback");
        assert!(error.contains("local or private"));

        let local = validate_api_endpoint("http://127.0.0.1:9000").expect("valid local endpoint");
        resolve_api_target("http://127.0.0.1:9001/api", &local)
            .await
            .expect("explicit local endpoints may redirect within loopback");
    }
}
