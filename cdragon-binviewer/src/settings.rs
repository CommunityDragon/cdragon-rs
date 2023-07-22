
/// Base URL for static resources
pub const STATIC_BASE_URL: &'static str = &".";

/// Base URL for game assets
pub const ASSETS_BASE_URL: &'static str = &"https://raw.communitydragon.org/pbe/game";

/// Maximum number of search results to display
pub const MAX_SEARCH_RESULTS: usize = 1000;


macro_rules! static_uri {
    ($e:expr) => (format!("{}/{}", $crate::settings::STATIC_BASE_URL, $e));
    ($fmt:literal $(, $e:expr)*) => (format!(concat!("{}/", $fmt), $crate::settings::STATIC_BASE_URL $(, $e)*));
}

