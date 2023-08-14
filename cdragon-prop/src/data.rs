//! Bin data definitions
use std::any::Any;
use num_enum::TryFromPrimitive;
use super::BinHashMappers;
use cdragon_hashes::{
    define_hash_type,
    HashOrStr,
    bin::{BinHashKind, compute_binhash},
    wad::compute_wad_hash,
};
pub use cdragon_hashes::bin::BinHashMapper;


/// Field value for an antry, a struct or an embed
pub struct BinField {
    /// Field name (hashed)
    pub name: BinFieldName,
    /// Field value type
    pub vtype: BinType,
    pub(crate) value: Box<dyn Any>,  // Any = vtype
}

impl BinField {
    /// Downcast the field value
    pub fn downcast<T: BinValue + 'static>(&self) -> Option<&T> {
        self.value.downcast_ref::<T>()
    }
}


/// Declare a bin hash type
macro_rules! declare_bin_hash {
    (
        $(#[$meta:meta])*
        $name:ident => $kind:expr
    ) => {
        define_hash_type! {
            $(#[$meta])*
            $name(u32) => compute_binhash
        }

        impl $name {
            /// Hash kind, for use with [BinHashMappers]
            const KIND: BinHashKind = $kind;
            /// Get the string associated to the hash
            pub fn get_str<'a>(&self, mapper: &'a BinHashMappers) -> Option<&'a str> {
                mapper.get(Self::KIND).get(self.hash)
            }
            /// Get the string associated to the hash or fallback to the hash itself
            pub fn seek_str<'a>(&self, mapper: &'a BinHashMappers) -> HashOrStr<u32, &'a str> {
                mapper.get(Self::KIND).seek(self.hash)
            }
        }
    }
}

declare_bin_hash! {
    /// Hash of a [BinEntry] path
    BinEntryPath => BinHashKind::EntryPath
}
declare_bin_hash! {
    /// Hash of a bin class name (type of [entries](BinEntry), [structs](BinStruct) and
    /// [embeds](BinEmbed))
    BinClassName => BinHashKind::ClassName
}
declare_bin_hash! {
    /// Hash of a field name of bin class
    BinFieldName => BinHashKind::FieldName
}
declare_bin_hash! {
    /// Hash of a [BinHash] value
    BinHashValue => BinHashKind::HashValue
}

define_hash_type! {
    /// Hash of a [BinPath] value, put to a file in a [cdragon_wad::Wad] archive
    BinPathValue(u64) => compute_wad_hash
}
impl BinPathValue {
    /// Get the path associated to the hash
    pub fn get_str<'a>(&self, mapper: &'a BinHashMappers) -> Option<&'a str> {
        mapper.path_value.get(self.hash)
    }
    /// Get the path associated to the hash or fallback to the hash itself
    pub fn seek_str<'a>(&self, mapper: &'a BinHashMappers) -> HashOrStr<u64, &'a str> {
        mapper.path_value.seek(self.hash)
    }
}


/// Trait for values enumerated in [BinType]
pub trait BinValue {
    /// Bin type associated to the value
    const TYPE: BinType;
}

macro_rules! declare_bintype_struct {
    ($type:ident ($t:ty) [$($d:ident),* $(,)?]) => {
        #[allow(missing_docs)]
        #[derive(Debug,$($d),*)]
        pub struct $type(pub $t);
        impl From<$t> for $type {
            fn from(v: $t) -> Self { Self(v) }
        }
    };
    ($type:ident ($($v:ident: $t:ty),* $(,)?)) => {
        #[allow(missing_docs)]
        #[derive(Debug)]
        pub struct $type($(pub $t,)*);
        impl From<($($t),*)> for $type {
            fn from(($($v),*): ($($t),*)) -> Self {
                Self($($v),*)
            }
        }
    };
}

declare_bintype_struct!{ BinNone() }
declare_bintype_struct!{ BinBool(bool) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinS8(i8) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinU8(u8) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinS16(i16) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinU16(u16) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinS32(i32) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinU32(u32) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinS64(i64) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinU64(u64) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinFloat(f32) [] }
declare_bintype_struct!{ BinVec2(a: f32, b: f32) }
declare_bintype_struct!{ BinVec3(a: f32, b: f32, c: f32) }
declare_bintype_struct!{ BinVec4(a: f32, b: f32, c: f32, d: f32) }
declare_bintype_struct!{ BinMatrix([[f32; 4]; 4]) [] }
/// Color bin value (RGBA)
#[allow(missing_docs)]
#[derive(Debug)]
pub struct BinColor { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }
declare_bintype_struct!{ BinString(String) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinHash(BinHashValue) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinPath(BinPathValue) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinLink(BinEntryPath) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinFlag(bool) [Eq,PartialEq,Hash] }


/// List of values, variable size
///
/// This type is used for both [BinType::List] and [BinType::List2].
pub struct BinList {
    /// Type of values in the list
    pub vtype: BinType,
    pub(crate) values: Box<dyn Any>,  // Any = Vec<vtype>
}

impl BinList {
    /// Downcast the list to a vector
    pub fn downcast<T: BinValue + 'static>(&self) -> Option<&Vec<T>> {
        self.values.downcast_ref::<Vec<T>>()
    }
}

/// Bin structure, referenced by pointer
pub struct BinStruct {
    /// Class type of the struct
    pub ctype: BinClassName,
    /// Struct fields
    pub fields: Vec<BinField>,
}

impl BinStruct {
    /// Get a field by its name
    pub fn get(&self, name: BinFieldName) -> Option<&BinField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Get a field by its name and downcast it
    pub fn getv<T: BinValue + 'static>(&self, name: BinFieldName) -> Option<&T> {
        self.get(name).and_then(|field| field.downcast::<T>())
    }
}

/// Bin structure whose data is embedded directly
pub struct BinEmbed {
    /// Class type of the embed
    pub ctype: BinClassName,
    /// Embed fields
    pub fields: Vec<BinField>,
}

impl BinEmbed {
    /// Get a field by its name
    pub fn get(&self, name: BinFieldName) -> Option<&BinField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Get a field by its name and downcast it
    pub fn getv<T: BinValue + 'static>(&self, name: BinFieldName) -> Option<&T> {
        self.get(name).and_then(|field| field.downcast::<T>())
    }
}

/// Optional bin value
pub struct BinOption {
    /// Type of the value in the option
    pub vtype: BinType,
    pub(crate) value: Option<Box<dyn Any>>,  // Any = vtype
}

impl BinOption {
    /// Return `true` if the option contains a value
    pub fn is_some(&self) -> bool {
        self.value.is_some()
    }

    /// Downcast the option
    pub fn downcast<T: BinValue + 'static>(&self) -> Option<&T> {
        match self.value {
            Some(ref v) => Some(v.downcast_ref::<T>()?),
            None => None,
        }
    }
}


/// Map of values, with separate key and value types
pub struct BinMap {
    /// Type of map keys
    pub ktype: BinType,
    /// Type of map values
    pub vtype: BinType,
    pub(crate) values: Box<dyn Any>,  // Any = Vec<(ktype, vtype)>
}

impl BinMap {
    /// Downcast the map to a vector of `(key, value)` pairs
    pub fn downcast<K: BinValue + 'static, V: BinValue + 'static>(&self) -> Option<&Vec<(K, V)>> {
        self.values.downcast_ref::<Vec<(K, V)>>()
    }
}

impl BinValue for BinNone { const TYPE: BinType = BinType::None; }
impl BinValue for BinBool { const TYPE: BinType = BinType::Bool; }
impl BinValue for BinS8 { const TYPE: BinType = BinType::S8; }
impl BinValue for BinU8 { const TYPE: BinType = BinType::U8; }
impl BinValue for BinS16 { const TYPE: BinType = BinType::S16; }
impl BinValue for BinU16 { const TYPE: BinType = BinType::U16; }
impl BinValue for BinS32 { const TYPE: BinType = BinType::S32; }
impl BinValue for BinU32 { const TYPE: BinType = BinType::U32; }
impl BinValue for BinS64 { const TYPE: BinType = BinType::S64; }
impl BinValue for BinU64 { const TYPE: BinType = BinType::U64; }
impl BinValue for BinFloat { const TYPE: BinType = BinType::Float; }
impl BinValue for BinVec2 { const TYPE: BinType = BinType::Vec2; }
impl BinValue for BinVec3 { const TYPE: BinType = BinType::Vec3; }
impl BinValue for BinVec4 { const TYPE: BinType = BinType::Vec4; }
impl BinValue for BinMatrix { const TYPE: BinType = BinType::Matrix; }
impl BinValue for BinColor { const TYPE: BinType = BinType::Color; }
impl BinValue for BinString { const TYPE: BinType = BinType::String; }
impl BinValue for BinHash { const TYPE: BinType = BinType::Hash; }
impl BinValue for BinPath { const TYPE: BinType = BinType::Path; }
impl BinValue for BinList { const TYPE: BinType = BinType::List; }
impl BinValue for BinStruct { const TYPE: BinType = BinType::Struct; }
impl BinValue for BinEmbed { const TYPE: BinType = BinType::Embed; }
impl BinValue for BinLink { const TYPE: BinType = BinType::Link; }
impl BinValue for BinOption { const TYPE: BinType = BinType::Option; }
impl BinValue for BinMap { const TYPE: BinType = BinType::Map; }
impl BinValue for BinFlag { const TYPE: BinType = BinType::Flag; }


/// Basic bin types
///
/// Variant values match the binary values used in PROP files.
#[allow(dead_code, missing_docs)]
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, TryFromPrimitive, Debug)]
pub enum BinType {
    None = 0,
    Bool = 1,
    S8 = 2,
    U8 = 3,
    S16 = 4,
    U16 = 5,
    S32 = 6,
    U32 = 7,
    S64 = 8,
    U64 = 9,
    Float = 10,
    Vec2 = 11,
    Vec3 = 12,
    Vec4 = 13,
    Matrix = 14,
    Color = 15,
    String = 16,
    Hash = 17,
    Path = 18,  // introduced in 10.23
    // Complex types (shifted to 0x80+ in 9.23)
    List = 19,
    List2 = 20,  // handled as List, introduced in 10.8
    Struct = 21,
    Embed = 22,
    Link = 23,
    Option = 24,
    Map = 25,
    Flag = 26,
}

impl BinType {
    /// Return true for nested types
    #[inline]
    pub const fn is_nested(&self) -> bool {
        matches!(self,
            BinType::List |
            BinType::List2 |
            BinType::Struct |
            BinType::Embed |
            BinType::Option |
            BinType::Map)
    }
}

