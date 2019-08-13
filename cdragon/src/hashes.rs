use std::fmt;
use std::fs::File;
use std::io::{BufReader, BufRead};
use std::collections::HashMap;
use std::path::Path;
use std::hash::Hash;
use num_traits::Num;
use crate::Result;


#[derive(Debug)]
pub enum HashError {
    InvalidHashLine(String),
    InvalidHashValue(String),
    Io(std::io::Error),
}

impl fmt::Display for HashError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HashError::InvalidHashLine(s) => write!(f, "invalid hash line: {:?}", s),
            HashError::InvalidHashValue(s) => write!(f, "invalid hash value: {:?}", s),
            HashError::Io(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for HashError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}


/// Store a single hash-to-string association
#[derive(Default)]
pub struct HashMapper<T> where T: Num + Eq + Hash {
    map: HashMap<T, String>,
}

impl<T> HashMapper<T> where T: Num + Eq + Hash {
    pub const HASH_LEN: usize = std::mem::size_of::<T>() * 2;

    /// Create a new, empty mapping
    pub fn new() -> Self {
        Self { map: HashMap::<T, String>::new() }
    }

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

    /// Load hash map from a reader
    pub fn load_reader<R: BufRead>(&mut self, reader: R) -> Result<(), HashError> {
        for line in reader.lines() {
            let l = line.map_err(|e| HashError::Io(e))?;
            if l.len() < Self::HASH_LEN + 2 {
                return Err(HashError::InvalidHashLine(l));
            }
            let hash = T::from_str_radix(&l[..Self::HASH_LEN], 16).map_err(|_e| {
                HashError::InvalidHashValue(l[..Self::HASH_LEN].to_string())
            })?;
            self.map.insert(hash, l[Self::HASH_LEN+1..].to_string());
        }
        Ok(())
    }

    /// Load hash map from a file
    pub fn load_path<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let file = File::open(&path)?;
        self.load_reader(BufReader::new(file))?;
        Ok(())
    }

    /// Get a value from the mapping
    pub fn get(&self, hash: T) -> Option<&str> {
        self.map.get(&hash).map(|v| v.as_ref())
    }

    /// Return true if the map is empty
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Return true if the given hash is known
    pub fn is_known(&self, hash: T) -> bool {
        self.map.contains_key(&hash)
    }
}


/// Trait for hash values types
///
/// This trait is implemented by types created with `declare_hash_type!`.
/// This allows to define all the hash definitions without implementing the type itself, allowing
/// one to add its own elements to the type.
pub(crate) trait HashDef: Sized {
    type T: Sized;  // Integer type
    const HASHER: fn(&str) -> Self::T;

    /// Create a new hash value from an integer
    fn new(hash: Self::T) -> Self;

    /// Convert a string into a hash value
    #[inline]
    fn from_str(s: &str) -> Self {
        Self::new(Self::HASHER(s))
    }

    /// Return true if hash is the null hash (0)
    fn is_null(&self) -> bool;
}


/// Declare a hash value type, wrapped into a unique type
macro_rules! declare_hash_type {
    (
        $(#[$meta:meta])*
        $name:ident($T:ty) => ($fmt:literal, $hasher:expr)
    ) => {
        $(#[$meta])*
        #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
        pub struct $name {
            pub hash: $T,
        }

        impl $crate::hashes::HashDef for $name {
            type T = $T;
            const HASHER: fn(&str) -> Self::T = $hasher;

            #[inline]
            fn new(hash: Self::T) -> Self {
                Self { hash }
            }

            #[inline]
            fn is_null(&self) -> bool {
                self.hash == 0
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                $crate::hashes::HashDef::from_str(s)
            }
        }

        impl From<$T> for $name {
            fn from(v: $T) -> Self {
                Self { hash: v }
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, concat!(stringify!($name), "(", $fmt, ")"), self.hash)
            }
        }

        impl std::fmt::LowerHex for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, $fmt, self.hash)
            }
        }
    }
}

