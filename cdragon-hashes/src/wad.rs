//! Hashes used in WAD archives
//!
//! File paths in WAD archive are hashed using 64-bit xxHash
use std::hash::Hasher;
use twox_hash::XxHash64;
use crate::HashMapper;

/// Compute a hash for a WAD file path
pub fn compute_wad_hash(s: &str) -> u64 {
    let mut h = XxHash64::with_seed(0);
    h.write(s.as_bytes());
    h.finish()
}

/// Mapper for WAD hashes
pub type WadHashMapper = HashMapper<u64, 64>;

