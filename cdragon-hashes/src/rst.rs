//! Hashes used in RST files
//!
//! Keys are hashed using 64-bit xxHash, then truncated.
use std::hash::Hasher;
use twox_hash::XxHash64;
use crate::HashMapper;

/// Compute a hash for an RST file key, untruncated
pub fn compute_rst_hash_full(s: &str) -> u64 {
    let mut h = XxHash64::with_seed(0);
    h.write(s.as_bytes());
    h.finish()
}

/// Compute a hash for an RST file key, truncated to `n` bits
pub fn compute_rst_hash_n(s: &str, bits: u8) -> u64 {
    compute_rst_hash_full(s) & ((1 << bits) - 1)
}

/// Mapper for RST hashes, use current default hash size
pub type RstHashMapper<const NBITS: usize = 39> = HashMapper<u64, NBITS>;

