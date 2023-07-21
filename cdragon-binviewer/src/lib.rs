
#![recursion_limit = "256"]
pub mod settings;
mod entrydb;
mod binloadservice;
mod sharedstate;
mod resultentry;

use std::fmt;
use log::{debug, info, error};
use yew::{html, Component, Context, Html};
use yew::events::KeyboardEvent;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;
use cdragon_prop::data::*;

use sharedstate::SharedStateRef;
use entrydb::EntryDatabase;
use resultentry::ResultEntry;
use binloadservice::FetchedHashMappers;

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;


#[derive(Default)]
pub struct Model {
    state: SharedStateRef,
    result_entries: Vec<BinEntryPath>,
    /// Entry to expand and jump to, reset once built
    selected_entry: Option<BinEntryPath>,
}

pub enum Msg {
    /// Hash mappers data loaded
    BinHashMappersReady(FetchedHashMappers),
    /// Entry database data loaded
    EntryDbReady(EntryDatabase),
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
            Msg::BinHashMappersReady(_) => write!(f, "BinHashMapperReady(data)"),
            Msg::EntryDbReady(_) => write!(f, "EntryDbReady(db)"),
            Msg::SearchEntries(s) => write!(f, "SearchEntries({:?})", s),
            Msg::GoToEntry(h) => write!(f, "GoToEntry({:x})", h),
            Msg::FilterEntryType(h) => write!(f, "FilterEntryType({:x})", h),
        }
    }
}


impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let model = Self::default();
        let link = ctx.link();

        {
            let state = model.state.clone();
            link.send_future_batch(async move {
                let result = get_binload_service!(state).fetch_hash_maps().await;
                match result {
                    Ok(mappers) => vec![Msg::BinHashMappersReady(mappers)],
                    Err(e) => { error!("failed to load hash mappers: {}", e); vec![] }
                }
            });
        }

        {
            let state = model.state.clone();
            link.send_future_batch(async move {
                let result = get_binload_service!(state).fetch_entrydb().await;
                match result {
                    Ok(db) => vec![Msg::EntryDbReady(db)],
                    Err(e) => { error!("failed to load entry db: {}", e); vec![] }
                }
            });
        }

        model
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        debug!("Model message: {:?}", msg);
        match msg {
            Msg::BinHashMappersReady(mappers) => {
                info!("setting bin hash mappers");
                let hash_mappers = &mut self.state.borrow_mut().hash_mappers;
                hash_mappers.entry_path = mappers.entry_path;
                hash_mappers.class_name = mappers.class_name;
                hash_mappers.field_name = mappers.field_name;
                hash_mappers.hash_value = mappers.hash_value;
                false
            }

            Msg::EntryDbReady(db) => {
                info!("entry database loaded ({} entries)", db.entry_count());
                self.state.borrow_mut().entrydb = db;
                false
            }

            Msg::SearchEntries(pattern) => {
                info!("search entries: {:?}", pattern);
                let words: Vec<&str> = pattern.split_whitespace().collect();
                let mappers = &self.state.as_ref().borrow().hash_mappers;
                let results = match get_state!(self).entrydb.search_words(&words, mappers) {
                    Ok(it) => it.take(settings::MAX_SEARCH_RESULTS).collect(),
                    Err(e) => {
                        error!("search failed: {}", e);
                        return false;
                    }
                };
                self.result_entries = results;
                debug!("new search results: {} entries", self.result_entries.len());
                true
            }

            Msg::GoToEntry(hpath) => {
                info!("go to entry {:x}", hpath);
                if !self.result_entries.contains(&hpath) {
                    // add the element to the results and jump to it
                    self.result_entries.insert(0, hpath);
                }
                self.selected_entry = Some(hpath);
                true
            }

            Msg::FilterEntryType(htype) => {
                info!("filter entry type {:x}", htype);
                let results: Vec<BinEntryPath> = get_state!(self).entrydb
                    .iter_by_type(htype)
                    .take(settings::MAX_SEARCH_RESULTS)
                    .collect();
                debug!("collected by entry type: {}", results.len());
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
        let onkeypress = ctx.link().batch_callback(|e: KeyboardEvent| {
            e.current_target().and_then(|this| {
                if e.key() == "Enter" {
                    this.dyn_into::<HtmlInputElement>().ok()
                        .map(|input| Msg::SearchEntries(input.value()))
                } else {
                    None
                }
            })
        });

        html! {
            <div>
                <div id="search">
                    <input type="text"
                        placeholder="Search entries"
                        {onkeypress}
                    />
                    { self.view_result_count() }
                </div>
                <div id="bindata-content">
                    { self.view_current_bindata(ctx) }
                </div>
            </div>
        }
    }
}

impl Model {
    /// Return the result count displayed under the search bar
    fn view_result_count(&self) -> Html {
        let entry_count = get_state!(self).entrydb.entry_count();
        let nresults = self.result_entries.len();
        let mut results_count = format!("{}", nresults);
        // assume there was additional results if result count is exactly MAX_SEARCH_RESULTS
        if nresults >= settings::MAX_SEARCH_RESULTS {
            results_count.push_str("+");
        };
        html! {
            <p><b>{ results_count }</b>{" results out of "}<b>{ entry_count }</b>{" entries"}</p>
        }
    }

    /// Return view for current bin data (results, expanded entry)
    fn view_current_bindata(&self, ctx: &Context<Self>) -> Html {
        if !self.result_entries.is_empty() {
            html! {
                <ul>
                    { for self.result_entries.iter().map(|hpath| {
                        let state = self.state.clone();
                        let send_model = ctx.link().callback(|m| m);
                        let htype = match get_state!(self).entrydb.get_entry_type(*hpath) {
                            Some(v) => v,
                            None => {
                                error!("entry not found in database: {:x}", *hpath);
                                return html! {};
                            }
                        };
                        let select = self.selected_entry.map(|h| h == *hpath).unwrap_or(false);
                        html! { <ResultEntry {state} {send_model} hpath={*hpath} {htype} {select} /> }
                      })
                    }
                </ul>
            }
        } else {
            html! {}
        }
    }
}

