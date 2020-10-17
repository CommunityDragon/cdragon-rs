
/// Map `BinType` variant to `BinValue` concrete type in an expression
#[macro_export]
macro_rules! binvalue_map_type {
    ($b:expr, $t:ident, $e:expr) => (match $b {
        $crate::prop::BinType::None => { type $t = BinNone; $e },
        $crate::prop::BinType::Bool => { type $t = BinBool; $e },
        $crate::prop::BinType::S8 => { type $t = BinS8; $e },
        $crate::prop::BinType::U8 => { type $t = BinU8; $e },
        $crate::prop::BinType::S16 => { type $t = BinS16; $e },
        $crate::prop::BinType::U16 => { type $t = BinU16; $e },
        $crate::prop::BinType::S32 => { type $t = BinS32; $e },
        $crate::prop::BinType::U32 => { type $t = BinU32; $e },
        $crate::prop::BinType::S64 => { type $t = BinS64; $e },
        $crate::prop::BinType::U64 => { type $t = BinU64; $e },
        $crate::prop::BinType::Float => { type $t = BinFloat; $e },
        $crate::prop::BinType::Vec2 => { type $t = BinVec2; $e },
        $crate::prop::BinType::Vec3 => { type $t = BinVec3; $e },
        $crate::prop::BinType::Vec4 => { type $t = BinVec4; $e },
        $crate::prop::BinType::Matrix => { type $t = BinMatrix; $e },
        $crate::prop::BinType::Color => { type $t = BinColor; $e },
        $crate::prop::BinType::String => { type $t = BinString; $e },
        $crate::prop::BinType::Hash => { type $t = BinHash; $e },
        $crate::prop::BinType::Path => { type $t = BinPath; $e },
        $crate::prop::BinType::List | $crate::prop::BinType::List2 => { type $t = BinList; $e },
        $crate::prop::BinType::Struct => { type $t = BinStruct; $e },
        $crate::prop::BinType::Embed => { type $t = BinEmbed; $e },
        $crate::prop::BinType::Link => { type $t = BinLink; $e },
        $crate::prop::BinType::Option => { type $t = BinOption; $e },
        $crate::prop::BinType::Map => { type $t = BinMap; $e },
        $crate::prop::BinType::Flag => { type $t = BinFlag; $e },
    })
}

/// Same as `binvalue_map_type!`, but limited to types used a `BinMap` keys
#[macro_export]
macro_rules! binvalue_map_keytype {
    ($b:expr, $t:ident, $e:expr) => (match $b {
        $crate::prop::BinType::S8 => { type $t = BinS8; $e },
        $crate::prop::BinType::U8 => { type $t = BinU8; $e },
        $crate::prop::BinType::S16 => { type $t = BinS16; $e },
        $crate::prop::BinType::U16 => { type $t = BinU16; $e },
        $crate::prop::BinType::S32 => { type $t = BinS32; $e },
        $crate::prop::BinType::U32 => { type $t = BinU32; $e },
        $crate::prop::BinType::S64 => { type $t = BinS64; $e },
        $crate::prop::BinType::U64 => { type $t = BinU64; $e },
        $crate::prop::BinType::String => { type $t = BinString; $e },
        $crate::prop::BinType::Hash => { type $t = BinHash; $e },
        _ => panic!("invalid type for map key: {}", $b as u8),
    })
}

/// Convenient helper for const, inline computation of bin hashes
#[macro_export]
macro_rules! binh { ($e:expr) => { compute_binhash_const($e).into() } }

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
    // `(Type)`: downcast
    ($e:expr, ($t:ty) $($tail:tt)*) => { binget!($e.downcast::<$t>()?, $($tail)*) };
    //TODO
    // - BinMap access
    // - allow shorten types (e.g. without `Bin`), requires `concat_indents!()`, or procedural macro
}

