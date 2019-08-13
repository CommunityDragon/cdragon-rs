
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
        $crate::prop::BinType::List => { type $t = BinList; $e },
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

