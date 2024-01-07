//! Tools to work with hashes, as used by cdragon
//!
//! Actual hash values are created with [crate::define_hash_type!()], which implements [HashDef] and
//! conversions.
//!
//! [HashMapper] manages a mapping to retrieve a string from a hash value.
//! The type provides methods to load mapping files, check for known hashes, etc.
//! update mapping files, etc.
use std::fmt;
use std::fs::File;
use std::io::{BufReader, BufRead, BufWriter, Write};
use std::collections::HashMap;
use std::path::Path;
use std::hash::Hash;
use num_traits::Num;
use thiserror::Error;
use cdragon_utils::GuardedFile;

#[cfg(feature = "bin")]
pub mod bin;
#[cfg(feature = "rst")]
pub mod rst;
#[cfg(feature = "wad")]
pub mod wad;

type Result<T, E = HashError> = std::result::Result<T, E>;


/// Hash related error
///
/// For now, it is only used when parsing hash mappings.
#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum HashError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid hash line: {0:?}")]
    InvalidHashLine(String),
    #[error("invalid hash value: {0:?}")]
    InvalidHashValue(String),
}


/// Store hash-to-string association for a hash value
///
/// A hash mapping can be loaded from and written to files.
/// Such files store one line per hash, formatted as `<hex-value> <string>`.
#[derive(Default)]
pub struct HashMapper<T, const NBITS: usize> where T: Hash {
    map: HashMap<T, String>,
}

impl<T, const NBITS: usize> HashMapper<T, NBITS> where T: Hash {
    /// Number of characters used to format the hash
    const NCHARS: usize = NBITS.div_ceil(4);
}

impl<T, const N: usize> HashMapper<T, N> where T: Eq + Hash + Copy {
    /// Create a new, empty mapping
    pub fn new() -> Self {
        Self { map: HashMap::<T, String>::new() }
    }

    /// Get a value from the mapping
    pub fn get(&self, hash: T) -> Option<&str> {
        self.map.get(&hash).map(|v| v.as_ref())
    }

    /// Return a matching string (if known) or the hash
    ///
    /// Use this method to get a string representation with a fallback for unknown hashes.
    /// ```
    /// # use cdragon_hashes::HashMapper;
    /// let mut mapper = HashMapper::<u16>::new();
    /// mapper.insert(42, "forty-two".to_string());
    /// assert_eq!(format!("{}", mapper.seek(42)), "forty-two");
    /// assert_eq!(format!("{}", mapper.seek(0x1234)), "{1234}");
    /// ```
    pub fn seek(&self, hash: T) -> HashOrStr<T, &str> {
        match self.map.get(&hash) {
            Some(s) => HashOrStr::Str(s.as_ref()),
            None => HashOrStr::Hash(hash),
        }
    }

    /// Return `true` if the mapping is empty
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Return `true` if the given hash is known
    pub fn is_known(&self, hash: T) -> bool {
        self.map.contains_key(&hash)
    }

    /// Add a hash to the mapper
    ///
    /// **Important:** the caller must ensure the value matches the hash.
    pub fn insert(&mut self, hash: T, value: String) {
        self.map.insert(hash, value);
    }
}

impl<T, const N: usize> HashMapper<T, N> where T: Num + Eq + Hash + Copy {
    /// Create a new mapping, loaded from a reader
    pub fn from_reader<R: BufRead>(reader: R) -> Result<Self> {
        let mut this = Self::new();
        this.load_reader(reader)?;
        Ok(this)
    }

    /// Create a new mapping, loaded from a file
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut this = Self::new();
        this.load_path(&path)?;
        Ok(this)
    }

    /// Load hash mapping from a reader
    pub fn load_reader<R: BufRead>(&mut self, reader: R) -> Result<(), HashError> {
        for line in reader.lines() {
            let l = line?;
            if l.len() < Self::NCHARS + 2 {
                return Err(HashError::InvalidHashLine(l));
            }
            let hash = T::from_str_radix(&l[..Self::NCHARS], 16).map_err(|_e| {
                HashError::InvalidHashValue(l[..Self::NCHARS].to_string())
            })?;
            self.map.insert(hash, l[Self::NCHARS+1..].to_string());
        }
        Ok(())
    }

    /// Load hash mapping from a file
    pub fn load_path<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let file = File::open(&path)?;
        self.load_reader(BufReader::new(file))?;
        Ok(())
    }
}

impl<T, const N: usize> HashMapper<T, N> where T: Eq + Hash + Copy + fmt::LowerHex {
    /// Write hash mapping to a writer
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let mut entries: Vec<_> = self.map.iter().collect();
        entries.sort_by_key(|kv| kv.1);
        for (h, s) in entries {
            writeln!(writer, "{:0w$x} {}", h, s, w = Self::NCHARS)?;
        }
        Ok(())
    }

    /// Write hash map to a file
    ///
    /// File is upadeted atomically.
    pub fn write_path<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        GuardedFile::for_scope(path, |file| {
            self.write(&mut BufWriter::new(file))
        })
    }
}

impl<T, const N: usize> std::fmt::Debug for HashMapper<T, N> where T: Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HashMapper")
            .field("BIT_SIZE", &N)
            .field("len", &self.map.len())
            .finish()
    }
}


/// Trait for hash values types
///
/// This trait is implemented by types created with [crate::define_hash_type!()].
pub trait HashDef: Sized {
    /// Type of hash values (integer type)
    type Hash: Sized;
    /// Hashing method
    const HASHER: fn(&str) -> Self::Hash;

    /// Create a new hash value from an integer
    fn new(hash: Self::Hash) -> Self;

    /// Convert a string into a hash by hashing it
    #[inline]
    fn hashed(s: &str) -> Self {
        Self::new(Self::HASHER(s))
    }

    /// Return true if hash is the null hash (0)
    fn is_null(&self) -> bool;
}


/// Either a hash or its associated string
///
/// This enum is intended to be used along with a [HashMapper] for display.
/// If string is unknown, the hash value is written as `{hex-value}`
#[derive(Debug)]
pub enum HashOrStr<H, S>
where H: Copy, S: AsRef<str> {
    /// Hash value, string is unknown
    Hash(H),
    /// String value matching the hash
    Str(S),
}

impl<H, S> fmt::Display for HashOrStr<H, S>
where H: Copy + fmt::LowerHex, S: AsRef<str> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Hash(h) => write!(f, "{{{:0w$x}}}", h, w = std::mem::size_of::<H>() * 2),
            Self::Str(s) => write!(f, "{}", s.as_ref()),
        }
    }
}


/// Define a hash type wrapping an integer hash value
///
/// The created type provides
/// - a `hash` field, with the hash numeric value
/// - [HashDef] implementation
/// - conversion from a string, using the hasher method (`From<&str>` implementation that calls the hasher method
/// - implicit conversion from/to hash integer type (`From<T>`)
/// - [std::fmt::Debug] implementation
/// - [std::fmt::LowerHex] implementation
#[macro_export]
macro_rules! define_hash_type {
    (
        $(#[$meta:meta])*
        $name:ident($T:ty) => $hasher:expr
    ) => {
        $(#[$meta])*
        #[derive(Default, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
        pub struct $name {
            /// Hash value
            pub hash: $T,
        }

        impl $crate::HashDef for $name {
            type Hash = $T;
            const HASHER: fn(&str) -> Self::Hash = $hasher;

            #[inline]
            fn new(hash: Self::Hash) -> Self {
                Self { hash }
            }

            #[inline]
            fn is_null(&self) -> bool {
                self.hash == 0
            }
        }

        impl From<$T> for $name {
            fn from(v: $T) -> Self {
                Self { hash: v }
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, concat!(stringify!($name), "({:x})"), self)
            }
        }

        impl std::fmt::LowerHex for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{:0w$x}", self.hash, w = std::mem::size_of::<$T>() * 2)
            }
        }
    }
}


/// Each kind of hash handled by CDragon
///
/// See also [bin::BinHashKind] for a kind limited to bin hashes.
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum HashKind {
    /// Hash for game WAD entries (`.wad.client`)
    WadGame,
    /// Hash for launcher WAD entries (`.wad`)
    WadLcu,
    /// Hash of an bin entry path
    BinEntryPath,
    /// Hash of a bin class name
    BinClassName,
    /// Hash of a bin field name
    BinFieldName,
    /// Hash of a bin hash value
    BinHashValue,
    /// Hash of RST files (translation files)
    Rst,
}

impl HashKind {
    /// Return filename used by CDragon to store the mapping this kind of hash
    ///
    /// ```
    /// use cdragon_hashes::HashKind;
    /// assert_eq!(HashKind::WadLcu.mapping_path(), "hashes.lcu.txt");
    /// assert_eq!(HashKind::BinEntryPath.mapping_path(), "hashes.binentries.txt");
    /// ```
    pub fn mapping_path(&self) -> &'static str {
        match self {
            Self::WadGame => "hashes.game.txt",
            Self::WadLcu => "hashes.lcu.txt",
            Self::BinEntryPath => "hashes.binentries.txt",
            Self::BinClassName => "hashes.bintypes.txt",
            Self::BinFieldName => "hashes.binfields.txt",
            Self::BinHashValue => "hashes.binhashes.txt",
            Self::Rst => "hashes.rst.txt",
        }
    }

    /// Return WAD hash kind from a WAD path
    ///
    /// The path is assumed to be a "regular" WAD path that follows Riot conventions.
    /// ```
    /// use cdragon_hashes::HashKind;
    /// assert_eq!(HashKind::from_wad_path("Global.wad.client"), Some(HashKind::WadGame));
    /// assert_eq!(HashKind::from_wad_path("assets.wad"), Some(HashKind::WadLcu));
    /// assert_eq!(HashKind::from_wad_path("unknown"), None);
    /// ```
    pub fn from_wad_path<P: AsRef<Path>>(path: P) -> Option<Self> {
        let path = path.as_ref().to_str()?;
        if path.ends_with(".wad.client") {
            Some(Self::WadGame)
        } else if path.ends_with(".wad") {
            Some(Self::WadLcu)
        } else {
            None
        }
    }
}

