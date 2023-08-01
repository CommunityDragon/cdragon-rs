#![recursion_limit = "256"]
#[macro_use]
pub mod settings;
mod entrydb;
mod binloadservice;
mod services;
mod resultentry;
mod binview;

use std::fmt;
use std::rc::Rc;
use gloo_console::{debug, info, error};
use yew::{
    prelude::*,
    events::KeyboardEvent,
};
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;
use cdragon_prop::data::*;

use services::Services;
use resultentry::{
    ResultEntry,
    entry_element_id,
};

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

/// Values shared to all components
#[derive(Clone, Default)]
pub struct AppState {
    services: Rc<Services>,
    messaging: Callback<Msg>,
}

impl PartialEq for AppState {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.services, &other.services) &&
        self.messaging.eq(&other.messaging)
    }
}


#[derive(Default)]
pub struct App {
    state: AppState,
    result_entries: Vec<BinEntryPath>,
}

pub enum Msg {
    /// Switch the state to a loaded one
    ServicesLoaded(Services),
    /// Search for entries, update results
    SearchEntries(String),
    /// Load given entry (if needed) then scroll to it
    GoToEntry(BinEntryPath),
    /// Load entries of the given type (update results)
    FilterEntryType(BinClassName),
}

impl fmt::Debug for Msg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            // Specialize for better formatting of some paramters
            Msg::ServicesLoaded(_) => write!(f, "ServicesLoaded(state)"),
            Msg::SearchEntries(s) => write!(f, "SearchEntries({:?})", s),
            Msg::GoToEntry(h) => write!(f, "GoToEntry({:x})", h),
            Msg::FilterEntryType(h) => write!(f, "FilterEntryType({:x})", h),
        }
    }
}


impl Component for App {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_future(async move {
            let state = Services::load().await;
            Msg::ServicesLoaded(state)
        });

        let state = AppState {
            services: Default::default(),
            messaging: ctx.link().callback(std::convert::identity),
        };

        App { state, ..Default::default() }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        debug!(format!("App message: {:?}", msg));
        match msg {
            Msg::ServicesLoaded(services) => {
                info!("switching to loaded state");
                self.state.services = Rc::new(services);
                true
            }

            Msg::SearchEntries(pattern) => {
                info!(format!("search entries: {:?}", pattern));
                let services = &self.state.services;
                let words: Vec<&str> = pattern.split_whitespace().collect();
                let results = match services.entrydb.search_words(&words, &services.hash_mappers) {
                    Ok(it) => it.take(settings::MAX_SEARCH_RESULTS).collect(),
                    Err(e) => {
                        error!(format!("search failed: {}", e));
                        return false;
                    }
                };
                self.result_entries = results;
                debug!(format!("new search results: {} entries", self.result_entries.len()));
                true
            }

            Msg::GoToEntry(hpath) => {
                info!(format!("go to entry {:x}", hpath));
                if !self.result_entries.contains(&hpath) {
                    if let Some(path) = self.state.services.entrydb.get_entry_file(hpath) {
                        ctx.link().send_message(Msg::SearchEntries(path.into()));
                    }
                }
                //XXX Does not work if navigating due to search change
                // To be reworked when handling the query as part of the URL
                if let Some(window) = web_sys::window() {
                    let _ = window.location().set_hash(&format!("#{}", entry_element_id(hpath)));
                }
                true
            }

            Msg::FilterEntryType(htype) => {
                info!(format!("filter entry type {:x}", htype));
                let results: Vec<BinEntryPath> = self.state.services.entrydb
                    .iter_by_type(htype)
                    .take(settings::MAX_SEARCH_RESULTS)
                    .collect();
                debug!(format!("collected by entry type: {}", results.len()));
                if results.is_empty() {
                    // no result, probably not an entry type, ignore it
                    false
                } else {
                    self.result_entries = results;
                    true
                }
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let onkeypress = ctx.link().batch_callback(move |e: KeyboardEvent| {
            if e.key() == "Enter" {
                if let Some(input) = e.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok()) {
                    return Some(Msg::SearchEntries(input.value()));
                }
            }
            None
        });

        let state = self.state.clone();
        html! {
            <ContextProvider<AppState> context={state}>
                <div>
                    <div id="search">
                        <input type="text" placeholder="Search entries" {onkeypress} />
                        { self.view_result_count() }
                    </div>
                    <div id="bindata-content">
                        { self.view_current_bindata() }
                    </div>
                </div>
            </ContextProvider<AppState>>
        }
    }
}

impl App {
    /// Return the result count displayed under the search bar
    fn view_result_count(&self) -> Html {
        let entry_count = self.state.services.entrydb.entry_count();
        let nresults = self.result_entries.len();
        let mut results_count = format!("{}", nresults);
        // assume there was additional results if result count is exactly MAX_SEARCH_RESULTS
        if nresults >= settings::MAX_SEARCH_RESULTS {
            results_count.push('+');
        };
        html! {
            <p><b>{ results_count }</b>{" results out of "}<b>{ entry_count }</b>{" entries"}</p>
        }
    }

    /// Return view for current bin data (results, expanded entry)
    fn view_current_bindata(&self) -> Html {
        if !self.result_entries.is_empty() {
            html! {
                <ul>
                    { for self.result_entries.iter().map(|hpath| {
                        let htype = match self.state.services.entrydb.get_entry_type(*hpath) {
                            Some(v) => v,
                            None => {
                                error!(format!("entry not found in database: {:x}", *hpath));
                                return html! {};
                            }
                        };
                        html! { <ResultEntry hpath={*hpath} {htype} /> }
                      })
                    }
                </ul>
            }
        } else {
            html! {}
        }
    }
}

