pub mod cobalt;
pub mod direct;
pub mod gallery_dl;
pub mod instagram;
pub mod yt_dlp;

use crate::models::ProviderKind;

pub fn plan_provider_order(api_enabled: bool) -> Vec<ProviderKind> {
    let mut providers = vec![ProviderKind::Direct, ProviderKind::YtDlp];

    if api_enabled {
        providers.push(ProviderKind::ConfiguredApi);
    }

    providers.push(ProviderKind::GalleryDl);
    providers.push(ProviderKind::HtmlProbe);
    providers
}
