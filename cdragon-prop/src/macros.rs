
/// Map `BinType` variant to `BinValue` concrete type in an expression
#[macro_export]
macro_rules! binvalue_map_type {
    ($b:expr, $t:ident, $e:expr) => (match $b {
        $crate::BinType::None => { type $t = BinNone; $e },
        $crate::BinType::Bool => { type $t = BinBool; $e },
        $crate::BinType::S8 => { type $t = BinS8; $e },
        $crate::BinType::U8 => { type $t = BinU8; $e },
        $crate::BinType::S16 => { type $t = BinS16; $e },
        $crate::BinType::U16 => { type $t = BinU16; $e },
        $crate::BinType::S32 => { type $t = BinS32; $e },
        $crate::BinType::U32 => { type $t = BinU32; $e },
        $crate::BinType::S64 => { type $t = BinS64; $e },
        $crate::BinType::U64 => { type $t = BinU64; $e },
        $crate::BinType::Float => { type $t = BinFloat; $e },
        $crate::BinType::Vec2 => { type $t = BinVec2; $e },
        $crate::BinType::Vec3 => { type $t = BinVec3; $e },
        $crate::BinType::Vec4 => { type $t = BinVec4; $e },
        $crate::BinType::Matrix => { type $t = BinMatrix; $e },
        $crate::BinType::Color => { type $t = BinColor; $e },
        $crate::BinType::String => { type $t = BinString; $e },
        $crate::BinType::Hash => { type $t = BinHash; $e },
        $crate::BinType::Path => { type $t = BinPath; $e },
        $crate::BinType::List | $crate::BinType::List2 => { type $t = BinList; $e },
        $crate::BinType::Struct => { type $t = BinStruct; $e },
        $crate::BinType::Embed => { type $t = BinEmbed; $e },
        $crate::BinType::Link => { type $t = BinLink; $e },
        $crate::BinType::Option => { type $t = BinOption; $e },
        $crate::BinType::Map => { type $t = BinMap; $e },
        $crate::BinType::Flag => { type $t = BinFlag; $e },
    })
}

/// Same as `binvalue_map_type!`, but limited to types used a `BinMap` keys
#[macro_export]
macro_rules! binvalue_map_keytype {
    ($b:expr, $t:ident, $e:expr) => (match $b {
        $crate::BinType::S8 => { type $t = BinS8; $e },
        $crate::BinType::U8 => { type $t = BinU8; $e },
        $crate::BinType::S16 => { type $t = BinS16; $e },
        $crate::BinType::U16 => { type $t = BinU16; $e },
        $crate::BinType::S32 => { type $t = BinS32; $e },
        $crate::BinType::U32 => { type $t = BinU32; $e },
        $crate::BinType::S64 => { type $t = BinS64; $e },
        $crate::BinType::U64 => { type $t = BinU64; $e },
        $crate::BinType::Float => { type $t = BinFloat; $e },
        $crate::BinType::String => { type $t = BinString; $e },
        $crate::BinType::Hash => { type $t = BinHash; $e },
        _ => panic!("invalid type for map key: {}", $b as u8),
    })
}

/// Convenient helper for const, inline computation of bin hashes
#[macro_export]
macro_rules! binh {
    ($e:expr) => { compute_binhash_const($e).into() };
    ($t:ident, $e:literal) => { $t { hash: compute_binhash_const($e) } };
}

/// Helper to access nested bin values
///
/// First parameter is the top-level bin value to access.
/// Second parameter is the sequence of items to access.
/// Elements must be properly downcasted by indicating the type when needed (in brackets).
///
/// Return an `Option`.
#[macro_export]
macro_rules! binget {
    // Entry-point: wrap in a lambda to use `?` to handle options
    ($e:expr => $($tail:tt)*) => { (|| Some(binget!($e, $($tail)*)))() };
    // Termination
    ($e:expr, ) => { $e };
    // `.`: intended to be used to chain field access, but actually ignored
    ($e:expr, . $($tail:tt)*) => { binget!($e, $($tail)*) };
    // `fieldName(Type)`: access field from struct-like
    ($e:expr, $f:ident($t:ty) $($tail:tt)*) => { binget!($e.getv::<$t>(binh!(stringify!($f)))?, $($tail)*) };
    ($e:expr, $x:literal($t:ty) $($tail:tt)*) => { binget!($e.getv::<$t>($x.into())?, $($tail)*) };
    // `(Type)`: downcast
    ($e:expr, ($t:ty) $($tail:tt)*) => { binget!($e.downcast::<$t>()?, $($tail)*) };
    // `(Key,Value)`: map downcast
    ($e:expr, ($k:ty, $v:ty) $($tail:tt)*) => { binget!($e.downcast::<$k, $v>()?, $($tail)*) };
    //TODO
    // - allow shorten types (e.g. without `Bin`), requires `concat_indents!()`, or procedural macro
}

