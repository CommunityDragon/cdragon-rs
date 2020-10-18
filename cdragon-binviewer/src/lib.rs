
#![recursion_limit = "256"]
#[macro_use]
pub mod settings;
mod entrydb;
mod binloadservice;
mod sharedstate;
mod resultentry;

use std::fmt;
use log::{debug, info, error};
use yew::{html, Component, ComponentLink, Html, Renderable, ShouldRender};
use yew::events::IKeyboardEvent;
use stdweb::web::{
    event::IEvent,
    html_element::InputElement,
};
use stdweb::unstable::TryInto;
use cdragon_prop::data::*;
use cdragon_prop::BinHashMapper;

use sharedstate::SharedStateRef;
use entrydb::EntryDatabase;
use binloadservice::{
    HashMappersFetchTask,
    EntryDbFetchTask,
};
use resultentry::ResultEntry;

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;


/// Group all tasks used by the model
#[derive(Default)]
struct RunningTasks {
    hash_mappers: Option<HashMappersFetchTask>,
    entrydb: Option<EntryDbFetchTask>,
}

pub struct Model {
    state: SharedStateRef,
    result_entries: Vec<BinEntryPath>,
    /// Entry to expand and jump to, reset once built
    selected_entry: Option<BinEntryPath>,
    tasks: RunningTasks,
    link: ComponentLink<Model>,
}

pub enum Msg {
    /// Hash mapper data loaded
    BinHashMapperReady(BinHashKind, BinHashMapper),
    /// Entry database data loaded
    EntryDbReady(EntryDatabase),
    /// Search for entries, update results
    SearchEntries(String),
    /// Load given entry (if needed) then scroll to it
    GoToEntry(BinEntryPath),
    /// Load entries of the given type (update results)
    FilterEntryType(BinClassName),

    Ignore,
}

impl fmt::Debug for Msg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            // Specialize for better formatting of some paramters
            Msg::BinHashMapperReady(k, _) => write!(f, "BinHashMapperReady({:?}, data)", k),
            Msg::EntryDbReady(_) => write!(f, "EntryDbReady(db)"),
            Msg::SearchEntries(s) => write!(f, "SearchEntries({:?})", s),
            Msg::GoToEntry(h) => write!(f, "GoToEntry({:x})", h),
            Msg::FilterEntryType(h) => write!(f, "FilterEntryType({:x})", h),
            Msg::Ignore => write!(f, "Ignore"),
        }
    }
}


impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        let mut model = Self {
            state: Default::default(),
            result_entries: Default::default(),
            selected_entry: Default::default(),
            tasks: Default::default(),
            link,
        };

        {
            let callback = model.link.send_back(move |result| {
                match result {
                    Ok((kind, mapper)) => Msg::BinHashMapperReady(kind, mapper),
                    Err(e) => { error!("failed to load hash mapping: {}", e); Msg::Ignore }
                }
            });
            let task = get_binload_service!(model).fetch_hash_maps(callback);
            model.tasks.hash_mappers = Some(task);
        }

        {
            let callback = model.link.send_back(move |result: Result<EntryDatabase>| {
                match result {
                    Ok(db) => Msg::EntryDbReady(db),
                    Err(e) => { error!("failed to load entry db: {}", e); Msg::Ignore }
                }
            });
            let task = get_binload_service!(model).fetch_entrydb(callback);
            model.tasks.entrydb = Some(task);
        }

        model
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        debug!("Model message: {:?}", msg);
        match msg {
            Msg::BinHashMapperReady(kind, mapper) => {
                info!("setting bin hash map {:?}", kind);
                *self.state.borrow_mut().hash_mappers.get_mut(kind) = mapper;
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

            Msg::Ignore => false
        }
    }
}

impl Renderable<Model> for Model {
    fn view(&self) -> Html<Self> {
        html! {
            <div>
                <div id="search">
                    <input type="text"
                        placeholder="Search entries"
                        onkeypress=|e| {
                            match e.current_target() {
                                Some(ref this) if e.key() == "Enter" => {
                                    this.clone().try_into().map(|e: InputElement| Msg::SearchEntries(e.raw_value())).ok()
                                }
                                _ => None
                            }.unwrap_or(Msg::Ignore)
                        } />
                    { self.view_result_count() }
                </div>
                <div id="bindata-content">
                    { self.view_current_bindata() }
                </div>
            </div>
        }
    }
}

impl Model {
    /// Return the result count displayed under the search bar
    fn view_result_count(&self) -> Html<Self> {
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
    fn view_current_bindata(&self) -> Html<Self> {
        if !self.result_entries.is_empty() {
            html! {
                <ul>
                    { for self.result_entries.iter().map(|hpath| {
                        let state = self.state.clone();
                        let htype = match get_state!(self).entrydb.get_entry_type(*hpath) {
                            Some(v) => v,
                            None => {
                                error!("entry not found in database: {:x}", *hpath);
                                return html! {};
                            }
                        };
                        let select = self.selected_entry.map(|h| h == *hpath).unwrap_or(false);
                        html! { <ResultEntry state=state send_model=|m|{m} hpath=*hpath htype=htype select=select /> }
                      })
                    }
                </ul>
            }
        } else {
            html! {}
        }
    }
}

