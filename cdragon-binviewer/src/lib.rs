#![recursion_limit = "256"]
#[macro_use]
pub mod settings;
mod entrydb;
mod hooks;
mod services;
mod components;
mod binview;

use std::rc::Rc;
use gloo_console::{info, error};
use yew::prelude::*;
use wasm_bindgen::{
    JsCast,
    JsValue,
    UnwrapThrowExt,
    closure::Closure,
};
use web_sys::UrlSearchParams;
use cdragon_prop::data::*;
use cdragon_utils::hashes::HashDef;

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
    /// Load given history state
    LoadHistoryState,
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
    /// Parse search from location, search and return a new instance
    fn from_location(services: Rc<Services>) -> Result<Self, JsValue> {
        let window = web_sys::window().unwrap_throw();
        let search = window.location().search()?;
        let params = UrlSearchParams::new_with_str(&search)?;
        let pattern = params.get("s").unwrap_or_default();
        let focus = params.get("e").and_then(|s| u32::from_str_radix(&s, 16).ok().map(BinEntryPath::new));
        Ok(Self::from_search(services, pattern, focus))
    }

    /// Search and return a new instance
    fn from_search(services: Rc<Services>, pattern: String, focus: Option<BinEntryPath>) -> Self {
        let words: Vec<&str> = pattern.split_whitespace().collect();
        let result_entries = if words.is_empty() {
            Vec::new()
        } else {
            match services.entrydb.search_words(&words, &services.hmappers) {
                Ok(it) => it.take(settings::MAX_SEARCH_RESULTS).collect(),
                Err(e) => {
                    error!(format!("search failed: {}", e));
                    vec![]
                }
            }
        };
        Self {
            services,
            search_pattern: pattern,
            result_entries,
            //XXX Keep previously opened entries?
            focused_entry: focus,
        }
    }

    fn search_and_push(self: Rc<Self>, pattern: String, focus: Option<BinEntryPath>) -> Rc<Self> {
        let this: Rc<Self> = Self::from_search(self.services.clone(), pattern, focus).into();
        this.push_history().unwrap_throw();
        this
    }


    /// Push state to history
    fn push_history(&self) -> Result<(), JsValue> {
        let query = js_sys::encode_uri_component(&self.search_pattern);
        let url = match self.focused_entry {
            Some(hpath) => format!("?s={}&e={:x}#{}", query, hpath, entry_element_id(hpath)),
            None => format!("?s={}", query),
        };
        let window = web_sys::window().unwrap_throw();
        //TODO Push open/close state
        window.history()?.push_state_with_url(&JsValue::NULL, &"", Some(&url))
    }
}

impl Reducible for AppState {
    type Action = AppAction;

    fn reduce(mut self: Rc<Self>, action: AppAction) -> Rc<Self> {
        match action {
            AppAction::ServicesLoaded(services) => {
                // Load the location after initial load, no need to preserve current state
                Self::from_location(services.into()).unwrap_throw().into()
            }

            AppAction::SearchEntries(pattern) => {
                info!(format!("search entries: {:?}", pattern));
                self.search_and_push(pattern, None)
            }

            AppAction::FollowLink(hpath) => {
                info!(format!("follow link: {:x}", hpath));
                if self.result_entries.contains(&hpath) {
                    Rc::make_mut(&mut self).focused_entry = Some(hpath);
                    self.push_history().unwrap_throw();
                    self
                } else {
                    // It could be nice to load the file, but it may have too many entries.
                    // Use a safe and predictable, behavior.
                    let hstr = hpath.try_str(&self.services.hmappers);
                    let pattern = format!("{}", hstr);
                    self.search_and_push(pattern, Some(hpath))
                }
            }

            AppAction::FilterEntryType(htype) => {
                info!(format!("filter entry type {:x}", htype));
                // No type should match an entry, so this should be fine
                let hstr = htype.try_str(&self.services.hmappers);
                let pattern = format!("{}", hstr);
                self.search_and_push(pattern, None)
            }

            AppAction::FilterFile(file) => {
                info!(format!("filter file: {}", &file));
                self.search_and_push(file, None)
            }

            AppAction::LoadHistoryState => {
                //TODO Don't re-search, use the state
                Self::from_location(self.services.clone()).unwrap_throw().into()
            }
        }
    }
}


pub type AppContext = Rc<Services>;


#[function_component(App)]
pub fn app() -> Html {
    // Note: URL is loaded after services are loaded
    let state = use_reducer(AppState::default);
    use_memo({
        let state = state.clone();
        move |_| {
            yew::platform::spawn_local(async move {
                let services = Services::load().await;
                state.dispatch(AppAction::ServicesLoaded(services));
            });
        }
    }, ());

    let on_search = Callback::from({
        let state = state.clone();
        move |value| state.dispatch(AppAction::SearchEntries(value))
    });

    let dispatch = Callback::from({
        let state = state.clone();
        move |action| state.dispatch(action)
    });

    let services = state.services.clone();
    let focused_entry = state.focused_entry;

    // Setup listener for history change
    use_effect_with_deps({
        let state = state.clone();
        move |_| {
            let window = web_sys::window().unwrap_throw();
            let listener: Closure<dyn FnMut()> = Closure::new(move || state.dispatch(AppAction::LoadHistoryState));
            window.add_event_listener_with_callback("popstate", listener.as_ref().unchecked_ref()).unwrap_throw();

            move || drop(listener)
        }
    }, ());

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

