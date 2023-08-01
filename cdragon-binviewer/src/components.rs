pub use searchbar::SearchBar;
pub use resultentry::{ResultEntry, entry_element_id};

mod searchbar {
    use yew::prelude::*;
    use web_sys::{Event, HtmlInputElement};
    use wasm_bindgen::{JsCast, UnwrapThrowExt};
    use crate::{AppContext, AppAction};

    #[derive(Clone, PartialEq, Properties)]
    pub struct Props {
        pub value: String,
    }

    #[function_component(SearchBar)]
    pub fn search_bar(props: &Props) -> Html {
        let Props { value } = props.clone();

        let state = use_context::<AppContext>().unwrap();

        let onchange = move |e: Event| {
            let target = e.target().unwrap_throw();
            let target: HtmlInputElement = target.dyn_into().unwrap_throw();
            state.dispatch(AppAction::SearchEntries(target.value()));
        };

        html! {
            <div id="search">
                <input type="search" placeholder="Search entries" {value} {onchange} />
            </div>
        }
    }
}

mod resultentry;

