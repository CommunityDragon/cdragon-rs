use std::fmt;
use log::{debug, error};
use yew::prelude::*;
use yew::{classes, html, Component, Html, Callback};
use wasm_bindgen::UnwrapThrowExt;
use cdragon_prop::{
    BinEntryPath,
    BinClassName,
    BinEntry,
};
use super::SharedStateRef;
use binview::{BinViewBuilder, view_binfield};
use super::Msg as ModelMsg;


#[derive(Default)]
pub struct ResultEntry {
    entry: Option<BinEntry>,
}

pub enum Msg {
    ToggleCollapse,
    SetEntry(BinEntry),
    Forward(ModelMsg),
}

impl fmt::Debug for Msg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Msg::ToggleCollapse => write!(f, "ToggleCollapse"),
            Msg::SetEntry(_) => write!(f, "SetEntry(entry)"),
            Msg::Forward(m) => write!(f, "Forward({:?})", m),
        }
    }
}


#[derive(Properties)]
pub struct Props {
    pub state: SharedStateRef,
    pub hpath: BinEntryPath,
    pub htype: BinClassName,
    pub send_model: Callback<ModelMsg>,
    /// Force expand and scroll into view if true, do nothing if false
    #[prop_or_default]
    pub select: bool,
}

impl Props {
    /// Return the HTML ID of the "main" element
    fn element_id(&self) -> String {
        format!("entry-{:x}", self.hpath)
    }
}

impl PartialEq for Props {
    fn eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.state, &other.state) &&
        self.hpath.eq(&other.hpath) &&
        self.htype.eq(&other.htype) &&
        self.send_model.eq(&other.send_model) &&
        self.select.eq(&other.select)
    }
}

impl Component for ResultEntry {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();
        let mut entry = ResultEntry::default();
        if props.select {
            entry.select_entry(ctx); //TODO
        }
        entry
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        debug!("ResultEntry message: {:?}", msg);
        match msg {
            Msg::ToggleCollapse => {
                if self.is_collapsed() {
                    self.expand_entry(ctx);
                    false
                } else {
                    self.collapse_entry();
                    true
                }
            },

            Msg::SetEntry(entry) => {
                self.entry = Some(entry);
                true
            },

            Msg::Forward(m) => {
                ctx.props().send_model.emit(m);
                false
            }
        }
    }

    fn changed(&mut self, ctx: &Context<Self>) -> bool {
        let props = ctx.props();
        if self.entry.as_ref().map(|e| e.path) != Some(props.hpath) {
            self.entry = None;
        }
        if props.select {
            self.select_entry(ctx); //TODO
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let mappers = &props.state.as_ref().borrow().hash_mappers;
        let mut b = BinViewBuilder::new(mappers);
        let item_class = if self.entry.is_none() { "collapsed" } else { "" };
        let onclick_htype = props.htype.clone();

        let on_header_click = ctx.link().callback(|_| Msg::ToggleCollapse);
        let on_type_click = ctx.link().callback(move |_| Msg::Forward(ModelMsg::FilterEntryType(onclick_htype)));

        html! {
            <li>
                <div class="bin-entry" id={props.element_id()}>
                    <div class={classes!("bin-entry-header", "bin-item-header", item_class)}
                        onclick={on_header_click}>
                        <span class="bin-entry-path">
                            { b.format_entry_path(props.hpath) }
                        </span>
                        <span class="bin-entry-type"
                              onclick={on_type_click}>
                            { b.format_type_name(props.htype) }
                        </span>
                    </div>
                    { self.view_expanded_entry(ctx, &mut b) }
                </div>
            </li>
        }
    }
}

impl ResultEntry {
    fn is_collapsed(&self) -> bool {
        !self.entry.is_some()
    }

    fn collapse_entry(&mut self) {
        self.entry = None;
    }

    fn expand_entry(&mut self, ctx: &Context<Self>) {
        if self.entry.is_some() {
            return;  // already expanded
        }

        let props = ctx.props();
        let hpath = props.hpath;
        let state = props.state.clone();
        ctx.link().send_future_batch(async move {
            let file = state.borrow().entrydb.get_entry_file(hpath).unwrap().to_string();
            let result = get_binload_service!(state).fetch_entry(&file, hpath).await;
            match result {
                Ok(entry) => vec![Msg::SetEntry(entry)],
                Err(e) => { error!("failed to load bin entry: {}", e); vec![] }
            }
        });
    }

    /// Expand entry and scroll to it
    fn select_entry(&mut self, ctx: &Context<Self>) {
        self.expand_entry(ctx);
        web_sys::window().expect_throw("window is undefined")
            .document().expect_throw("document is undefined")
            .get_element_by_id(&ctx.props().element_id())
            .map(|e| e.scroll_into_view());
    }

    fn view_expanded_entry(&self, ctx: &Context<Self>, b: &mut BinViewBuilder) -> Html {
        match self.entry.as_ref() {
            Some(entry) =>
                html! {
                    <ul>
                        { for entry.fields.iter().map(|v| view_binfield(ctx, b, v)) }
                    </ul>
                },
            None => html! {},
        }
    }
}


mod binview {
    use yew::{classes, html, Callback, Context, Html};
    use yew::events::MouseEvent;
    use web_sys::Element;
    use wasm_bindgen::JsCast;
    use cdragon_prop::*;
    use super::{ResultEntry, Msg, ModelMsg};
    use crate::settings;

    /// Toggle a header's `collapsed` class, to be used in callbacks
    fn header_toggle_collapse(e: MouseEvent) {
        let this: Option<Element> = e.current_target().and_then(|e| e.dyn_into::<Element>().ok());
        this.map(|e| {
            let classes = e.class_list();
            if classes.contains("collapsed") {
                classes.remove_1("collapsed").ok();
            } else {
                classes.add_1("collapsed").ok();
            };
        });
    }


    pub struct BinViewBuilder<'a> {
        hash_mappers: &'a BinHashMappers,
    }

    impl<'a> BinViewBuilder<'a> {
        pub fn new(m: &'a BinHashMappers) -> Self {
            Self { hash_mappers: m }
        }

        pub fn format_entry_path(&self, h: BinEntryPath) -> String {
            match h.get_str(&self.hash_mappers) {
                Some(s) => s.to_string(),
                _ => format!("{{{:x}}}", h),
            }
        }

        pub fn format_type_name(&self, h: BinClassName) -> String {
            match h.get_str(&self.hash_mappers) {
                Some(s) => s.to_string(),
                _ => format!("{{{:x}}}", h),
            }
        }

        pub fn format_field_name(&self, h: BinFieldName) -> String {
            match h.get_str(&self.hash_mappers) {
                Some(s) => s.to_string(),
                _ => format!("{{{:x}}}", h),
            }
        }

        pub fn format_hash_value(&self, h: BinHashValue) -> String {
            match h.get_str(&self.hash_mappers) {
                Some(s) => s.to_string(),
                _ => format!("{{{:x}}}", h),
            }
        }

        pub fn format_path_value(&self, h: BinPathValue) -> String {
            match h.get_str(&self.hash_mappers) {
                Some(s) => s.to_string(),
                _ => format!("{{{:x}}}", h),
            }
        }
    }


    trait BinViewable {
        const NESTED: bool = false;

        fn view_value(&self, _ctx: &Context<ResultEntry>, b: &mut BinViewBuilder) -> Html;

        fn view_field_value(&self, ctx: &Context<ResultEntry>, b: &mut BinViewBuilder) -> Html {
            self.view_value(ctx, b)
        }

        fn view_type(&self, b: &BinViewBuilder) -> Html;
    }

    fn basic_bintype_name(btype: BinType) -> &'static str {
        match btype {
            BinType::None => "NONE",
            BinType::Bool => "BOOL",
            BinType::S8 => "S8",
            BinType::U8 => "U8",
            BinType::S16 => "S16",
            BinType::U16 => "U16",
            BinType::S32 => "S32",
            BinType::U32 => "U32",
            BinType::S64 => "S64",
            BinType::U64 => "U64",
            BinType::Float => "FLOAT",
            BinType::Vec2 => "VEC2",
            BinType::Vec3 => "VEC3",
            BinType::Vec4 => "VEC4",
            BinType::Matrix => "MATRIX",
            BinType::Color => "COLOR",
            BinType::String => "STRING",
            BinType::Hash => "HASH",
            BinType::Path => "PATH",
            BinType::Struct => "STRUCT",
            BinType::Embed => "EMBED",
            BinType::Link => "LINK",
            BinType::Flag => "FLAG",
            _ => panic!("basic BinType name should not be needed for non-nestable types"),
        }
    }


    pub fn view_binfield(ctx: &Context<ResultEntry>, b: &mut BinViewBuilder, field: &BinField) -> Html {
        let (v_nested, v_type, v_value) = binvalue_map_type!(field.vtype, T, {
            let v = field.downcast::<T>().unwrap();
            (T::NESTED, v.view_type(b), v.view_field_value(ctx, b))
        });

        let fname = html! { <span class="bin-field-name">{ b.format_field_name(field.name) }</span> };
        let ftype = html! { <span class="bin-field-type">{ v_type }</span> };
        let (v_header, v_value) = if v_nested {
            (html! {
                <div class={classes!("bin-field-header", "bin-item-header")}
                     onclick={Callback::from(header_toggle_collapse)}>
                    { fname } { ftype }
                </div>
            }, v_value)
        } else {
            (html! {
                <div class={classes!("bin-field-header", "bin-item-leaf")}>
                    { fname } { ftype } { v_value }
                </div>
            }, html! {})
        };

        html! {
            <li>
                <div class="bin-field">
                    { v_header } { v_value }
                </div>
            </li>
        }
    }

    macro_rules! impl_viewable {
        ($t:ty, $btype:expr, ($self:ident, $ctx:ident, $b:ident) => $e:expr) => {
            impl BinViewable for $t {
                fn view_value(&self, ctx: &Context<ResultEntry>, b: &mut BinViewBuilder) -> Html {
                    let ($self, $ctx, $b) = (self, ctx, b);
                    $e.into()
                }

                fn view_type(&self, _b: &BinViewBuilder) -> Html {
                    basic_bintype_name($btype).into()
                }
            }
        };
        ($t:ty, $b:expr) => { impl_viewable!($t, $b, (this, _ctx, _b) => this.0); };
        ($t:ty, $b:expr, $self:ident => $e:expr) => { impl_viewable!($t, $b, ($self, _ctx, _b) => $e); };
    }

    impl_viewable!(BinNone, BinType::None, _this => &"-");
    impl_viewable!(BinBool, BinType::Bool);
    impl_viewable!(BinS8, BinType::S8);
    impl_viewable!(BinU8, BinType::U8);
    impl_viewable!(BinS16, BinType::S16);
    impl_viewable!(BinU16, BinType::U16);
    impl_viewable!(BinS32, BinType::S32);
    impl_viewable!(BinU32, BinType::U32);
    impl_viewable!(BinS64, BinType::S64);
    impl_viewable!(BinU64, BinType::U64);
    impl_viewable!(BinFloat, BinType::Float);
    impl_viewable!(BinVec2, BinType::Vec2, this => format!("({}, {})", this.0, this.1));
    impl_viewable!(BinVec3, BinType::Vec3, this => format!("({}, {}, {})", this.0, this.1, this.2));
    impl_viewable!(BinVec4, BinType::Vec4, this => format!("({}, {}, {}, {})", this.0, this.1, this.2, this.3));
    impl_viewable!(BinMatrix, BinType::Matrix, this => format!(
        "(({}, {}, {}, {}), ({}, {}, {}, {}), ({}, {}, {}, {}), ({}, {}, {}, {}))",
        this.0[0][0], this.0[0][1], this.0[0][2], this.0[0][3],
        this.0[1][0], this.0[1][1], this.0[1][2], this.0[1][3],
        this.0[2][0], this.0[2][1], this.0[2][2], this.0[2][3],
        this.0[3][0], this.0[3][1], this.0[3][2], this.0[3][3]));
    impl_viewable!(BinColor, BinType::Color, this => format!("({}, {}, {}, {})", this.r, this.g, this.b, this.a));
    impl_viewable!(BinString, BinType::String, (this, _ctx, _b) => {
        let this = &this.0;
        if this.ends_with(".dds") || this.ends_with(".DDS") {
            let path = this[..this.len()-4].to_lowercase();
            let url = format!("{}/{}.png", settings::ASSETS_BASE_URL, path);
            html! {
                <a href={url.clone()} class="tooltipped">{ this }<br/><img src={url} /></a>
            }
        } else {
            this.into()
        }
    });
    impl_viewable!(BinHash, BinType::Hash, (this, _ctx, b) => html! {
        <span class="bin-hash-value">{ b.format_hash_value(this.0) }</span>
    });
    impl_viewable!(BinPath, BinType::Hash, (this, _ctx, b) => html! {
        <span class="bin-path-value">{ b.format_path_value(this.0) }</span>
    });

    impl BinViewable for BinList {
        const NESTED: bool = true;

        fn view_value(&self, ctx: &Context<ResultEntry>, b: &mut BinViewBuilder) -> Html {
            let v_values = binvalue_map_type!(
                self.vtype, T, view_vec_values(ctx, b, self.downcast::<T>().unwrap()));
            html! { <div class="bin-option">{ v_values }</div> }
        }

        fn view_type(&self, _b: &BinViewBuilder) -> Html {
            format!("CONTAINER({})", basic_bintype_name(self.vtype)).into()
        }
    }

    impl BinViewable for BinStruct {
        const NESTED: bool = true;

        fn view_value(&self, ctx: &Context<ResultEntry>, b: &mut BinViewBuilder) -> Html {
            html! {
                <div class="bin-struct">
                    <div class={classes!("bin-struct-header", "bin-item-header")}
                         onclick={Callback::from(header_toggle_collapse)}>
                        <span class="bin-struct-type">
                            { b.format_type_name(self.ctype) }
                        </span>
                    </div>
                    <ul>
                        { for self.fields.iter().map(|v| view_binfield(ctx, b, v)) }
                    </ul>
                </div>
            }
        }

        fn view_field_value(&self, ctx: &Context<ResultEntry>, b: &mut BinViewBuilder) -> Html {
            html! {
                <div class="bin-struct">
                    <ul>
                        { for self.fields.iter().map(|v| view_binfield(ctx, b, v)) }
                    </ul>
                </div>
            }
        }

        fn view_type(&self, b: &BinViewBuilder) -> Html {
            html! { <span class="bin-struct-type">{ b.format_type_name(self.ctype) }</span> }
        }
    }

    impl BinViewable for BinEmbed {
        const NESTED: bool = true;

        fn view_value(&self, ctx: &Context<ResultEntry>, b: &mut BinViewBuilder) -> Html {
            html! {
                <div class="bin-struct">
                    <div class={classes!("bin-struct-header", "bin-item-header")}
                         onclick={Callback::from(header_toggle_collapse)}>
                        <span class="bin-struct-type">
                            { b.format_type_name(self.ctype) }
                        </span>
                    </div>
                    <ul>
                        { for self.fields.iter().map(|v| view_binfield(ctx, b, v)) }
                    </ul>
                </div>
            }
        }

        fn view_field_value(&self, ctx: &Context<ResultEntry>, b: &mut BinViewBuilder) -> Html {
            html! {
                <div class="bin-struct">
                    <ul>
                        { for self.fields.iter().map(|v| view_binfield(ctx, b, v)) }
                    </ul>
                </div>
            }
        }

        fn view_type(&self, b: &BinViewBuilder) -> Html {
            html! { <span class="bin-struct-type">{ b.format_type_name(self.ctype) }</span> }
        }
    }

    impl_viewable!(BinLink, BinType::Link, (this, ctx, b) => {
        let path = this.0;
        let onclick = ctx.link().callback(move |_| Msg::Forward(ModelMsg::GoToEntry(path)));
        html! {
            <span class="bin-link-value" {onclick}>{ b.format_entry_path(path) }</span>
        }
    });

    impl BinViewable for BinOption {
        const NESTED: bool = true;

        fn view_value(&self, ctx: &Context<ResultEntry>, b: &mut BinViewBuilder) -> Html {
            match self.value {
                None => "-".into(),
                Some(_) => {
                    let v_value = binvalue_map_type!(
                        self.vtype, T, self.downcast::<T>().unwrap().view_value(ctx, b));
                    html! { <div class="bin-option">{ v_value }</div> }
                }
            }
        }

        fn view_type(&self, _b: &BinViewBuilder) -> Html {
            format!("OPTION({})", basic_bintype_name(self.vtype)).into()
        }
    }

    impl BinViewable for BinMap {
        const NESTED: bool = true;

        fn view_value(&self, ctx: &Context<ResultEntry>, b: &mut BinViewBuilder) -> Html {
            let v_values = binvalue_map_keytype!(
                self.ktype, K, binvalue_map_type!(
                    self.vtype, V, view_binvalue_map(ctx, b, self.downcast::<K, V>().unwrap())
                    ));
            html! { <div class="bin-map">{ v_values }</div> }
        }

        fn view_type(&self, _b: &BinViewBuilder) -> Html {
            format!("MAP({},{})", basic_bintype_name(self.ktype), basic_bintype_name(self.vtype)).into()
        }
    }

    fn view_vec_values<T: BinViewable>(ctx: &Context<ResultEntry>, b: &mut BinViewBuilder, values: &[T]) -> Html {
        html! {
            <ul>
                { for values.iter().map(|v| html! { <li>{ v.view_value(ctx, b) }</li> }) }
            </ul>
        }
    }

    fn view_binvalue_map<K: BinViewable, V: BinViewable>(ctx: &Context<ResultEntry>, b: &mut BinViewBuilder, values: &Vec<(K, V)>) -> Html {
        html! {
            <ul>
                { for values.iter().map(|(k, v)| html! {
                    <li>
                        <span class="bin-map-item">
                            { k.view_value(ctx, b) }
                            { " => " }
                            { v.view_value(ctx, b) }
                        </span>
                    </li>
                }) }
            </ul>
        }
    }

    impl_viewable!(BinFlag, BinType::Flag);
}

