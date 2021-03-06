//! Support of PROP files

#[macro_use]
mod macros;
mod parser;
mod serializer;
mod text_tree;
mod json;
mod gather_hashes;
mod data;

use std::io;
use std::path::Path;
use std::collections::HashSet;

use crate::Result;
use crate::hashes::HashMapper;
pub use serializer::{BinSerializer, BinEntriesSerializer};
pub use data::*;
pub use parser::BinEntryScanner;
pub use text_tree::TextTreeSerializer;
pub use json::JsonSerializer;


/// Mapper used for bin hashes
pub type BinHashMapper = HashMapper<u32>;

/// Generic type to map `BinHashKind`
pub struct BinHashKindMapping<T> {
    pub entry_path: T,
    pub class_name: T,
    pub field_name: T,
    pub hash_value: T,
}

impl<T> BinHashKindMapping<T> {
    /// Give access to a specific field from its kind
    #[inline]
    pub fn get(&self, kind: BinHashKind) -> &T {
        match kind {
            BinHashKind::EntryPath => &self.entry_path,
            BinHashKind::ClassName => &self.class_name,
            BinHashKind::FieldName => &self.field_name,
            BinHashKind::HashValue => &self.hash_value,
        }
    }

    /// Give mutable access to a specific mapper from its kind
    #[inline]
    pub fn get_mut(&mut self, kind: BinHashKind) -> &mut T {
        match kind {
            BinHashKind::EntryPath => &mut self.entry_path,
            BinHashKind::ClassName => &mut self.class_name,
            BinHashKind::FieldName => &mut self.field_name,
            BinHashKind::HashValue => &mut self.hash_value,
        }
    }
}

impl<T: Default> Default for BinHashKindMapping<T> {
    fn default() -> Self {
        Self {
            entry_path: T::default(),
            class_name: T::default(),
            field_name: T::default(),
            hash_value: T::default(),
        }
    }
}


/// Mapper for all kinds of bin hashes
pub type BinHashMappers = BinHashKindMapping<BinHashMapper>;

impl BinHashMappers {
    /// Create mapper, load all sub-mappers from a directory path
    pub fn from_dirpath(path: &Path) -> Result<Self> {
        let mut this = Self::default();
        this.load_dirpath(&path)?;
        Ok(this)
    }

    /// Load all sub-mappers from a directory path
    pub fn load_dirpath(&mut self, path: &Path) -> Result<()> {
        for kind in BinHashKind::variants() {
            self.get_mut(kind).load_path(path.join(kind.mapper_path()))?;
        }
        Ok(())
    }
}


/// PROP file, with entries
pub struct PropFile {
    pub version: u32,
    pub is_patch: bool,
    pub linked_files: Vec<String>,
    pub entries: Vec<BinEntry>,
}

impl PropFile {
    /// Parse a whole `PropFile` from data
    pub fn from_slice(data: &[u8]) -> Result<PropFile> {
        Ok(parser::binparse(data)?)
    }

    /// Iterate on entry headers (path and type) from a PROP file
    pub fn scan_entries<R: io::Read>(reader: R) -> Result<BinEntryScanner<R>> {
        BinEntryScanner::new(reader)
    }
}

/// Entry header, used by parsers that iterate on entries
pub type BinEntryHeader = (BinEntryPath, BinClassName);

/// Entry in a PROP file
pub struct BinEntry {
    pub path: BinEntryPath,
    pub ctype: BinClassName,
    pub fields: Vec<BinField>,
}

pub type BinHashSets = BinHashKindMapping<HashSet<u32>>;
use gather_hashes::GatherHashes;

impl BinEntry {
    pub fn gather_bin_hashes(&self, hashes: &mut BinHashSets) {
        self.gather_hashes(hashes);
    }
}

/// Compute a bin hash from a string
///
/// The input string is assumed to be ASCII only.
/// Use FNV-1a hash, on lowercased input.
pub fn compute_binhash(s: &str) -> u32 {
    s.to_ascii_lowercase().bytes()
        .fold(0x811c9dc5_u32, |h, b| (h ^ b as u32).wrapping_mul(0x01000193))
}

