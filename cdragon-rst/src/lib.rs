//! Support of Riot translation files (RST)
//!
//! Use [Rst] to open an RST file (`.stringtable`) and access its content.
//!
//! An RST file maps hashed translation keys to translation strings.
//! When an instance is created, the file header is parsed, data is read, but strings are actually
//! read and parsed (as UTF-8) only on access.
//!
//! # Example
//! ```no_run
//! # use cdragon_rst::Rst;
//! # // Use explicit type annotation, required only by rustdoc
//! # type RstHashMapper = cdragon_rst::RstHashMapper<39>;
//!
//! let rst = Rst::open("main_en_us.stringtable").expect("failed to open or read data");
//! // Get an entry by its key string
//! assert_eq!(rst.get("item_1001_name"), Some("Boots".into()));
//! // Or by its key hash
//! assert_eq!(rst.get(0x3376eae1da), Some("Boots".into()));
//!
//! // Entries can be iterated
//! // Use a mapper to filter on (known) keys
//! let hmapper = RstHashMapper::from_path("hashes.rst.txt").expect("failed to load hashes");
//! for (hash, value) in rst.iter() {
//!     if let Some(key) = hmapper.get(hash) {
//!         println!("{key} = {value}");
//!     }
//! }
//! ```
//!
//! # Older RST versions
//!
//! ## Hash bit size
//!
//! Hashes from RST files used more bits.
//! Number of bits used by an RST file can be retrieved with [Rst::hash_bits()].
//! The default [RstHashMapper] is suitable for the latest RST version.
//!
//! ## Encrypted entries
//!
//! Older RST versions could have encrypted entries whose data is not valid UTF-8.
//! Use [Rst::get_raw()] to access both encrypted and non-encrypted entries.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Read, Seek, BufReader};
use std::path::Path;
use nom::{
    number::complete::{le_u8, le_u32, le_u64},
    bytes::complete::tag,
    sequence::tuple,
};
use thiserror::Error;
use cdragon_hashes::rst::compute_rst_hash_full;
use cdragon_utils::{
    parsing::{ParseError, ReadArray},
    parse_buf,
};
pub use cdragon_hashes::rst::RstHashMapper;


/// Result type for RST errors
type Result<T, E = RstError> = std::result::Result<T, E>;

/// A raw RST entry value, possibly encrypted
#[derive(Debug)]
pub enum RstRawValue<'a> {
    String(&'a [u8]),
    Encrypted(&'a [u8]),
}


/// Riot translation file
///
/// String values can be accessed by hash key or string key.
/// All getters accept non-truncated hashes and will truncate it as needed.
pub struct Rst {
    /// RST version
    pub version: u8,
    /// Optional font config (obsolete)
    pub font_config: Option<String>,
    /// Number of bits per hash
    hash_bits: u8,
    /// True if some entries are encrypted
    has_trenc: bool,
    /// Entry offsets, indexed by their hash
    entry_offsets: HashMap<u64, usize>,
    /// Buffer of entry data (unparsed)
    entry_data: Vec<u8>,
}

impl Rst {
    /// Open an RMAN file from path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = std::fs::File::open(path.as_ref())?;
        let reader = BufReader::new(file);
        Rst::read(reader)
    }

    /// Read an RST file, check header, read entry headers
    pub fn read<R: Read + Seek>(mut reader: R) -> Result<Self> {
        let (version, hash_bits, font_config, entry_count) = Self::parse_header(&mut reader)?;

        let entry_offsets = {
            let mut entry_offsets = HashMap::with_capacity(entry_count as usize);
            let mut buf = vec![0; 8 * entry_count as usize];
            reader.read_exact(&mut buf)?;

            let hash_mask = (1 << hash_bits) - 1;
            let mut it = nom::combinator::iterator(buf.as_slice(), le_u64);
            entry_offsets.extend(it
                .take(entry_count as usize)
                .map(|v: u64| (v & hash_mask, (v >> hash_bits) as usize))
            );
            let result: nom::IResult<_, _, ()> = it.finish();
            let _ = result.map_err(ParseError::from)?;
            entry_offsets
        };

        let has_trenc = version < 5 && reader.read_array::<1>()?[0] != 0;

        let mut entry_data = Vec::new();
        reader.read_to_end(&mut entry_data)?;

        Ok(Self {
            version,
            font_config,
            hash_bits,
            has_trenc,
            entry_offsets,
            entry_data,
        })
    }

    /// Parse header, advance to the beginning of entry directory
    fn parse_header<R: Read + Seek>(reader: &mut R) -> Result<(u8, u8, Option<String>, u32)> {
        let version = {
            let buf = reader.read_array::<{3 + 1}>()?;
            let (_, version) = parse_buf!(buf, tuple((tag("RST"), le_u8)));
            version
        };

        let hash_bits: u8 = match version {
            2 | 3 => 40,
            4 | 5 => 39,
            _ => return Err(RstError::UnsupportedVersion(version)),
        };

        let font_config = if version == 2 && reader.read_array::<1>()?[0] != 0 {
            let buf = reader.read_array::<4>()?;
            let n = parse_buf!(buf, le_u32);
            let mut buf = vec![0; n as usize];
            reader.read_exact(&mut buf)?;
            Some(String::from_utf8(buf)?)
        } else {
            None
        };

        let entry_count = {
            let buf = reader.read_array::<4>()?;
            parse_buf!(buf, le_u32)
        };

        Ok((version, hash_bits, font_config, entry_count))
    }

    /// Get the number of bits used by hash keys
    pub fn hash_bits(&self) -> u8 {
        self.hash_bits
    }

    /// Truncate a hash key to the number of bits used by the file
    pub fn truncate_hash_key(&self, key: u64) -> u64 {
        key & ((1 << self.hash_bits) - 1)
    }

    /// Get a string from its key
    ///
    /// `key` is truncated has needed.
    /// If the entry is encrypted, return `None`.
    pub fn get<K: IntoRstKey>(&self, key: K) -> Option<Cow<'_, str>> {
        match self.get_raw_by_hash(key.into_rst_key())? {
            RstRawValue::String(s) => Some(String::from_utf8_lossy(s)),
            _ => None
        }
    }

    /// Get a raw value from its key
    pub fn get_raw<K: IntoRstKey>(&self, key: K) -> Option<RstRawValue> {
        self.get_raw_by_hash(key.into_rst_key())
    }

    /// Get a raw value from its hash key
    fn get_raw_by_hash(&self, key: u64) -> Option<RstRawValue> {
        let key = self.truncate_hash_key(key);
        let offset = *self.entry_offsets.get(&key)?;
        self.get_raw_by_offset(offset)
    }

    /// Get a raw value from its offset
    fn get_raw_by_offset(&self, offset: usize) -> Option<RstRawValue> {
        let data = &self.entry_data[offset..];
        if data[0] == 0xff && self.has_trenc {
            let size = u16::from_le_bytes(data[1..3].try_into().unwrap());
            Some(RstRawValue::Encrypted(&data[3..3+size as usize]))
        } else {
            let pos = data.iter().position(|&b| b == 0)?;
            Some(RstRawValue::String(&data[..pos]))
        }
    }

    /// Iterate on string entries
    pub fn iter(&self) -> impl Iterator<Item=(u64, Cow<'_, str>)> {
        self.entry_offsets.iter().filter_map(|(key, offset)| {
            match self.get_raw_by_offset(*offset)? {
                RstRawValue::String(s) => Some(String::from_utf8_lossy(s)),
                _ => None
            }.map(|value| (*key, value))
        })
    }
}

impl std::fmt::Debug for Rst {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rst")
            .field("version", &self.version)
            .field("font_config", &self.font_config)
            .field("hash_bits", &self.hash_bits)
            .field("has_trenc", &self.has_trenc)
            .field("len", &self.entry_offsets.len())
            .finish()
    }
}


pub trait IntoRstKey {
    fn into_rst_key(self) -> u64;
}

impl IntoRstKey for u64 {
    fn into_rst_key(self) -> u64 {
        self
    }
}

impl IntoRstKey for &str {
    fn into_rst_key(self) -> u64 {
        compute_rst_hash_full(self)
    }
}



/// Error in an RST file
#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum RstError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("parsing error")]
    Parsing(#[from] ParseError),
    #[error("version not supported: {0}")]
    UnsupportedVersion(u8),
}

