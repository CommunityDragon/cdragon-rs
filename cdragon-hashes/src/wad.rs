//! Hashes used in WAD archives
//!
//! File paths in WAD archive are hashed using 64-bit xxHash
use std::hash::Hasher;
use std::path::Path;
use twox_hash::XxHash64;
use crate::HashMapper;

/// Compute a hash for a WAD file path
pub fn compute_wad_hash(s: &str) -> u64 {
    let mut h = XxHash64::with_seed(0);
    h.write(s.as_bytes());
    h.finish()
}

/// Mapper for WAD hashes
pub type WadHashMapper = HashMapper<u64>;

/// Enum to describe each set of WAD hashes
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum WadHashKind {
    /// WAD from launcher (`.wad`)
    Lcu,
    /// WAD from game files (`.wad.client`)
    Game,
}

impl WadHashKind {
    /// All kinds of bin hashes
    pub const VARIANTS: [Self; 2] = [
        Self::Lcu,
        Self::Game,
    ];

    /// Return WAD hash kind from a WAD path
    ///
    /// The path is assumed to be a "regular" WAD path that follows Riot conventions.
    pub fn from_wad_path<P: AsRef<Path>>(path: P) -> Option<Self> {
        let path = path.as_ref().to_str()?;
        if path.ends_with(".wad.client") {
            Some(Self::Game)
        } else if path.ends_with(".wad") {
            Some(Self::Lcu)
        } else {
            None
        }
    }

    /// Return filename used by CDragon to store the mapping this kind of hash
    pub fn mapper_path(&self) -> &'static str {
        match self {
            Self::Lcu => "hashes.lcu.txt",
            Self::Game => "hashes.game.txt",
        }
    }
}

/// Helper for const, inline computation of bin hashes, with implicit conversion
#[macro_export]
macro_rules! binh {
    ($e:expr) => { $crate::bin::compute_binhash_const($e).into() };
    ($t:ident, $e:literal) => { $t { hash: $crate::bin::compute_binhash_const($e) } };
}

