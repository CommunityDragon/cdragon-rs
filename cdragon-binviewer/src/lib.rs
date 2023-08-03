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
    /// Search for entries
    SearchEntries(String),
    /// Load given entry (if needed) then focus it
    FollowLink(BinEntryPath),
    /// Load entries of the given type
    FilterEntryType(BinClassName),
    /// Load entries of the given file
    FilterFile(String),
}

#[derive(Clone, Default)]
pub struct AppState {
    /// Services, loaded at start
    services: Rc<Services>,
    /// Current search pattern
    search_pattern: String,
    /// Result entries, displayed
    result_entries: Vec<BinEntryPath>,
    /// Entry to forcily open and jump to
    focused_entry: Option<BinEntryPath>,
}

impl AppState {
    fn with_pattern(search_pattern: String) -> Self {
        //XXX `focused_entry` should be read from hash
        Self {
            search_pattern,
            .. Default::default()
        }
    }

    fn search_entries(self: Rc<Self>, pattern: String, focus: Option<BinEntryPath>, push_history: bool) -> Rc<Self> {
        let words: Vec<&str> = pattern.split_whitespace().collect();
        let result_entries = match self.services.entrydb.search_words(&words, &self.services.hmappers) {
            Ok(it) => it.take(settings::MAX_SEARCH_RESULTS).collect(),
            Err(e) => {
                error!(format!("search failed: {}", e));
                vec![]
            }
        };
        debug!(format!("new search results: {} entries", result_entries.len()));
        if push_history {
            push_history_state(&pattern, focus).unwrap_throw();
        }
        Self {
            services: self.services.clone(),
            search_pattern: pattern,
            result_entries,
            focused_entry: focus,
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
                    this.search_entries(self.search_pattern.clone(), self.focused_entry, false)
                } else {
                    this
                }
            }

            AppAction::SearchEntries(pattern) => {
                info!(format!("search entries: {:?}", pattern));
                self.search_entries(pattern, None, true)
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
                    self.search_entries(pattern, Some(hpath), true)
                }
            }

            AppAction::FilterEntryType(htype) => {
                info!(format!("filter entry type {:x}", htype));
                // No type should match an entry, so this should be fine
                let hstr = htype.try_str(&self.services.hmappers);
                let pattern = format!("{}", hstr);
                self.search_entries(pattern, None, true)
            }

            AppAction::FilterFile(file) => {
                info!(format!("filter file: {}", &file));
                self.search_entries(file, None, true)
            }
        }
    }
}


pub type AppContext = Rc<Services>;


#[function_component(App)]
pub fn app() -> Html {
    let state = use_reducer(|| {
        let pattern = get_location_query().unwrap_throw();
        AppState::with_pattern(pattern.unwrap_or_default())
    });
    {
        let state = state.clone();
        use_memo(move |_| {
            yew::platform::spawn_local(async move {
                let services = Services::load().await;
                state.dispatch(AppAction::ServicesLoaded(services));
            });
        }, ());
    }

    let on_search = {
        let state = state.clone();
        Callback::from(move |value| state.dispatch(AppAction::SearchEntries(value)))
    };

    let dispatch = {
        let state = state.clone();
        Callback::from(move |action| state.dispatch(action))
    };

    let services = state.services.clone();
    let focused_entry = state.focused_entry;

    html! {
        <ContextProvider<AppContext> context={services.clone()}>
            <div>
                <SearchBar value={state.search_pattern.clone()} {on_search} />
                { html_result_count(&state) }
                <div id="bindata-content">
                    if !state.result_entries.is_empty() {
                        <ul>
                        { for state.result_entries.iter().map(move |hpath| {
                             if services.entrydb.has_entry(*hpath) {
                                 let focus = focused_entry == Some(*hpath);
                                 html! {
                                     <ResultEntry key={hpath.hash} dispatch={dispatch.clone()} hpath={*hpath} {focus} />
                                 }
                             } else {
                                 error!(format!("entry not found in database: {:x}", *hpath));
                                 html! {}
                             }
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

