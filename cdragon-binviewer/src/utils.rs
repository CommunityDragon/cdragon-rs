use web_sys::{MouseEvent, UrlSearchParams};
use wasm_bindgen::UnwrapThrowExt;
use yew::callback::Callback;
use cdragon_prop::data::BinEntryPath;
use cdragon_hashes::HashDef;

/// Pass normal clicks, drop other ones
///
/// This method is intended to wrap a `MouseEvent` event to handle normal clicks and let the
/// browser handle otherwise, typically to follow a link.
pub fn handle_normal_click(f: Callback<MouseEvent, ()>) -> Callback<MouseEvent, ()> {
    Callback::from(move |event: MouseEvent| {
        let normal =
            event.button() == 0 &&
            !event.alt_key() &&
            !event.ctrl_key() &&
            !event.meta_key() &&
            !event.shift_key();
        if normal {
            event.prevent_default();
            f.emit(event);
        }
    })
}

/// Build an app URL
pub fn build_app_url(query: &str, hpath: Option<BinEntryPath>) -> String {
    let query = js_sys::encode_uri_component(query);
    match hpath {
        Some(hpath) => format!("?s={}&e={:x}#{}", query, hpath, entry_element_id(hpath)),
        None => format!("?s={}", query),
    }
}

/// Parse an app URL, using current location
pub fn parse_app_url() -> (String, Option<BinEntryPath>) {
    let window = web_sys::window().unwrap_throw();
    let search = window.location().search().unwrap_throw();
    let params = UrlSearchParams::new_with_str(&search).unwrap_throw();
    let pattern = params.get("s").unwrap_or_default();
    let focus = params.get("e").and_then(|s| u32::from_str_radix(&s, 16).ok().map(BinEntryPath::new));
    (pattern, focus)
}

/// Return HTML ID of an entry element
pub fn entry_element_id(hpath: BinEntryPath) -> String {
    format!("entry-{:x}", hpath)
}

