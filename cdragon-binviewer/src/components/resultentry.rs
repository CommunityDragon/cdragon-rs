use std::rc::Rc;
use gloo_console::error;
use yew::prelude::*;
use wasm_bindgen::{JsValue, UnwrapThrowExt};
use cdragon_prop::{
    BinEntryPath,
    BinClassName,
    BinEntry,
};
use crate::{
    AppContext,
    AppAction,
    binview::{BinViewBuilder, view_binfield},
    hooks::use_async,
};


#[derive(Properties, PartialEq)]
pub struct Props {
    /// Send back actions to the app
    pub dispatch: Callback<AppAction>,
    pub hpath: BinEntryPath,
    pub htype: BinClassName,
    /// True to forcily open the entry and jump to it when loaded
    pub focus: bool,
}


enum State {
    Empty,
    Loading,
    Opened(Rc<BinEntry>),
    Closed(Rc<BinEntry>),
}

impl State {
    fn displayed_entry(&self) -> Option<&BinEntry> {
        match self {
            Self::Opened(entry) => Some(entry),
            _ => None,
        }
    }
}


#[function_component(ResultEntry)]
pub fn result_entry(props: &Props) -> Html {
    let services = use_context::<AppContext>().unwrap();
    let state = use_state(|| State::Empty);

    let load_entry = {
        let services = services.clone();
        let state = state.clone();
        let hpath = props.hpath;
        use_async(async move {
            let file = services.entrydb.get_entry_file(hpath).unwrap().to_string();
            let result = services.fetch_entry(&file, hpath).await;
            state.set(match result {
                Ok(entry) => State::Opened(entry.into()),
                Err(e) => {
                    error!(format!("failed to load bin entry: {}", e));
                    State::Empty
                }
            });
        })
    };

    let toggle_entry = {
        let state = state.clone();
        Callback::from(move |()| {
            match &*state {
                State::Empty => {
                    state.set(State::Loading);
                    load_entry.run();
                }
                State::Loading => {}
                State::Opened(entry) => {
                    state.set(State::Closed(entry.clone()));
                }
                State::Closed(entry) => {
                    state.set(State::Opened(entry.clone()));
                }
            };
        })
    };

    let on_header_click = toggle_entry.reform(|_| ());

    let on_type_click = {
        let htype = props.htype;
        props.dispatch.reform(move |_| AppAction::FilterEntryType(htype))
    };

    let on_link_click = props.dispatch.reform(AppAction::FollowLink);

    // Focus if asked to and wasn't before
    {
        let focus_after_render = {
            let state = state.clone();
            let toggle_entry = toggle_entry;
            use_memo(move |focus| {
                if let (true, State::Empty | State::Closed(_)) = (focus, &*state) {
                    toggle_entry.emit(());
                }
                *focus
            }, props.focus)
        };

        use_effect_with_deps(move |(focus, opened)| {
            if *focus && *opened {
                // Assume the hash is correct
                reset_location_hash().unwrap_throw();
            }
        }, (*focus_after_render, matches!(*state, State::Opened(_))));
    }

    let mut b = BinViewBuilder::new(&services.hmappers, on_link_click);
    let entry = state.displayed_entry();
    let item_class = if entry.is_some() { None } else { Some("collapsed") };
    let element_id = entry_element_id(props.hpath);

    html! {
        <li>
            <div class="bin-entry" id={element_id}>
                <div class={classes!("bin-entry-header", "bin-item-header", item_class)}
                    onclick={on_header_click}>
                    <span class="bin-entry-path">
                        { b.format_entry_path(props.hpath) }
                    </span>
                    {" "}
                    <span class="bin-entry-type"
                          onclick={on_type_click}>
                        { b.format_type_name(props.htype) }
                    </span>
                </div>
                {
                    match entry {
                        Some(entry) => html! {
                            <ul>
                                { for entry.fields.iter().map(|v| view_binfield(&mut b, v)) }
                            </ul>
                        },
                        None => html! {},
                    }
                }
            </div>
        </li>
    }
}


pub fn entry_element_id(hpath: BinEntryPath) -> String {
    format!("entry-{:x}", hpath)
}

/// Force a URL hash reset
fn reset_location_hash() -> Result<(), JsValue> {
    let window = web_sys::window().unwrap_throw();
    window.location().set_hash(&window.location().hash()?)
}

