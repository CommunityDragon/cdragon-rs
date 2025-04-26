//! Hashes used in WAD archives
//!
//! File paths in WAD archive are hashed using 64-bit xxHash
use twox_hash::XxHash64;
use crate::HashMapper;

/// Compute a hash for a WAD file path
pub fn compute_wad_hash(s: &str) -> u64 {
    XxHash64::oneshot(0, s.as_bytes())
}

/// Mapper for WAD hashes
pub type WadHashMapper = HashMapper<u64, 64>;

