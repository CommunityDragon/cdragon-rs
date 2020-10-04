use std::any::Any;
use derive_try_from_primitive::TryFromPrimitive;
use super::{BinHashMappers, compute_binhash};


/// Field value for a struct or an embed
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
            Self::EntryPath => &"hashes.binentries.txt",
            Self::ClassName => &"hashes.bintypes.txt",
            Self::FieldName => &"hashes.binfields.txt",
            Self::HashValue => &"hashes.binhashes.txt",
        }
    }

    /// Iterate on variants
    pub fn variants() -> impl Iterator<Item=Self> {
        static VARIANTS: &'static [BinHashKind] = &[
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
        declare_hash_type! {
            $(#[$meta])*
            $name(u32) => ("{:08x}", compute_binhash)
        }

        impl $name {
            const KIND: BinHashKind = $kind;
            pub fn get_str<'a>(&self, mapper: &'a BinHashMappers) -> Option<&'a str> {
                mapper.get(Self::KIND).get(self.hash)
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


/// Trait for values enumerated in `BinType`
pub trait BinValue {}

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

impl BinValue for BinNone {}
impl BinValue for BinBool {}
impl BinValue for BinS8 {}
impl BinValue for BinU8 {}
impl BinValue for BinS16 {}
impl BinValue for BinU16 {}
impl BinValue for BinS32 {}
impl BinValue for BinU32 {}
impl BinValue for BinS64 {}
impl BinValue for BinU64 {}
impl BinValue for BinFloat {}
impl BinValue for BinVec2 {}
impl BinValue for BinVec3 {}
impl BinValue for BinVec4 {}
impl BinValue for BinMatrix {}
impl BinValue for BinColor {}
impl BinValue for BinString {}
impl BinValue for BinHash {}
impl BinValue for BinList {}
impl BinValue for BinStruct {}
impl BinValue for BinEmbed {}
impl BinValue for BinLink {}
impl BinValue for BinOption {}
impl BinValue for BinMap {}
impl BinValue for BinFlag {}


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
    List = 18,
    Struct = 19,
    Embed = 20,
    Link = 21,
    Option = 22,
    Map = 23,
    Flag = 24,
}

