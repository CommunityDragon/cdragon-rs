#![recursion_limit = "256"]
pub mod settings;
mod entrydb;
mod hooks;
mod services;
mod components;
mod binview;
mod utils;

use std::rc::Rc;
use gloo_console::{info, error};
use yew::prelude::*;
use wasm_bindgen::{
    JsCast,
    JsValue,
    UnwrapThrowExt,
    closure::Closure,
};
use cdragon_prop::data::*;

use services::Services;
use components::*;
use utils::*;

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;


pub enum AppAction {
    /// Switch the state to a loaded one
    ServicesLoaded(Services),
    /// Search for entries
    SearchEntries(String),
    /// Load given entry (if needed) then focus it
    FollowLink(BinEntryPath),
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
    fn from_location(services: Rc<Services>) -> Self {
        let (pattern, focus) = parse_app_url();
        Self::from_search(services, pattern, focus)
    }

    /// Search and return a new instance
    fn from_search(services: Rc<Services>, pattern: String, focus: Option<BinEntryPath>) -> Self {
        let words: Vec<&str> = pattern.split_whitespace().collect();
        let result_entries = if words.is_empty() {
            Vec::new()
        } else {
            match services.entrydb.search_words(&words, &services.hmappers) {
                Ok(it) => it.take(settings::max_search_results()).collect(),
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
        let url = build_app_url(&self.search_pattern, self.focused_entry);
        let window = web_sys::window().unwrap_throw();
        window.history()?.push_state_with_url(&JsValue::NULL, "", Some(&url))
    }
}

impl Reducible for AppState {
    type Action = AppAction;

    fn reduce(mut self: Rc<Self>, action: AppAction) -> Rc<Self> {
        match action {
            AppAction::ServicesLoaded(services) => {
                // Load the location after initial load, no need to preserve current state
                Self::from_location(services.into()).into()
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
                    let hstr = hpath.seek_str(&self.services.hmappers);
                    let pattern = format!("{}", hstr);
                    self.search_and_push(pattern, Some(hpath))
                }
            }

            AppAction::LoadHistoryState => {
                Self::from_location(self.services.clone()).into()
            }
        }
    }
}


pub type AppContext = Rc<Services>;


#[function_component(App)]
pub fn app() -> Html {
    // Note: URL is loaded after services are loaded
    let state = use_reducer(AppState::default);
    use_memo((), {
        let state = state.clone();
        move |_| {
            yew::platform::spawn_local(async move {
                let services = Services::load().await;
                state.dispatch(AppAction::ServicesLoaded(services));
            });
        }
    });

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
    use_effect_with((), {
        let state = state.clone();
        move |_| {
            let window = web_sys::window().unwrap_throw();
            let listener: Closure<dyn FnMut()> = Closure::new(move || state.dispatch(AppAction::LoadHistoryState));
            window.add_event_listener_with_callback("popstate", listener.as_ref().unchecked_ref()).unwrap_throw();

            move || drop(listener)
        }
    });

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
    // assume there was additional results if result count is exactly max_search_results
    if nresults >= settings::max_search_results() {
        results_count.push('+');
    };
    html! {
        <div id="result-count">
            <b>{ results_count }</b>{" results out of "}<b>{ entry_count }</b>{" entries"}
        </div>
    }
}

