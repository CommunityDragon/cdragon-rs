//! Support of Riot PROP files
//!
//! # Overview
//!
//! PROP files, more commonly called *bin* files, contain nested data structures.
//! All structures are typed and usually abide to the same type, but type is provided in the data
//! itself, so there is no need to have a schema to decode it.
//!
//! Each PROP File contains a list of [entries](BinEntry) which itself contains a nested list of
//! [fields](BinField) of various [data types](BinType).
//!
//! # Dynamic types
//!
//! Container types store data whose type is defined dynamically.
//! Since Rust types are defined statically, they cannot be used directly.
//! Moreover, even if the embedded bin type is known by the container at run-time, the Rust type
//! system requires the user to explicitely request a given type, one way or the other.
//!
//! The [`binget!()`] macro makes it easier to chain casts and should be enough when the names and
//! types to get are known in advance.
//!
//! Data can also be casted explicitely, using the `downcast()` method provided by
//! all types wrapping dynamically typed data:
//! ```no_run
//! # use cdragon_prop::data::*;
//! # fn test(field: BinField) {
//! field.downcast::<BinString>();
//! # }
//! // => `Some(BinString)` if field contains a string, `None` otherwise
//! ```
//! Containers with fields provide a `getv()` helper to follow a `get()` with a `downcast()`.
//!
//! If all possible types have to be handled, the [`binvalue_map_type!()`] can be used to provide a
//! single generic expression to handle all possible types.
//!
//! Map keys support only a subset or types. [`binvalue_map_keytype!()`] can be used to map only
//! key types. It can be combined with another [`binvalue_map_type!()`] to handle both keys and
//! values.
//!
//! **Note:** those macros are expanded to a `match` handling all possible cases; resulting code can be
//! large.
//!
//! ## Examples
//! ```
//! # use cdragon_prop::{binvalue_map_keytype, binvalue_map_type, data::*};
//! # fn test(field: BinField, map: BinMap) {
//! binvalue_map_type!(field.vtype, T, {
//!     let value: &T = field.downcast::<T>().unwrap();
//! });
//!
//! binvalue_map_keytype!(map.ktype, K,
//!     binvalue_map_type!(map.vtype, V, {
//!         let entries: &Vec<(K, V)> = map.downcast::<K, V>().unwrap();
//!     })
//! );
//! # }
//! ```
//!
//! # Bin hashes
//!
//! Bin files use 32-bit FNV-1a hashes for several identifier names:
//!
//! - [entry paths](BinEntryPath)
//! - [class names](BinClassName)
//! - [field names](BinFieldName)
//! - ["bin hash" values](BinHashValue).
//!
//! Hash values can be computed with [`compute_binhash()`] or [`compute_binhash_const()`]
//! (compile-time version). The [`binh!()`] macro can be used with both integers or strings, and
//! will convert the result into the intended type, making it convenient when a typed value (e.g.
//! [`BinEntryPath`]) is expected.
//!
//! A [`BinHashMappers`] gather all hash-to-string conversion needed by bin data.

mod macros;
mod parser;
mod serializer;
mod text_tree;
mod json;
pub mod visitor;
pub mod data;

use std::io;
use std::fs;
use std::path::Path;
use std::collections::HashSet;
use thiserror::Error;
use cdragon_hashes::{HashMapper, HashError, wad::WadHashKind};
use cdragon_utils::parsing::ParseError;
pub use cdragon_hashes::bin::{BinHashKind, BinHashMapper};

pub use serializer::{BinSerializer, BinEntriesSerializer};
pub use data::*;
pub use parser::{BinEntryScanner, BinEntryScannerItem};
pub use text_tree::TextTreeSerializer;
pub use json::JsonSerializer;
pub use visitor::{BinVisitor, BinTraversal};


/// Result type for PROP file errors
type Result<T, E = PropError> = std::result::Result<T, E>;


/// Generic type to associate each `BinHashKind` to a value
#[allow(missing_docs)]
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


/// Hash mappers for all kinds of bin hashes
///
/// Each individual mapper can be accessed either directly through its field, or from a
/// `BinHashKind` value.
pub type BinHashMappers = BinHashKindMapping<BinHashMapper, HashMapper<u64>>;

impl BinHashMappers {
    /// Create mapper, load all sub-mappers from a directory path
    pub fn from_dirpath(path: &Path) -> Result<Self, HashError> {
        let mut this = Self::default();
        this.load_dirpath(path)?;
        Ok(this)
    }

    /// Load all sub-mappers from a directory path
    pub fn load_dirpath(&mut self, path: &Path) -> Result<(), HashError> {
        for &kind in &BinHashKind::VARIANTS {
            self.get_mut(kind).load_path(path.join(kind.mapper_path()))?;
        }
        self.path_value.load_path(path.join(WadHashKind::Game.mapper_path()))?;
        Ok(())
    }

    /// Write all sub-mappers to a directory path
    pub fn write_dirpath(&self, path: &Path) -> Result<(), HashError> {
        for &kind in &BinHashKind::VARIANTS {
            self.get(kind).write_path(path.join(kind.mapper_path()))?;
        }
        self.path_value.write_path(path.join(WadHashKind::Game.mapper_path()))?;
        Ok(())
    }
}

/// Set for for all kinds of bin hashes
///
/// This type can be used to gather all known or unknown hash values.
pub type BinHashSets = BinHashKindMapping<HashSet<u32>, HashSet<u64>>;


/// PROP file, with entries
///
/// This structure contains all the data of a PROP file, completely parsed.
/// It also provides methods to simply scan an file, without storing all the data, and possibly
/// skipping unneeded data.
pub struct PropFile {
    /// PROP version
    pub version: u32,
    /// `true` for patch file
    ///
    /// Patch files are used to hot-patch data from other, regular files.
    /// They are usually much slower and a notably used by Riot to update the game without a new
    /// release (patches are then provided directly by the server when the game starts).
    pub is_patch: bool,
    /// List of paths to other PROP files
    pub linked_files: Vec<String>,
    /// List of bin entries
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
        let scanner = BinEntryScanner::new(reader)?;
        Ok(scanner)
    }

    /// Iterate on entry headers (path and type) from a PROP file path
    pub fn scan_entries_from_path<P: AsRef<Path>>(path: P) -> Result<BinEntryScanner<io::BufReader<fs::File>>> {
        let file = fs::File::open(path)?;
        let reader = io::BufReader::new(file);
        let scanner = BinEntryScanner::new(reader)?;
        Ok(scanner)
    }
}

/// Entry header, used by parsers that iterate on entries
pub type BinEntryHeader = (BinEntryPath, BinClassName);

/// Entry in a PROP file
pub struct BinEntry {
    /// Entry path (hashed)
    pub path: BinEntryPath,
    /// Class type of the entry
    pub ctype: BinClassName,
    /// Struct fields
    pub fields: Vec<BinField>,
}

impl BinEntry {
    /// Get a field by its name
    pub fn get(&self, name: BinFieldName) -> Option<&BinField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Get a field by its name and downcast it
    pub fn getv<T: BinValue + 'static>(&self, name: BinFieldName) -> Option<&T> {
        self.get(name).and_then(|field| field.downcast::<T>())
    }
}

/// Files known to not be PROP files, despite their extension
pub const NON_PROP_BASENAMES: &[&str]  = &[
    "atlas_info.bin",
    "tftoutofgamecharacterdata.bin",
    "tftmapcharacterlists.bin",
    "tftactivesets.bin",
    "tftitemlist.bin",
];

/// Return `true` if a path is a bin file path
///
/// This helper is intended to be used with files extracted by CDragon.
/// It has several limitations.
///
/// - File content is not checked, only path is checkde
/// - Some PROP files don't have the `.bin` extension, they will not be detected.
///   (CDragon add the missing extension and thus does not have this problem.)
/// - Some files have a `.bin` extension but are not actually PROP files.
///   This helper return `false` for known occurrences.
pub fn is_binfile_path(path: &Path) -> bool {
    if let Some(true) = path.extension().map(|s| s == "bin") {
        if let Some(name) = path.file_name() {
            // Some files are not actual 'PROP' files
            return name.to_str()
                .map(|s| !NON_PROP_BASENAMES.contains(&s))
                .unwrap_or(false)
        }
    }
    false
}


/// Error in a PROP file
#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum PropError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("parsing error")]
    Parsing(#[from] ParseError),
}

