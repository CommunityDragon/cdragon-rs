use std::any::Any;
use num_enum::TryFromPrimitive;
use super::{
    BinHashMappers,
    compute_binhash,
};
use cdragon_utils::{
    hashes::HashOrStr,
    define_hash_type,
};


/// Field value for an antry, a struct or an embed
pub struct BinField {
    pub name: BinFieldName,
    pub vtype: BinType,
    pub(crate) value: Box<dyn Any>,  // Any = vtype
}

impl BinField {
    pub fn downcast<T: BinValue + 'static>(&self) -> Option<&T> {
        self.value.downcast_ref::<T>()
    }
}

/// Enum with a variant for each kind of `BinHash`
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum BinHashKind {
    EntryPath,
    ClassName,
    FieldName,
    HashValue,
}

impl BinHashKind {
    /// Return filename used to store the mapping for a `BinHashKind`
    pub fn mapper_path(&self) -> &'static str {
        match self {
            Self::EntryPath => "hashes.binentries.txt",
            Self::ClassName => "hashes.bintypes.txt",
            Self::FieldName => "hashes.binfields.txt",
            Self::HashValue => "hashes.binhashes.txt",
        }
    }

    /// Iterate on variants
    pub fn variants() -> impl Iterator<Item=Self> {
        static VARIANTS: &[BinHashKind] = &[
            BinHashKind::EntryPath,
            BinHashKind::ClassName,
            BinHashKind::FieldName,
            BinHashKind::HashValue,
        ];
        VARIANTS.iter().copied()
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
            const KIND: BinHashKind = $kind;
            pub fn get_str<'a>(&self, mapper: &'a BinHashMappers) -> Option<&'a str> {
                mapper.get(Self::KIND).get(self.hash)
            }
            pub fn seek_str<'a>(&self, mapper: &'a BinHashMappers) -> HashOrStr<u32, &'a str> {
                mapper.get(Self::KIND).seek(self.hash)
            }
        }
    }
}

declare_bin_hash! {
    /// Hash of a bin entry path
    BinEntryPath => BinHashKind::EntryPath
}
declare_bin_hash! {
    /// Hash of a bin class name (used by bin objects)
    BinClassName => BinHashKind::ClassName
}
declare_bin_hash! {
    /// Hash of a field name of bin class
    BinFieldName => BinHashKind::FieldName
}
declare_bin_hash! {
    /// Hash of a `BinHash` value
    BinHashValue => BinHashKind::HashValue
}

define_hash_type! {
    /// Hash of a `BinPath` value, put to a file in a [cdragon_wad::Wad] archive
    BinPathValue(u64) => cdragon_wad::compute_entry_hash
}
impl BinPathValue {
    pub fn get_str<'a>(&self, mapper: &'a BinHashMappers) -> Option<&'a str> {
        mapper.path_value.get(self.hash)
    }
    pub fn seek_str<'a>(&self, mapper: &'a BinHashMappers) -> HashOrStr<u64, &'a str> {
        mapper.path_value.seek(self.hash)
    }
}


/// Trait for values enumerated in `BinType`
pub trait BinValue {
    const TYPE: BinType;
}

macro_rules! declare_bintype_struct {
    ($type:ident ($t:ty) [$($d:ident),* $(,)?]) => {
        #[derive(Debug,$($d),*)]
        pub struct $type(pub $t);
        impl From<$t> for $type {
            fn from(v: $t) -> Self { Self(v) }
        }
    };
    ($type:ident ($($v:ident: $t:ty),* $(,)?)) => {
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
#[derive(Debug)]
pub struct BinColor { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }
declare_bintype_struct!{ BinString(String) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinHash(BinHashValue) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinPath(BinPathValue) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinLink(BinEntryPath) [Eq,PartialEq,Hash] }
declare_bintype_struct!{ BinFlag(bool) [Eq,PartialEq,Hash] }


pub struct BinList {
    pub vtype: BinType,
    pub(crate) values: Box<dyn Any>,  // Any = Vec<vtype>
}

impl BinList {
    pub fn downcast<T: BinValue + 'static>(&self) -> Option<&Vec<T>> {
        self.values.downcast_ref::<Vec<T>>()
    }
}

/// Bin structure, referenced by pointer
pub struct BinStruct {
    pub ctype: BinClassName,
    pub fields: Vec<BinField>,
}

impl BinStruct {
    pub fn get(&self, name: BinFieldName) -> Option<&BinField> {
        self.fields.iter().find(|f| f.name == name)
    }

    pub fn getv<T: BinValue + 'static>(&self, name: BinFieldName) -> Option<&T> {
        self.get(name).and_then(|field| field.downcast::<T>())
    }
}

/// Bin structure whose data is embedded directly
pub struct BinEmbed {
    pub ctype: BinClassName,
    pub fields: Vec<BinField>,
}

impl BinEmbed {
    pub fn get(&self, name: BinFieldName) -> Option<&BinField> {
        self.fields.iter().find(|f| f.name == name)
    }

    pub fn getv<T: BinValue + 'static>(&self, name: BinFieldName) -> Option<&T> {
        self.get(name).and_then(|field| field.downcast::<T>())
    }
}

/// Optional bin value
pub struct BinOption {
    pub vtype: BinType,
    pub value: Option<Box<dyn Any>>,  // Any = vtype
}

impl BinOption {
    pub fn downcast<T: BinValue + 'static>(&self) -> Option<&T> {
        match self.value {
            Some(ref v) => Some(v.downcast_ref::<T>()?),
            None => None,
        }
    }
}


pub struct BinMap {
    pub ktype: BinType,
    pub vtype: BinType,
    pub(crate) values: Box<dyn Any>,  // Any = Vec<(ktype, vtype)>
}

impl BinMap {
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
#[allow(dead_code)]
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

