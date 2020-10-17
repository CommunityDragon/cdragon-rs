//! Support of PROP files

#[macro_use]
mod macros;
mod parser;
mod serializer;
mod text_tree;
mod json;
mod gather_hashes;
pub mod data;

use std::io;
use std::fs;
use std::path::Path;
use std::collections::HashSet;

use cdragon_utils::Result;
use cdragon_utils::hashes::HashMapper;
use cdragon_wad::WadHashKind;
pub use serializer::{BinSerializer, BinEntriesSerializer};
pub use data::*;
pub use parser::BinEntryScanner;
pub use text_tree::TextTreeSerializer;
pub use json::JsonSerializer;


/// Mapper used for bin hashes
pub type BinHashMapper = HashMapper<u32>;

/// Generic type to map `BinHashKind`
pub struct BinHashKindMapping<T, U> {
    pub entry_path: T,
    pub class_name: T,
    pub field_name: T,
    pub hash_value: T,
    pub path_value: U,
}

impl<T, U> BinHashKindMapping<T, U> {
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

impl<T: Default, U: Default> Default for BinHashKindMapping<T, U> {
    fn default() -> Self {
        Self {
            entry_path: T::default(),
            class_name: T::default(),
            field_name: T::default(),
            hash_value: T::default(),
            path_value: U::default(),
        }
    }
}


/// Mapper for all kinds of bin hashes
pub type BinHashMappers = BinHashKindMapping<BinHashMapper, HashMapper<u64>>;

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
        self.path_value.load_path(path.join(WadHashKind::Game.mapper_path()))?;
        Ok(())
    }

    /// Write all sub-mappers to a directory path
    pub fn write_dirpath(&self, path: &Path) -> Result<()> {
        for kind in BinHashKind::variants() {
            self.get(kind).write_path(path.join(kind.mapper_path()))?;
        }
        self.path_value.write_path(path.join(WadHashKind::Game.mapper_path()))?;
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

    /// Parse a whole `PropFile` from data
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<PropFile> {
        Self::from_slice(&fs::read(path.as_ref())?)
    }

    /// Iterate on entry headers (path and type) from a PROP reader
    pub fn scan_entries_from_reader<R: io::Read>(reader: R) -> Result<BinEntryScanner<R>> {
        BinEntryScanner::new(reader)
    }

    /// Iterate on entry headers (path and type) from a PROP file path
    pub fn scan_entries_from_path<P: AsRef<Path>>(path: P) -> Result<BinEntryScanner<io::BufReader<fs::File>>> {
        let file = fs::File::open(path)?;
        let reader = io::BufReader::new(file);
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

pub type BinHashSets = BinHashKindMapping<HashSet<u32>, HashSet<u64>>;
use gather_hashes::GatherHashes;

impl BinEntry {
    pub fn gather_bin_hashes(&self, hashes: &mut BinHashSets) {
        self.gather_hashes(hashes);
    }

    pub fn get(&self, name: BinFieldName) -> Option<&BinField> {
        self.fields.iter().find(|f| f.name == name)
    }

    pub fn getv<T: BinValue + 'static>(&self, name: BinFieldName) -> Option<&T> {
        self.get(name).and_then(|field| field.downcast::<T>())
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

/// Same as `compute_binhash()` but const
///
/// Implementation is less straightforward due to current limited support of const.
pub const fn compute_binhash_const(s: &str) -> u32 {
    let mut h = 0x811c9dc5_u32;
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let mut b = bytes[i];
        // convert to lowercase, const
        if b >= 'A' as u8 && b <= 'Z' as u8 {
            b = b | ' ' as u8;
        }
        h = (h ^ b as u32).wrapping_mul(0x01000193);
        i += 1;
    }
    return h;
}

