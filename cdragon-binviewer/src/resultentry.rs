use std::fmt;
use log::{debug, error};
use yew::prelude::*;
use yew::{html, Component, ComponentLink, Html, Renderable, ShouldRender};
use yew::callback::Callback;
use stdweb::js;
use stdweb::web::INonElementParentNode;
use cdragon_prop::{
    BinEntryPath,
    BinClassName,
    BinEntry,
};
use super::SharedStateRef;
use crate::binloadservice::BinFetchTask;
use binview::{BinViewBuilder, view_binfield};
use super::Msg as ModelMsg;
use crate::Result;


pub struct ResultEntry {
    state: SharedStateRef,
    hpath: BinEntryPath,
    htype: BinClassName,
    entry: Option<BinEntry>,
    entry_task: Option<BinFetchTask>,
    send_model: Callback<ModelMsg>,
    link: ComponentLink<ResultEntry>,
}

pub enum Msg {
    ToggleCollapse,
    SetEntry(BinEntry),
    Forward(ModelMsg),
    Ignore,
}

impl fmt::Debug for Msg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Msg::ToggleCollapse => write!(f, "ToggleCollapse"),
            Msg::SetEntry(_) => write!(f, "SetEntry(entry)"),
            Msg::Forward(m) => write!(f, "Forward({:?})", m),
            Msg::Ignore => write!(f, "Ignore"),
        }
    }
}


#[derive(Properties)]
pub struct Props {
    #[props(required)]
    pub state: SharedStateRef,
    #[props(required)]
    pub hpath: BinEntryPath,
    #[props(required)]
    pub htype: BinClassName,
    #[props(required)]
    pub send_model: Callback<ModelMsg>,
    /// Force expand and scroll into view if true, do nothing if false
    pub select: bool,
}

impl Component for ResultEntry {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let mut entry = ResultEntry {
            state: props.state,
            hpath: props.hpath,
            htype: props.htype,
            entry: None,
            entry_task: None,
            send_model: props.send_model,
            link,
        };
        if props.select {
            entry.select_entry(); //TODO
        }
        entry
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        debug!("ResultEntry message: {:?}", msg);
        match msg {
            Msg::ToggleCollapse => {
                if self.is_collapsed() {
                    self.expand_entry();
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
                self.send_model.emit(m);
                false
            }

            Msg::Ignore => false
        }
    }

    fn change(&mut self, props: Self::Properties) -> ShouldRender {
        if self.hpath != props.hpath {
            self.entry = None;
        }
        self.state = props.state;
        self.hpath = props.hpath;
        self.htype = props.htype;
        self.send_model = props.send_model;

        if props.select {
            self.select_entry(); //TODO
        }
        true
    }
}

impl Renderable<ResultEntry> for ResultEntry {
    fn view(&self) -> Html<Self> {
        let mappers = &self.state.as_ref().borrow().hash_mappers;
        let mut b = BinViewBuilder::new(mappers);
        let item_class = if self.entry.is_none() { "collapsed" } else { "" };
        let onclick_htype = self.htype.clone();
        html! {
            <li>
                <div class="bin-entry" id=self.element_id()>
                    <div class=("bin-entry-header", "bin-item-header", item_class)
                         onclick=|_| Msg::ToggleCollapse>
                        <span class="bin-entry-path">
                            { b.format_entry_path(self.hpath) }
                        </span>
                        <span class="bin-entry-type"
                              onclick=|_| Msg::Forward(ModelMsg::FilterEntryType(onclick_htype))>
                            { b.format_type_name(self.htype) }
                        </span>
                    </div>
                    { self.view_expanded_entry(&mut b) }
                </div>
            </li>
        }
    }
}

impl ResultEntry {
    /// Return the HTML ID of the "main" element
    fn element_id(&self) -> String {
        format!("entry-{:x}", self.hpath)
    }

    fn is_collapsed(&self) -> bool {
        !self.entry.is_some()
    }

    fn collapse_entry(&mut self) {
        self.entry = None;
    }

    fn expand_entry(&mut self) {
        if self.entry.is_some() {
            return;  // already expanded
        }
        let callback = self.link.send_back(move |result: Result<BinEntry>| {
            match result {
                Ok(entry) => Msg::SetEntry(entry),
                Err(e) => { error!("failed to load bin entry: {}", e); Msg::Ignore }
            }
        });
        let file = get_state!(self).entrydb.get_entry_file(self.hpath).unwrap().to_string();
        let task = get_binload_service!(self).fetch_entry(&file, self.hpath, callback);
        self.entry_task = Some(task);
    }

    /// Expand entry and scroll to it
    fn select_entry(&mut self) {
        self.expand_entry();
        stdweb::web::document().get_element_by_id(&self.element_id())
            .map(|e| js! { @{e}.scrollIntoView(); });
    }

    fn view_expanded_entry(&self, b: &mut BinViewBuilder) -> Html<Self> {
        match self.entry.as_ref() {
            Some(entry) =>
                html! {
                    <ul>
                        { for entry.fields.iter().map(|v| view_binfield(b, v)) }
                    </ul>
                },
            None => html! {},
        }
    }
}


mod binview {
    use yew::{html, Html};
    use cdragon_prop::*;
    use super::{ResultEntry, Msg, ModelMsg};
    use stdweb::unstable::TryInto;
    use stdweb::web::{
        event::IEvent,
        IElement,
        Element,
    };
    use crate::settings;

    /// Toggle a header's `collapsed` class, to be used in callbacks
    fn header_toggle_collapse(e: impl IEvent) -> Msg {
        let this: Option<Element> = e.current_target().and_then(|e| e.try_into().ok());
        this.map(|e| {
            let classes = e.class_list();
            if classes.contains("collapsed") {
                classes.remove("collapsed").ok();
            } else {
                classes.add("collapsed").ok();
            };
        });
        Msg::Ignore
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

        fn view_value(&self, b: &mut BinViewBuilder) -> Html<ResultEntry>;

        fn view_field_value(&self, b: &mut BinViewBuilder) -> Html<ResultEntry> {
            self.view_value(b)
        }

        fn view_type(&self, b: &BinViewBuilder) -> Html<ResultEntry>;
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


    pub fn view_binfield(b: &mut BinViewBuilder, field: &BinField) -> Html<ResultEntry> {
        let (v_nested, v_type, v_value) = binvalue_map_type!(field.vtype, T, {
            let v = field.downcast::<T>().unwrap();
            (T::NESTED, v.view_type(b), v.view_field_value(b))
        });

        let fname = html! { <span class="bin-field-name">{ b.format_field_name(field.name) }</span> };
        let ftype = html! { <span class="bin-field-type">{ v_type }</span> };
        let (v_header, v_value) = if v_nested {
            (html! {
                <div class=("bin-field-header", "bin-item-header")
                     onclick=|e| header_toggle_collapse(e)>
                    { fname } { ftype }
                </div>
            }, v_value)
        } else {
            (html! {
                <div class=("bin-field-header", "bin-item-leaf")>
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
        ($t:ty, $btype:expr, ($self:ident, $b:ident) => $e:expr) => {
            impl BinViewable for $t {
                fn view_value(&self, b: &mut BinViewBuilder) -> Html<ResultEntry> {
                    let ($self, $b) = (self, b);
                    $e.into()
                }

                fn view_type(&self, _b: &BinViewBuilder) -> Html<ResultEntry> {
                    basic_bintype_name($btype).into()
                }
            }
        };
        ($t:ty, $b:expr) => { impl_viewable!($t, $b, (this, _b) => this.0); };
        ($t:ty, $b:expr, $self:ident => $e:expr) => { impl_viewable!($t, $b, ($self, _b) => $e); };
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
    impl_viewable!(BinString, BinType::String, (this, _b) => {
        let this = &this.0;
        if this.ends_with(".dds") || this.ends_with(".DDS") {
            let path = this[..this.len()-4].to_lowercase();
            let url = format!("{}/{}.png", settings::ASSETS_BASE_URL, path);
            html! {
                <a href=url.as_str() class="tooltipped">{ this }<br/><img src=&url /></a>
            }
        } else {
            this.into()
        }
    });
    impl_viewable!(BinHash, BinType::Hash, (this, b) => html! {
        <span class="bin-hash-value">{ b.format_hash_value(this.0) }</span>
    });
    impl_viewable!(BinPath, BinType::Hash, (this, b) => html! {
        <span class="bin-path-value">{ b.format_path_value(this.0) }</span>
    });

    impl BinViewable for BinList {
        const NESTED: bool = true;

        fn view_value(&self, b: &mut BinViewBuilder) -> Html<ResultEntry> {
            let v_values = binvalue_map_type!(
                self.vtype, T, view_vec_values(b, self.downcast::<T>().unwrap()));
            html! { <div class="bin-option">{ v_values }</div> }
        }

        fn view_type(&self, _b: &BinViewBuilder) -> Html<ResultEntry> {
            format!("CONTAINER({})", basic_bintype_name(self.vtype)).into()
        }
    }

    impl BinViewable for BinStruct {
        const NESTED: bool = true;

        fn view_value(&self, b: &mut BinViewBuilder) -> Html<ResultEntry> {
            html! {
                <div class="bin-struct">
                    <div class=("bin-struct-header", "bin-item-header")
                         onclick=|e| header_toggle_collapse(e)>
                        <span class="bin-struct-type">
                            { b.format_type_name(self.ctype) }
                        </span>
                    </div>
                    <ul>
                        { for self.fields.iter().map(|v| view_binfield(b, v)) }
                    </ul>
                </div>
            }
        }

        fn view_field_value(&self, b: &mut BinViewBuilder) -> Html<ResultEntry> {
            html! {
                <div class="bin-struct">
                    <ul>
                        { for self.fields.iter().map(|v| view_binfield(b, v)) }
                    </ul>
                </div>
            }
        }

        fn view_type(&self, b: &BinViewBuilder) -> Html<ResultEntry> {
            html! { <span class="bin-struct-type">{ b.format_type_name(self.ctype) }</span> }
        }
    }

    impl BinViewable for BinEmbed {
        const NESTED: bool = true;

        fn view_value(&self, b: &mut BinViewBuilder) -> Html<ResultEntry> {
            html! {
                <div class="bin-struct">
                    <div class=("bin-struct-header", "bin-item-header")
                         onclick=|e| header_toggle_collapse(e)>
                        <span class="bin-struct-type">
                            { b.format_type_name(self.ctype) }
                        </span>
                    </div>
                    <ul>
                        { for self.fields.iter().map(|v| view_binfield(b, v)) }
                    </ul>
                </div>
            }
        }

        fn view_field_value(&self, b: &mut BinViewBuilder) -> Html<ResultEntry> {
            html! {
                <div class="bin-struct">
                    <ul>
                        { for self.fields.iter().map(|v| view_binfield(b, v)) }
                    </ul>
                </div>
            }
        }

        fn view_type(&self, b: &BinViewBuilder) -> Html<ResultEntry> {
            html! { <span class="bin-struct-type">{ b.format_type_name(self.ctype) }</span> }
        }
    }

    impl_viewable!(BinLink, BinType::Link, (this, b) => {
        let path = this.0.clone();
        html! {
            <span class="bin-link-value" onclick=|_| Msg::Forward(ModelMsg::GoToEntry(path))>{ b.format_entry_path(path) }</span>
        }
    });

    impl BinViewable for BinOption {
        const NESTED: bool = true;

        fn view_value(&self, b: &mut BinViewBuilder) -> Html<ResultEntry> {
            match self.value {
                None => "-".into(),
                Some(_) => {
                    let v_value = binvalue_map_type!(
                        self.vtype, T, self.downcast::<T>().unwrap().view_value(b));
                    html! { <div class="bin-option">{ v_value }</div> }
                }
            }
        }

        fn view_type(&self, _b: &BinViewBuilder) -> Html<ResultEntry> {
            format!("OPTION({})", basic_bintype_name(self.vtype)).into()
        }
    }

    impl BinViewable for BinMap {
        const NESTED: bool = true;

        fn view_value(&self, b: &mut BinViewBuilder) -> Html<ResultEntry> {
            let v_values = binvalue_map_keytype!(
                self.ktype, K, binvalue_map_type!(
                    self.vtype, V, view_binvalue_map(b, self.downcast::<K, V>().unwrap())
                    ));
            html! { <div class="bin-map">{ v_values }</div> }
        }

        fn view_type(&self, _b: &BinViewBuilder) -> Html<ResultEntry> {
            format!("MAP({},{})", basic_bintype_name(self.ktype), basic_bintype_name(self.vtype)).into()
        }
    }

    fn view_vec_values<T: BinViewable>(b: &mut BinViewBuilder, values: &[T]) -> Html<ResultEntry> {
        html! {
            <ul>
                { for values.iter().map(|v| html! { <li>{ v.view_value(b) }</li> }) }
            </ul>
        }
    }

    fn view_binvalue_map<K: BinViewable, V: BinViewable>(b: &mut BinViewBuilder, values: &Vec<(K, V)>) -> Html<ResultEntry> {
        html! {
            <ul>
                { for values.iter().map(|(k, v)| html! {
                    <li>
                        <span class="bin-map-item">
                            { k.view_value(b) }
                            { " => " }
                            { v.view_value(b) }
                        </span>
                    </li>
                }) }
            </ul>
        }
    }

    impl_viewable!(BinFlag, BinType::Flag);
}

