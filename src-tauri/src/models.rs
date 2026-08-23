use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DownloadMode {
    Video,
    Image,
    Audio,
    MutedVideo,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoQuality {
    #[default]
    #[serde(rename = "best")]
    Best,
    #[serde(rename = "2160p")]
    P2160,
    #[serde(rename = "1440p")]
    P1440,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "480p")]
    P480,
    #[serde(rename = "360p")]
    P360,
}

impl VideoQuality {
    pub fn max_height(self) -> Option<u16> {
        match self {
            Self::Best => None,
            Self::P2160 => Some(2160),
            Self::P1440 => Some(1440),
            Self::P1080 => Some(1080),
            Self::P720 => Some(720),
            Self::P480 => Some(480),
            Self::P360 => Some(360),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioFormat {
    Best,
    Mp3,
    M4a,
    Opus,
    Wav,
}

impl AudioFormat {
    pub fn as_ytdlp_value(self) -> Option<&'static str> {
        match self {
            Self::Best => None,
            Self::Mp3 => Some("mp3"),
            Self::M4a => Some("m4a"),
            Self::Opus => Some("opus"),
            Self::Wav => Some("wav"),
        }
    }

    pub fn as_cobalt_value(self) -> Option<&'static str> {
        match self {
            Self::Best => None,
            Self::Mp3 => Some("mp3"),
            Self::M4a => Some("m4a"),
            Self::Opus => Some("opus"),
            Self::Wav => Some("wav"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    #[serde(rename = "direct")]
    Direct,
    #[serde(rename = "api")]
    ConfiguredApi,
    #[serde(rename = "public-api")]
    PublicApi,
    #[serde(rename = "yt-dlp")]
    YtDlp,
    #[serde(rename = "gallery-dl")]
    GalleryDl,
    #[serde(rename = "instagram-proxy")]
    InstagramProxy,
    #[serde(rename = "html")]
    HtmlProbe,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub url: String,
    pub output_dir: String,
    pub mode: DownloadMode,
    #[serde(default)]
    pub video_quality: VideoQuality,
    pub audio_format: AudioFormat,
    pub audio_bitrate: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderAttempt {
    pub provider: ProviderKind,
    pub status: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedDownload {
    pub url: String,
    pub output_dir: String,
    pub provider_order: Vec<ProviderKind>,
    pub ytdlp_args: Vec<String>,
    pub requires_ffmpeg: bool,
    pub attempts: Vec<ProviderAttempt>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreflightCheck {
    pub id: String,
    pub label: String,
    pub status: String,
    pub detail: String,
}
