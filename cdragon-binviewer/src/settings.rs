use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// JS object variable with binviewer configuration
    /// The variable MUST exist, but fields are optional
    ///
    /// - `staticBaseUrl`: base URL for hashes and entrydb (default: `"."`)
    /// - `binsBaseUrl`: base URL for bin files (default: `"game"`)
    /// - `assetsBaseUrl`: base URL for asset files (default: `"game"`)
    /// - `maxResults`: maximum search results (default: `1000`)
    static BINVIEWER: JsValue;
}

/// Return URL for a static binviewer resource
pub fn binviewer_static_url(path: &str) -> String {
    let base = get_setting_str("staticBaseUrl").unwrap_or_else(|| ".".into());
    format!("{}/{}", base, path)
}

/// Return URL for a bin file  
pub fn bin_file_url(path: &str) -> String {
    let base = get_setting_str("binsBaseUrl").unwrap_or_else(|| "game".into());
    format!("{}/{}", base, path)
}

/// Return base URL for game asset files
pub fn assets_base_url() -> String {
    get_setting_str("assetsBaseUrl").unwrap_or_else(|| "game".into())
}

/// Get maximum number of search results
pub fn max_search_results() -> usize {
    get_setting_f64("maxResults").map(|v| v as usize).unwrap_or(1000)
}


/// Read a binviewer setting variable
fn get_setting_str(name: &str) -> Option<String> {
    let value = js_sys::Reflect::get(&BINVIEWER, &name.into()).ok()?;
    value.as_string()
}

/// Read a binviewer setting variable
fn get_setting_f64(name: &str) -> Option<f64> {
    let value = js_sys::Reflect::get(&BINVIEWER, &name.into()).ok()?;
    value.as_f64()
}

