use serde::{Deserialize, Serialize};
use std::io::BufRead;

use crate::models::{AudioFormat, DownloadMode, ProviderKind};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ApiProviderSettings {
    pub enabled: bool,
    pub base_url: String,
    pub auth_type: String,
    /// Kept in memory for the current session. Never written to settings.json.
    #[serde(default)]
    pub token: String,
    pub timeout_seconds: u64,
}

impl Default for ApiProviderSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::new(),
            auth_type: "none".to_string(),
            token: String::new(),
            timeout_seconds: 20,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub output_folder: String,
    pub mode: DownloadMode,
    pub audio_format: AudioFormat,
    pub audio_bitrate: String,
    pub provider_order: Vec<ProviderKind>,
    pub concurrency: u8,
    pub cookies_from_browser: bool,
    pub cookie_browser: String,
    pub cookie_browser_profile: String,
    pub cookie_file: String,
    pub ffmpeg_path: String,
    pub api_provider: ApiProviderSettings,
    /// Third-party community Cobalt servers. Disabled unless the user opts in.
    pub community_fallback: bool,
    /// Experimental Instagram embed resolver. Never receives cookies or tokens.
    pub instagram_proxy_fallback: bool,
}

impl AppSettings {
    /// Loads settings from disk, falling back to defaults (with the user's
    /// Downloads folder pre-selected) so the app works with zero setup.
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create settings dir: {error}"))?;
        }
        let mut persisted = self.clone();
        persisted.api_provider.token.clear();
        let raw = serde_json::to_string_pretty(&persisted)
            .map_err(|error| format!("Could not serialize settings: {error}"))?;
        std::fs::write(path, raw).map_err(|error| format!("Could not save settings: {error}"))
    }
}

pub fn default_output_folder() -> String {
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .map(|dir| dir.to_string_lossy().to_string())
        .unwrap_or_default()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            output_folder: default_output_folder(),
            mode: DownloadMode::Video,
            audio_format: AudioFormat::Best,
            audio_bitrate: "best".to_string(),
            provider_order: vec![
                ProviderKind::Direct,
                ProviderKind::YtDlp,
                ProviderKind::ConfiguredApi,
                ProviderKind::GalleryDl,
                ProviderKind::HtmlProbe,
            ],
            concurrency: 2,
            cookies_from_browser: false,
            cookie_browser: "firefox".to_string(),
            cookie_browser_profile: String::new(),
            cookie_file: String::new(),
            ffmpeg_path: String::new(),
            api_provider: ApiProviderSettings::default(),
            community_fallback: false,
            instagram_proxy_fallback: false,
        }
    }
}

impl AppSettings {
    pub fn normalize(&mut self) {
        self.concurrency = self.concurrency.clamp(1, 8);
        if !matches!(
            self.cookie_browser.as_str(),
            "edge" | "chrome" | "firefox" | "brave" | "vivaldi" | "opera"
        ) {
            self.cookie_browser = "firefox".to_string();
        }
        self.cookie_browser_profile = self.cookie_browser_profile.trim().to_string();
        self.cookie_file = self.cookie_file.trim().to_string();
        self.api_provider.timeout_seconds = self.api_provider.timeout_seconds.clamp(5, 120);
        if !matches!(
            self.api_provider.auth_type.as_str(),
            "none" | "api-key" | "bearer"
        ) {
            self.api_provider.auth_type = "none".to_string();
        }
        self.api_provider.base_url = cobalt_endpoints(&self.api_provider.base_url).join("\n");
        if self.api_provider.base_url.is_empty() {
            self.api_provider.enabled = false;
        }
    }

    pub fn cookie_source(&self) -> Option<CookieSource> {
        if !self.cookie_file.is_empty() {
            return Some(CookieSource::File(self.cookie_file.clone()));
        }
        if !self.cookies_from_browser {
            return None;
        }

        let mut browser = self.cookie_browser.clone();
        if !self.cookie_browser_profile.is_empty() {
            browser.push(':');
            browser.push_str(&self.cookie_browser_profile);
        }
        Some(CookieSource::Browser(browser))
    }
}

pub fn cobalt_endpoints(value: &str) -> Vec<String> {
    let mut endpoints = Vec::new();
    for endpoint in value.lines().flat_map(|line| line.split(',')) {
        let endpoint = endpoint.trim().trim_end_matches('/');
        if !endpoint.is_empty() && !endpoints.iter().any(|item| item == endpoint) {
            endpoints.push(endpoint.to_string());
        }
    }
    endpoints
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CookieSource {
    Browser(String),
    File(String),
}

pub fn validate_cookie_file(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Ok(());
    }

    let file = std::fs::File::open(path)
        .map_err(|error| format!("Could not open the selected cookies file: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Could not inspect the selected cookies file: {error}"))?;
    if !metadata.is_file() {
        return Err("The selected cookies path is not a file.".to_string());
    }
    if metadata.len() > 10 * 1024 * 1024 {
        return Err("The selected cookies file is unexpectedly large.".to_string());
    }

    let mut first_line = String::new();
    std::io::BufReader::new(file)
        .read_line(&mut first_line)
        .map_err(|error| format!("Could not read the selected cookies file: {error}"))?;
    let header = first_line.trim_start_matches('\u{feff}').trim();
    if header != "# Netscape HTTP Cookie File" && header != "# HTTP Cookie File" {
        return Err(
            "Choose a Netscape cookies.txt file. Its first line must identify it as an HTTP cookie file."
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_untrusted_settings() {
        let mut settings = AppSettings {
            concurrency: 99,
            cookie_browser: "unknown".to_string(),
            api_provider: ApiProviderSettings {
                auth_type: "custom".to_string(),
                timeout_seconds: 999,
                ..ApiProviderSettings::default()
            },
            ..AppSettings::default()
        };
        settings.normalize();
        assert_eq!(settings.concurrency, 8);
        assert_eq!(settings.cookie_browser, "firefox");
        assert_eq!(settings.api_provider.auth_type, "none");
        assert_eq!(settings.api_provider.timeout_seconds, 120);
    }

    #[test]
    fn normalizes_a_cobalt_endpoint_pool() {
        let mut settings = AppSettings {
            api_provider: ApiProviderSettings {
                enabled: true,
                base_url: " https://one.example/\nhttps://two.example, https://one.example "
                    .to_string(),
                ..ApiProviderSettings::default()
            },
            ..AppSettings::default()
        };
        settings.normalize();
        assert_eq!(
            settings.api_provider.base_url,
            "https://one.example\nhttps://two.example"
        );
    }

    #[test]
    fn resolves_cookie_file_before_browser_session() {
        let settings = AppSettings {
            cookies_from_browser: true,
            cookie_browser: "firefox".to_string(),
            cookie_browser_profile: "work".to_string(),
            cookie_file: "C:/cookies.txt".to_string(),
            ..AppSettings::default()
        };
        assert_eq!(
            settings.cookie_source(),
            Some(CookieSource::File("C:/cookies.txt".to_string()))
        );

        let browser_settings = AppSettings {
            cookie_file: String::new(),
            ..settings
        };
        assert_eq!(
            browser_settings.cookie_source(),
            Some(CookieSource::Browser("firefox:work".to_string()))
        );
    }

    #[test]
    fn validates_netscape_cookie_files() {
        let path = std::env::temp_dir().join(format!(
            "mediafilez-desktop-cookies-{}.txt",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(
            &path,
            "# Netscape HTTP Cookie File\n.example.com\tTRUE\t/\tFALSE\t0\ta\tb\n",
        )
        .expect("write test cookie file");
        assert!(validate_cookie_file(&path.to_string_lossy()).is_ok());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn never_persists_api_tokens() {
        let path = std::env::temp_dir().join(format!(
            "mediafilez-desktop-settings-{}.json",
            uuid::Uuid::new_v4()
        ));
        let mut settings = AppSettings::default();
        settings.api_provider.token = "session-secret".to_string();
        settings.save(&path).expect("settings save");
        let raw = std::fs::read_to_string(&path).expect("settings file");
        assert!(!raw.contains("session-secret"));
        let _ = std::fs::remove_file(path);
    }
}
