
/// Map `BinType` variant to `BinValue` concrete type in an expression
#[macro_export]
macro_rules! binvalue_map_type {
    ($b:expr, $t:ident, $e:expr) => (match $b {
        $crate::BinType::None => { type $t = $crate::data::BinNone; $e },
        $crate::BinType::Bool => { type $t = $crate::data::BinBool; $e },
        $crate::BinType::S8 => { type $t = $crate::data::BinS8; $e },
        $crate::BinType::U8 => { type $t = $crate::data::BinU8; $e },
        $crate::BinType::S16 => { type $t = $crate::data::BinS16; $e },
        $crate::BinType::U16 => { type $t = $crate::data::BinU16; $e },
        $crate::BinType::S32 => { type $t = $crate::data::BinS32; $e },
        $crate::BinType::U32 => { type $t = $crate::data::BinU32; $e },
        $crate::BinType::S64 => { type $t = $crate::data::BinS64; $e },
        $crate::BinType::U64 => { type $t = $crate::data::BinU64; $e },
        $crate::BinType::Float => { type $t = $crate::data::BinFloat; $e },
        $crate::BinType::Vec2 => { type $t = $crate::data::BinVec2; $e },
        $crate::BinType::Vec3 => { type $t = $crate::data::BinVec3; $e },
        $crate::BinType::Vec4 => { type $t = $crate::data::BinVec4; $e },
        $crate::BinType::Matrix => { type $t = $crate::data::BinMatrix; $e },
        $crate::BinType::Color => { type $t = $crate::data::BinColor; $e },
        $crate::BinType::String => { type $t = $crate::data::BinString; $e },
        $crate::BinType::Hash => { type $t = $crate::data::BinHash; $e },
        $crate::BinType::Path => { type $t = $crate::data::BinPath; $e },
        $crate::BinType::List | $crate::BinType::List2 => { type $t = $crate::data::BinList; $e },
        $crate::BinType::Struct => { type $t = $crate::data::BinStruct; $e },
        $crate::BinType::Embed => { type $t = $crate::data::BinEmbed; $e },
        $crate::BinType::Link => { type $t = $crate::data::BinLink; $e },
        $crate::BinType::Option => { type $t = $crate::data::BinOption; $e },
        $crate::BinType::Map => { type $t = $crate::data::BinMap; $e },
        $crate::BinType::Flag => { type $t = $crate::data::BinFlag; $e },
    })
}

/// Same as `binvalue_map_type!`, but limited to types used a `BinMap` keys
#[macro_export]
macro_rules! binvalue_map_keytype {
    ($b:expr, $t:ident, $e:expr) => (match $b {
        $crate::BinType::S8 => { type $t = $crate::data::BinS8; $e },
        $crate::BinType::U8 => { type $t = $crate::data::BinU8; $e },
        $crate::BinType::S16 => { type $t = $crate::data::BinS16; $e },
        $crate::BinType::U16 => { type $t = $crate::data::BinU16; $e },
        $crate::BinType::S32 => { type $t = $crate::data::BinS32; $e },
        $crate::BinType::U32 => { type $t = $crate::data::BinU32; $e },
        $crate::BinType::S64 => { type $t = $crate::data::BinS64; $e },
        $crate::BinType::U64 => { type $t = $crate::data::BinU64; $e },
        $crate::BinType::Float => { type $t = $crate::data::BinFloat; $e },
        $crate::BinType::String => { type $t = $crate::data::BinString; $e },
        $crate::BinType::Hash => { type $t = $crate::data::BinHash; $e },
        _ => panic!("invalid type for map key: {}", $b as u8),
    })
}

/// Helper to access nested bin values
///
/// First parameter is the top-level bin value to access.
/// Second parameter is the sequence of items to access.
/// Elements must be properly downcasted by indicating the type when needed (in brackets).
///
/// Return an `Option`.
///
/// # Examples
///
/// ```no_run
/// # use cdragon_prop::{binget, data::*, BinEntry};
/// # fn test(entry: BinEntry, map: BinMap) {
/// // Get an entry field value
/// binget!(entry => mName(BinString));
/// // Access content of a list field
/// binget!(entry => mNames(BinList)(BinString));
/// // Chained field access
/// binget!(entry => mData(BinStruct).mValue(BinU32));
/// // Access field from hash integer value
/// binget!(entry => 0x12345678(BinString));
/// // Access entries of a `BinMap`
/// binget!(map => (BinHash, BinLink));
/// # }
/// ```
#[macro_export]
macro_rules! binget {
    // Entry-point: wrap in a lambda to use `?` to handle options
    ($e:expr => $($tail:tt)*) => { (|| Some(binget!($e, $($tail)*)))() };
    // Termination
    ($e:expr, ) => { $e };
    // `.`: intended to be used to chain field access, but actually ignored
    ($e:expr, . $($tail:tt)*) => { binget!($e, $($tail)*) };
    // `fieldName(Type)`: access field from struct-like
    ($e:expr, $f:ident($t:ty) $($tail:tt)*) => { binget!($e.getv::<$t>(cdragon_hashes::binh!(stringify!($f)))?, $($tail)*) };
    ($e:expr, $x:literal($t:ty) $($tail:tt)*) => { binget!($e.getv::<$t>($x.into())?, $($tail)*) };
    // `(Type)`: downcast
    ($e:expr, ($t:ty) $($tail:tt)*) => { binget!($e.downcast::<$t>()?, $($tail)*) };
    // `(Key,Value)`: map downcast
    ($e:expr, ($k:ty, $v:ty) $($tail:tt)*) => { binget!($e.downcast::<$k, $v>()?, $($tail)*) };
    //TODO
    // - allow shorten types (e.g. without `Bin`), requires `concat_indents!()`, or procedural macro
}

