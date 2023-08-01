#![recursion_limit = "256"]
#[macro_use]
pub mod settings;
mod entrydb;
mod hooks;
mod services;
mod components;
mod binview;

use std::rc::Rc;
use gloo_console::{debug, info, error};
use yew::prelude::*;
use wasm_bindgen::{JsValue, UnwrapThrowExt};
use cdragon_prop::data::*;

use services::Services;
use components::*;

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;


pub enum AppAction {
    /// Switch the state to a loaded one
    ServicesLoaded(Services),
    /// Search for entries, update results
    SearchEntries(String),
    /// Load given entry (if needed) and scroll to it
    FollowLink(BinEntryPath),
    /// Load entries of the given type (update results)
    FilterEntryType(BinClassName),
}


/// Elements shared to all components
///
/// It contains the state of the root component, because of the interdependencies.
/// A clean way would be to separate services from the messaging (to avoid a circular
/// dependencies), or using a separate `Rc<Services>` stored both shared and root states. 
#[derive(Clone, Default)]
pub struct AppState {
    /// Services, loaded at start
    services: Rc<Services>,
    /// Current search pattern
    search_pattern: String,
    /// Result entries, displayed
    result_entries: Vec<BinEntryPath>,
}

impl AppState {
    fn with_pattern(pattern: String) -> Self {
        Self {
            search_pattern: pattern,
            .. Default::default()
        }
    }

    fn search_entries(self: Rc<Self>, pattern: String, hpath: Option<BinEntryPath>) -> Rc<Self> {
        let words: Vec<&str> = pattern.split_whitespace().collect();
        let result_entries = match self.services.entrydb.search_words(&words, &self.services.hmappers) {
            Ok(it) => it.take(settings::MAX_SEARCH_RESULTS).collect(),
            Err(e) => {
                error!(format!("search failed: {}", e));
                vec![]
            }
        };
        debug!(format!("new search results: {} entries", result_entries.len()));
        push_history_state(&pattern, hpath).unwrap_throw();
        Self {
            services: self.services.clone(),
            search_pattern: pattern,
            result_entries,
        }.into()
    }
}

impl Reducible for AppState {
    type Action = AppAction;

    fn reduce(self: Rc<Self>, action: AppAction) -> Rc<Self> {
        match action {
            AppAction::ServicesLoaded(services) => {
                let this: Rc<Self> = Self {
                    services: services.into(),
                    .. Default::default()
                }.into();
                if !self.search_pattern.is_empty() {
                    //TODO Don't set "hash" to None
                    this.search_entries(self.search_pattern.clone(), None)
                } else {
                    this
                }
            }

            AppAction::SearchEntries(pattern) => {
                info!(format!("search entries: {:?}", pattern));
                self.search_entries(pattern, None)
            }

            AppAction::FollowLink(hpath) => {
                info!(format!("follow link: {:x}", hpath));
                if self.result_entries.contains(&hpath) {
                    set_location_hash(hpath).unwrap_throw();
                    self
                } else {
                    // It could be nice to load the file, but it may have too many entries.
                    // Use a safe and predictable, behavior.
                    let hstr = hpath.try_str(&self.services.hmappers);
                    let pattern = format!("{}", hstr);
                    self.search_entries(pattern, Some(hpath))
                }
            }

            AppAction::FilterEntryType(htype) => {
                info!(format!("filter entry type {:x}", htype));
                // No type should match an entry, so this should be fine
                let hstr = htype.try_str(&self.services.hmappers);
                let pattern = format!("{}", hstr);
                self.search_entries(pattern, None)
            }
        }
    }
}



// Use a wrapping struct just to be able to redefine `PartialEq`
#[derive(Clone)]
pub struct AppContext {
    inner: UseReducerHandle<AppState>,
}

impl PartialEq for AppContext {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(&*self.inner, &*other.inner)
    }
}

impl std::ops::Deref for AppContext {
    type Target = UseReducerHandle<AppState>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}



#[function_component(App)]
pub fn app() -> Html {
    //XXX We could use `suspense::use_future()` to properly handle the "loading" state
    // For now, just load asynchronously, on initial rendering
    let state = use_reducer(|| {
        let pattern = get_location_query().unwrap_throw();
        AppState::with_pattern(pattern.unwrap_or_default())
    });
    let context = AppContext { inner: state.clone() };

    let _ = {
        // On app start, "reset" the hash to forcily highlight it
        reset_location_hash().unwrap_throw();

        // Start loading services on app start
        // (It would be easier with a direct access to `Rc<AppState>`.)
        let state = state.clone();
        use_memo(move |_| {
            yew::platform::spawn_local(async move {
                let services = Services::load().await;
                state.dispatch(AppAction::ServicesLoaded(services));
            });
        }, ());
    };

    html! {
        <ContextProvider<AppContext> {context}>
            <div>
                <SearchBar value={state.search_pattern.clone()} />
                { html_result_count(&state) }
                <div id="bindata-content">
                    if !state.result_entries.is_empty() {
                        <ul>
                        { for state.result_entries.iter().map(|hpath| {
                             let htype = match state.services.entrydb.get_entry_type(*hpath) {
                                 Some(v) => v,
                                 None => {
                                     error!(format!("entry not found in database: {:x}", *hpath));
                                     return html! {};
                                 }
                             };
                             html! { <ResultEntry key={hpath.hash} hpath={*hpath} {htype} /> }
                         })
                        }
                        </ul>
                    }
                </div>
            </div>
        </ContextProvider<AppContext>>
    }
}


/// Return the result count displayed under the search bar
fn html_result_count(state: &AppState) -> Html {
    let entry_count = state.services.entrydb.entry_count();
    let nresults = state.result_entries.len();
    let mut results_count = format!("{}", nresults);
    // assume there was additional results if result count is exactly MAX_SEARCH_RESULTS
    if nresults >= settings::MAX_SEARCH_RESULTS {
        results_count.push('+');
    };
    html! {
        <div id="result-count">
            <b>{ results_count }</b>{" results out of "}<b>{ entry_count }</b>{" entries"}
        </div>
    }
}

/// Set URL hash, to jump to the given entry
fn set_location_hash(hpath: BinEntryPath) -> Result<(), JsValue> {
    let window = web_sys::window().unwrap_throw();
    window.location().set_hash(&format!("#{}", entry_element_id(hpath)))
}

/// Force a URL hash reset
fn reset_location_hash() -> Result<(), JsValue> {
    let window = web_sys::window().unwrap_throw();
    window.dispatch_event(&web_sys::HashChangeEvent::new("")?.into())?;
    Ok(())
}

/// Get current query items
fn get_location_query() -> Result<Option<String>, JsValue> {
    let window = web_sys::window().unwrap_throw();
    let search = window.location().search()?;
    let pattern = web_sys::UrlSearchParams::new_with_str(&search)?.get("s");
    Ok(pattern)
}

/// Push a new URL, with given query and optional hash
fn push_history_state(pattern: &str, hpath: Option<BinEntryPath>) -> Result<(), JsValue> {
    let query = js_sys::encode_uri_component(pattern);
    let url = match hpath {
        Some(hpath) => format!("?s={}#{}", query, entry_element_id(hpath)),
        None => format!("?s={}", query),
    };
    let window = web_sys::window().unwrap_throw();
    window.history()?.push_state_with_url(&JsValue::NULL, &"", Some(&url))
}

