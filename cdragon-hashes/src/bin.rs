//! Hashes used in PROP files (bin files)
//!
//! Bin files use 32-bit FNV-1a hashes for several identifier names.
//!
//! This module provides methods to compute these hashes.
use super::{HashKind, HashMapper};

/// Compute a bin hash from a string
///
/// The input string is assumed to be ASCII only.
pub fn compute_binhash(s: &str) -> u32 {
    s.to_ascii_lowercase().bytes()
        .fold(0x811c9dc5_u32, |h, b| (h ^ b as u32).wrapping_mul(0x01000193))
}

/// Same as `compute_binhash()` but const
///
/// Implementation is less straightforward due to current limited support of const.
pub const fn compute_binhash_const(s: &str) -> u32 {
    let mut h = 0x811c9dc5_u32;
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i].to_ascii_lowercase();
        h = (h ^ b as u32).wrapping_mul(0x01000193);
        i += 1;
    }
    h
}

/// Get a bin hash, either parsed from hex, or computed from a string
///
/// A hex hash can be surrounded by braces (e.g. `{012345678}`).
///
/// This method can be used to get a hash, known or not, from a user.
pub fn binhash_from_str(s: &str) -> u32 {
    let hash = {
        if s.len() == 8 {
            u32::from_str_radix(s, 16).ok()
        } else if s.len() == 10 && s.starts_with('{') & s.ends_with('}') {
            u32::from_str_radix(&s[1..9], 16).ok()
        } else {
            None
        }
    };
    hash.unwrap_or_else(|| compute_binhash(s))
}


/// Mapper for bin hashes
pub type BinHashMapper = HashMapper<u32, 32>;

/// Enum with a variant for each kind of bin hash
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum BinHashKind {
    /// Hash of an entry path (`BinEntryPath`)
    EntryPath,
    /// Hash of an class name, used by entries, structs and embeds (`BinClassName`)
    ClassName,
    /// Hash of a field name (`BinFieldName`)
    FieldName,
    /// Hash of a hash value (`BinHashValue`)
    HashValue,
}

impl BinHashKind {
    /// All kinds of bin hashes
    pub const VARIANTS: [Self; 4] = [
        Self::EntryPath,
        Self::ClassName,
        Self::FieldName,
        Self::HashValue,
    ];
}

impl From<BinHashKind> for HashKind {
    fn from(val: BinHashKind) -> Self {
        match val {
            BinHashKind::EntryPath => HashKind::BinEntryPath,
            BinHashKind::ClassName => HashKind::BinClassName,
            BinHashKind::FieldName => HashKind::BinFieldName,
            BinHashKind::HashValue => HashKind::BinHashValue,
        }
    }
}

/// Helper for const, inline computation of bin hashes, with implicit conversion
#[macro_export]
macro_rules! binh {
    ($e:expr) => { $crate::bin::compute_binhash_const($e).into() };
    ($t:ident, $e:literal) => { $t { hash: $crate::bin::compute_binhash_const($e) } };
}

