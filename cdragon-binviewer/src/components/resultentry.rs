use gloo_console::error;
use yew::prelude::*;
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
    pub hpath: BinEntryPath,
    pub htype: BinClassName,
}


enum ResultEntryState {
    Folded,
    Loading,
    Opened(BinEntry),
}

impl ResultEntryState {
    fn entry(&self) -> Option<&BinEntry> {
        match self {
            Self::Opened(entry) => Some(entry),
            _ => None,
        }
    }
}


#[function_component(ResultEntry)]
pub fn result_entry(props: &Props) -> Html {
    let state = use_context::<AppContext>().unwrap();
    let entry_state = use_state(|| ResultEntryState::Folded);

    let load_entry = {
        let state = state.clone();
        let entry_state = entry_state.clone();
        let hpath = props.hpath;
        use_async(async move {
            let file = state.services.entrydb.get_entry_file(hpath).unwrap().to_string();
            let result = state.services.fetch_entry(&file, hpath).await;
            entry_state.set(match result {
                Ok(entry) => ResultEntryState::Opened(entry),
                Err(e) => {
                    error!(format!("failed to load bin entry: {}", e));
                    ResultEntryState::Folded
                }
            });
        })
    };

    let on_header_click = {
        let entry_state = entry_state.clone();
        Callback::from(move |_| {
            match *entry_state {
                ResultEntryState::Folded => {
                    entry_state.set(ResultEntryState::Loading);
                    load_entry.run();
                }
                ResultEntryState::Loading => {}
                ResultEntryState::Opened(_) => {
                    entry_state.set(ResultEntryState::Folded);
                }
            };
        })
    };

    let on_type_click = {
        let state = state.clone();
        let htype = props.htype;
        move |_| state.dispatch(AppAction::FilterEntryType(htype))
    };

    let mut b = BinViewBuilder::new(&state.services.hmappers);
    let entry = entry_state.entry();
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
                                { for entry.fields.iter().map(|v| view_binfield(&state, &mut b, v)) }
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

