pub use searchbar::SearchBar;
pub use resultentry::ResultEntry;

mod searchbar {
    use yew::prelude::*;
    use web_sys::{Event, HtmlInputElement};
    use wasm_bindgen::{JsCast, UnwrapThrowExt};

    #[derive(Clone, PartialEq, Properties)]
    pub struct Props {
        pub value: String,
        pub on_search: Callback<String>,
    }

    #[function_component(SearchBar)]
    pub fn search_bar(props: &Props) -> Html {
        let value = props.value.clone();
        let onchange = props.on_search.reform(move |e: Event| {
            let target = e.target().unwrap_throw();
            let target: HtmlInputElement = target.dyn_into().unwrap_throw();
            target.value()
        });

        html! {
            <div id="search">
                <input type="search" placeholder="Search entries" {value} {onchange} />
            </div>
        }
    }
}

mod resultentry;

