use yew::prelude::*;
use yew::events::MouseEvent;
use web_sys::Element;
use wasm_bindgen::JsCast;
use cdragon_prop::*;
use crate::{
    settings,
    utils::*,
};

/// Toggle a header's `collapsed` class, to be used in callbacks
fn header_toggle_collapse(e: MouseEvent) {
    if let Some(e) = e.target().and_then(|e| e.dyn_into::<Element>().ok()) {
        let classes = e.class_list();
        if classes.contains("closed") {
            classes.remove_1("closed").ok();
        } else {
            classes.add_1("closed").ok();
        };
    }
}


pub struct BinViewBuilder<'a> {
    hash_mappers: &'a BinHashMappers,
    on_link_click: Callback<BinEntryPath>,
    assets_base_url: String,
}

impl<'a> BinViewBuilder<'a> {
    pub fn new(hmappers: &'a BinHashMappers, on_link_click: Callback<BinEntryPath>) -> Self {
        Self {
            hash_mappers: hmappers,
            on_link_click,
            assets_base_url: settings::assets_base_url(),
        }
    }

    pub fn format_entry_path(&self, h: BinEntryPath) -> String {
        match h.get_str(self.hash_mappers) {
            Some(s) => s.to_string(),
            _ => format!("{{{:x}}}", h),
        }
    }

    pub fn format_type_name(&self, h: BinClassName) -> String {
        match h.get_str(self.hash_mappers) {
            Some(s) => s.to_string(),
            _ => format!("{{{:x}}}", h),
        }
    }

    pub fn format_field_name(&self, h: BinFieldName) -> String {
        match h.get_str(self.hash_mappers) {
            Some(s) => s.to_string(),
            _ => format!("{{{:x}}}", h),
        }
    }

    pub fn format_hash_value(&self, h: BinHashValue) -> String {
        match h.get_str(self.hash_mappers) {
            Some(s) => s.to_string(),
            _ => format!("{{{:x}}}", h),
        }
    }

    pub fn format_path_value(&self, h: BinPathValue) -> String {
        match h.get_str(self.hash_mappers) {
            Some(s) => s.to_string(),
            _ => format!("{{{:x}}}", h),
        }
    }
}


trait BinViewable {
    const NESTED: bool = false;

    fn view_value(&self, b: &mut BinViewBuilder) -> Html;

    fn view_field_value(&self, b: &mut BinViewBuilder) -> Html {
        self.view_value(b)
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


pub fn view_binfield(b: &mut BinViewBuilder, field: &BinField) -> Html {
    let (v_nested, v_type, v_value) = binvalue_map_type!(field.vtype, T, {
        let v = field.downcast::<T>().unwrap();
        (T::NESTED, v.view_type(b), v.view_field_value(b))
    });

    let fname = html! { <span class="bin-field-name">{ b.format_field_name(field.name) }</span> };
    let ftype = html! { <span class="bin-field-type">{ v_type }</span> };
    let (v_header, v_value) = if v_nested {
        (html! {
            <div class={classes!("bin-field-header", "bin-item-header")}
                 onclick={Callback::from(header_toggle_collapse)}>
                { fname }{" "}{ ftype }
            </div>
        }, v_value)
    } else {
        (html! {
            <div class={classes!("bin-field-header", "bin-item-leaf")}>
                { fname }{" "}{ ftype }{" "}{ v_value }
            </div>
        }, html! {})
    };

    html! {
        <li>
            <div class="bin-field">
                { v_header }{" "}{ v_value }
            </div>
        </li>
    }
}

macro_rules! impl_viewable {
    ($t:ty, $btype:expr, ($self:ident, $b:ident) => $e:expr) => {
        impl BinViewable for $t {
            fn view_value(&self, b: &mut BinViewBuilder) -> Html {
                let ($self, $b) = (self, b);
                $e.into()
            }

            fn view_type(&self, _b: &BinViewBuilder) -> Html {
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
impl_viewable!(BinColor, BinType::Color, this => {
    let s = format!("({}, {}, {}, {})", this.r, this.g, this.b, this.a);
    let style = format!("background-color: rgb({}, {}, {})", this.r, this.g, this.b);
    html! {
        <span><span class="bin-color-value-preview" {style}></span>{ s }</span>
    }
});
impl_viewable!(BinString, BinType::String, (this, b) => {
    let this = &this.0;
    if this.ends_with(".dds") || this.ends_with(".DDS") || this.ends_with(".tex") {
        let path = this[..this.len()-4].to_lowercase();
        let url = format!("{}/{}.png", b.assets_base_url, path);
        html! {
            <a href={url.clone()} class="tooltipped">{ this }<br/><img src={url} /></a>
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

    fn view_value(&self, b: &mut BinViewBuilder) -> Html {
        let v_values = binvalue_map_type!(
            self.vtype, T, view_vec_values(b, self.downcast::<T>().unwrap()));
        html! { <div class="bin-option">{ v_values }</div> }
    }

    fn view_type(&self, _b: &BinViewBuilder) -> Html {
        html! {
            <span>
                <span class="bin-container-type">{ "list" }</span>
                {" "}
                <span class="bin-struct-type">{ basic_bintype_name(self.vtype) }</span>
            </span>
        }
    }
}

impl BinViewable for BinStruct {
    const NESTED: bool = true;

    fn view_value(&self, b: &mut BinViewBuilder) -> Html {
        html! {
            <div class="bin-struct">
                <div class={classes!("bin-struct-header", "bin-item-header")}
                     onclick={Callback::from(header_toggle_collapse)}>
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

    fn view_field_value(&self, b: &mut BinViewBuilder) -> Html {
        html! {
            <div class="bin-struct">
                <ul>
                    { for self.fields.iter().map(|v| view_binfield(b, v)) }
                </ul>
            </div>
        }
    }

    fn view_type(&self, b: &BinViewBuilder) -> Html {
        html! {
            <span>
                <span class="bin-container-type">{ "struct" }</span>
                {" "}
                <span class="bin-struct-type">{ b.format_type_name(self.ctype) }</span>
            </span>
        }
    }
}

impl BinViewable for BinEmbed {
    const NESTED: bool = true;

    fn view_value(&self, b: &mut BinViewBuilder) -> Html {
        html! {
            <div class="bin-struct">
                <div class={classes!("bin-struct-header", "bin-item-header")}
                     onclick={Callback::from(header_toggle_collapse)}>
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

    fn view_field_value(&self, b: &mut BinViewBuilder) -> Html {
        html! {
            <div class="bin-struct">
                <ul>
                    { for self.fields.iter().map(|v| view_binfield(b, v)) }
                </ul>
            </div>
        }
    }

    fn view_type(&self, b: &BinViewBuilder) -> Html {
        html! {
            <span>
                <span class="bin-container-type">{ "embed" }</span>
                {" "}
                <span class="bin-struct-type">{ b.format_type_name(self.ctype) }</span>
            </span>
        }
    }
}

impl_viewable!(BinLink, BinType::Link, (this, b) => {
    let path = this.0;
    let onclick = handle_normal_click(b.on_link_click.reform(move |_| path));
    let hstr = path.seek_str(b.hash_mappers);
    let href = build_app_url(&format!("{}", hstr), Some(path));
    html! {
        <a class="bin-link-value" {href} {onclick}>{ b.format_entry_path(path) }</a>
    }
});

impl BinViewable for BinOption {
    const NESTED: bool = true;

    fn view_value(&self, b: &mut BinViewBuilder) -> Html {
        if self.is_some() {
            let v_value = binvalue_map_type!(
                self.vtype, T, self.downcast::<T>().unwrap().view_value(b));
            html! { <div class="bin-option">{ v_value }</div> }
        } else {
            "-".into()
        }
    }

    fn view_type(&self, _b: &BinViewBuilder) -> Html {
        html! {
            <span>
                <span class="bin-container-type">{ "option" }</span>
                {" "}
                <span class="bin-inner-type">{ basic_bintype_name(self.vtype) }</span>
            </span>
        }
    }
}

impl BinViewable for BinMap {
    const NESTED: bool = true;

    fn view_value(&self, b: &mut BinViewBuilder) -> Html {
        let v_values = binvalue_map_keytype!(
            self.ktype, K, binvalue_map_type!(
                self.vtype, V, view_binvalue_map(b, self.downcast::<K, V>().unwrap())
                ));
        html! { <div class="bin-map">{ v_values }</div> }
    }

    fn view_type(&self, _b: &BinViewBuilder) -> Html {
        html! {
            <span>
                <span class="bin-container-type">{ "map" }</span>
                {" "}
                <span class="bin-inner-type">{ basic_bintype_name(self.ktype) }</span>
                <span>{ "," }</span>
                <span class="bin-inner-type">{ basic_bintype_name(self.vtype) }</span>
            </span>
        }
    }
}

fn view_vec_values<T: BinViewable>(b: &mut BinViewBuilder, values: &[T]) -> Html {
    html! {
        <ul>
            { for values.iter().map(|v| html! { <li>{ v.view_value(b) }</li> }) }
        </ul>
    }
}

fn view_binvalue_map<K: BinViewable, V: BinViewable>(b: &mut BinViewBuilder, values: &[(K, V)]) -> Html {
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

