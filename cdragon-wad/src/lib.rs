//! Support of Riot WAD archive files
//!
//! # Example: list files in wad
//! ```no_run
//! use cdragon_wad::{WadFile, WadHashMapper};
//! let wad = WadFile::open("Global.wad.client").expect("failed to open WAD file");
//! let hmapper = WadHashMapper::from_path("hashes.game.txt").expect("failed to load hashes");
//! for entry in wad.iter_entries() {
//!     let entry = entry.expect("failed to read entry");
//!     println!("{}", hmapper.get(entry.path.hash).unwrap_or("?"));
//! }
//! ```
//!
//! [WadHashKind] can be used to use the appropriate hash file (assuming CDragon's files are used).
//! ```
//! use cdragon_wad::WadHashKind;
//! assert_eq!(WadHashKind::from_wad_path("Global.wad.client"), Some(WadHashKind::Game));
//! assert_eq!(WadHashKind::from_wad_path("assets.wad"), Some(WadHashKind::Lcu));
//! assert_eq!(WadHashKind::from_wad_path("unknown"), None);
//! assert_eq!(WadHashKind::Lcu.mapper_path(), "hashes.lcu.txt");
//! ```

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, BufReader};
use std::path::Path;
use nom::{
    number::complete::{le_u8, le_u16, le_u32, le_u64},
    bytes::complete::tag,
    combinator::{map, map_res},
    sequence::tuple,
};
use thiserror::Error;
use cdragon_hashes::{
    define_hash_type,
    wad::compute_wad_hash,
    HashError,
};
use cdragon_utils::{
    GuardedFile,
    parsing::{ParseError, ReadArray},
    parse_buf,
};
pub use cdragon_hashes::wad::{WadHashKind, WadHashMapper};


/// Result type for WAD errors
type Result<T, E = WadError> = std::result::Result<T, E>;


/// Riot WAD archive file
///
/// Store information from the header and list of entries.
/// To read a WAD file, use [WadFile] or [WadReader].
pub struct Wad {
    /// WAD version (`(major, minor)`)
    pub version: (u8, u8),
    entry_count: u32,
    entry_data: Vec<u8>,
}

impl Wad {
    const ENTRY_LEN: usize = 32;

    /// Read a WAD file, check header, read entry headers
    pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let (version, entry_count, entry_offset) = Self::parse_header(reader)?;

        let data_size = Self::ENTRY_LEN * entry_count as usize;
        let mut entry_data = Vec::with_capacity(data_size);
        reader.seek(SeekFrom::Start(entry_offset))?;
        if reader.take(data_size as u64).read_to_end(&mut entry_data)? != data_size {
            return Err(ParseError::NotEnoughData.into());
        }

        Ok(Self { version, entry_count, entry_data })
    }

    /// Parse header, advance to the beginning of the body
    fn parse_header<R: Read + Seek>(reader: &mut R) -> Result<((u8, u8), u32, u64)> {
        const MAGIC_VERSION_LEN: usize = 2 + 2;

        let version = {
            let buf = reader.read_array::<MAGIC_VERSION_LEN>()?;
            let (_, major, minor) = parse_buf!(buf, tuple((tag("RW"), le_u8, le_u8)));
            (major, minor)
        };

        let (entry_count, entry_offset) = match version.0 {
            2 => {
                // Skip "useless" fields
                reader.seek(SeekFrom::Current(84 + 8))?;
                let buf = reader.read_array::<{2 + 2 + 4}>()?;
                let (entry_offset, entry_size, entry_count) = parse_buf!(buf, tuple((le_u16, le_u16, le_u32)));
                // Not supported because it's not needed, but could be
                if entry_size != 32 {
                    return Err(WadError::UnsupportedV2EntrySize(entry_size));
                }
                (entry_count, entry_offset as u64)
            }
            3 => {
                // Skip "useless" fields
                reader.seek(SeekFrom::Current(264))?;
                let buf = reader.read_array::<4>()?;
                let entry_count = parse_buf!(buf, le_u32);
                let entry_offset = reader.stream_position()?;
                (entry_count, entry_offset)
            }
            // Note: version 1 could be supported
            _ => return Err(WadError::UnsupportedVersion(version.0, version.1)),
        };

        Ok((version, entry_count, entry_offset))
    }

    /// Iterate on file entries
    pub fn iter_entries(&self) -> impl Iterator<Item=Result<WadEntry>> + '_ {
        (0..self.entry_count as usize).map(move |i| self.parse_entry(i))
    }

    /// Parse entry at given index
    fn parse_entry(&self, index: usize) -> Result<WadEntry> {
        let offset = index * Self::ENTRY_LEN;
        let buf = &self.entry_data[offset .. offset + Self::ENTRY_LEN];

        let (path, offset, size, target_size, data_format, duplicate, first_subchunk_index, data_hash) =
            parse_buf!(buf, tuple((
                        map(le_u64, WadEntryHash::from), le_u32, le_u32, le_u32,
                        map_res(le_u8, WadDataFormat::try_from),
                        map(le_u8, |v| v != 0), le_u16, le_u64,
            )));
        Ok(WadEntry { path, offset, size, target_size, data_format, duplicate, first_subchunk_index, data_hash })
    }

    /// Find '.subchunktoc' file, if one exists
    fn find_subchunk_toc(&self, hmapper: &WadHashMapper) -> Option<WadEntry> {
        for entry in self.iter_entries().flatten() {
            if let Some(path) = hmapper.get(entry.path.hash) {
                if path.ends_with(".subchunktoc") {
                    return Some(entry)
                }
            }
        }
        None
    }
}

/// Read WAD archive files and their entries
///
/// This should be the prefered way to read a WAD file.
pub struct WadReader<R: Read + Seek> {
    reader: R,
    wad: Wad,
    subchunk_toc: Vec<WadSubchunkTocEntry>,
}

impl<R: Read + Seek> WadReader<R> {
    /// Load subchunks data from a '.subchunktoc' file
    ///
    /// Return whether data has been found, and loaded
    pub fn load_subchunk_toc(&mut self, hmapper: &WadHashMapper) -> Result<bool> {
        if let Some(entry) = self.wad.find_subchunk_toc(hmapper) {
            const TOC_ITEM_LEN: usize = 4 + 4 + 8;
            let nitems = entry.target_size as usize / TOC_ITEM_LEN;
            self.subchunk_toc.clear();
            self.subchunk_toc.reserve_exact(nitems);

            let mut subchunk_toc = Vec::new();
            {
                let mut reader = self.read_entry(&entry)?;
                for _ in 0..nitems {
                    let buf = reader.read_array::<TOC_ITEM_LEN>()?;
                    let (size, target_size, data_hash) = parse_buf!(buf, tuple((le_u32, le_u32, le_u64)));
                    subchunk_toc.push(WadSubchunkTocEntry { size, target_size, data_hash });
                }
            }
            self.subchunk_toc = subchunk_toc;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Read an entry data
    ///
    /// The entry must not be a redirection.
    pub fn read_entry(&mut self, entry: &WadEntry) -> Result<Box<dyn Read + '_>, WadError> {
        self.reader.seek(SeekFrom::Start(entry.offset as u64))?;
        let mut reader = Read::take(&mut self.reader, entry.size as u64);
        match entry.data_format {
            WadDataFormat::Uncompressed => {
                Ok(Box::new(reader))
            }
            WadDataFormat::Gzip => Err(WadError::UnsupportedDataFormat(entry.data_format)),
            WadDataFormat::Redirection => Err(WadError::UnsupportedDataFormat(entry.data_format)),
            WadDataFormat::Zstd => {
                let decoder = zstd::stream::read::Decoder::new(reader)?;
                Ok(Box::new(decoder))
            }
            WadDataFormat::Chunked(subchunk_count) => {
                if self.subchunk_toc.is_empty() {
                    Err(WadError::MissingSubchunkToc)
                } else {
                    // Allocate the whole final buffer and read everything right no
                    // It would be possible to implement a custom reader but that's not worth the
                    // complexity
                    let mut result = Vec::with_capacity(entry.target_size as usize);
                    for i in 0..subchunk_count {
                        let subchunk_entry = &self.subchunk_toc[(entry.first_subchunk_index + i as u16) as usize];
                        let mut subchunk_reader = Read::take(&mut reader, subchunk_entry.size as u64);
                        if subchunk_entry.size == subchunk_entry.target_size {
                            // Assume no compression
                            subchunk_reader.read_to_end(&mut result)?;
                        } else {
                            zstd::stream::read::Decoder::new(subchunk_reader)?.read_to_end(&mut result)?;
                        }
                    }
                    Ok(Box::new(std::io::Cursor::new(result)))
                }
            }
        }
    }

    /// Extract an entry to the given path
    pub fn extract_entry(&mut self, entry: &WadEntry, path: &Path) -> Result<()> {
        let mut reader = self.read_entry(entry)?;
        GuardedFile::for_scope(path, |file| {
            std::io::copy(&mut *reader, file)
        })?;
        Ok(())
    }

    /// Guess the extension of an entry
    pub fn guess_entry_extension(&mut self, entry: &WadEntry) -> Option<&'static str> {
        if entry.target_size == 0 {
            return None;
        }
        let mut reader = self.read_entry(entry).ok()?;
        guess_extension(&mut reader)
    }

    /// Iterate on entries
    pub fn iter_entries(&self) -> impl Iterator<Item=Result<WadEntry>> + '_ {
        self.wad.iter_entries()
    }
}

/// Read WAD from a file
pub type WadFile = WadReader<BufReader<File>>;

impl WadFile {
    /// Open a WAD from its path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path.as_ref())?;
        let mut reader = BufReader::new(file);
        let wad = Wad::read(&mut reader)?;
        Ok(Self { reader, wad, subchunk_toc: Vec::new(), })
    }
}


/// Subchunk TOC item data
struct WadSubchunkTocEntry {
    /// Subchunk size, compressed
    size: u32,
    /// Subchunk size, uncompressed
    target_size: u32,
    /// First 8 bytes of sha256 hash of data
    #[allow(dead_code)]
    data_hash: u64,
}


/// Information on a single file in a WAD
#[allow(dead_code)]
pub struct WadEntry {
    /// File path of the entry, hashed
    pub path: WadEntryHash,
    /// Data offset in the WAD
    offset: u32,
    /// Size in the WAD (possibly compressed)
    size: u32,
    /// Uncompressed size
    target_size: u32,
    /// Format of the entry data in the WAD file
    data_format: WadDataFormat,
    /// True for duplicate entries
    duplicate: bool,
    /// Index of the first subchunk (only relevant for chunked data)
    first_subchunk_index: u16,
    /// First 8 bytes of sha256 hash of data
    data_hash: u64,
}

impl WadEntry {
    /// Return `true` for a redirection entry
    pub fn is_redirection(&self) -> bool {
        self.data_format == WadDataFormat::Redirection
    }
}


define_hash_type! {
    /// Hash used by WAD entries
    WadEntryHash(u64) => compute_wad_hash
}

/// Mapper for all types of WAD path hashes
#[derive(Default)]
pub struct WadHashMappers {
    /// Hash mapper for launcher WAD files
    pub lcu: WadHashMapper,
    /// Hash mapper for game WAD files
    pub game: WadHashMapper,
}

impl WadHashMappers {
    /// Create mapper, load all sub-mappers from a directory path
    pub fn from_dirpath(path: &Path) -> Result<Self, HashError> {
        let mut this = Self::default();
        this.load_dirpath(path)?;
        Ok(this)
    }

    /// Load all sub-mappers from a directory path
    pub fn load_dirpath(&mut self, path: &Path) -> Result<(), HashError> {
        self.lcu.load_path(path.join(WadHashKind::Lcu.mapper_path()))?;
        self.game.load_path(path.join(WadHashKind::Game.mapper_path()))?;
        Ok(())
    }

    /// Get mapper from hash kind
    pub fn mapper(&self, kind: WadHashKind) -> &WadHashMapper {
        match kind {
            WadHashKind::Lcu => &self.lcu,
            WadHashKind::Game => &self.game,
        }
    }

    /// Get mapper to use for given WAD path
    pub fn mapper_from_wad_path<P: AsRef<Path>>(&self, path: P) -> Option<&WadHashMapper> {
        WadHashKind::from_wad_path(path).map(|kind| self.mapper(kind))
    }
}


#[allow(dead_code)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
/// Type of a WAD entry
pub enum WadDataFormat {
    /// Uncompressed entry
    Uncompressed,
    /// Entry compressed with gzip
    Gzip,
    /// Entry redirection
    Redirection,
    /// Entry compressed with zstd
    Zstd,
    /// Entry split into *n* individual zstd-compressed chunks
    ///
    /// A "subchunk TOC" is required for such entries.
    Chunked(u8),
}

impl TryFrom<u8> for WadDataFormat {
    type Error = WadError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Uncompressed),
            1 => Ok(Self::Gzip),
            2 => Ok(Self::Redirection),
            3 => Ok(Self::Zstd),
            b if b & 0xf == 4 => Ok(Self::Chunked(b >> 4)),
            _ => Err(WadError::InvalidDataFormat(value)),
        }
    }
}


/// Guess file extension from a reader
fn guess_extension(reader: &mut dyn Read) -> Option<&'static str> {
    const PREFIX_TO_EXT: &[(&[u8], &str)] = &[
        (b"\xff\xd8\xff", "jpg"),
        (b"\x89PNG\x0d\x0a\x1a\x0a", "png"),
        (b"OggS", "ogg"),
        (b"\x00\x01\x00\x00", "ttf"),
        (b"\x1a\x45\xdf\xa3", "webm"),
        (b"true", "ttf"),
        (b"OTTO\0", "otf"),
        (b"\"use strict\";", "min.js"),
        (b"<template ", "template.html"),
        (b"<!-- Elements -->", "template.html"),
        (b"DDS ", "dds"),
        (b"<svg", "svg"),
        (b"PROP", "bin"),
        (b"PTCH", "bin"),
        (b"BKHD", "bnk"),
        (b"r3d2Mesh", "scb"),
        (b"r3d2anmd", "anm"),
        (b"r3d2canm", "anm"),
        (b"r3d2sklt", "skl"),
        (b"r3d2", "wpk"),
        (b"\x33\x22\x11\x00", "skn"),
        (b"PreLoadBuildingBlocks = {", "preload"),
        (b"\x1bLuaQ\x00\x01\x04\x04", "luabin"),
        (b"\x1bLuaQ\x00\x01\x04\x08", "luabin64"),
        (b"\x02\x3d\x00\x28", "troybin"),
        (b"[ObjectBegin]", "sco"),
        (b"OEGM", "mapgeo"),
        (b"TEX\0", "tex"),
    ];

    // Use a sufficient length for all extensions (this is not checked)
    let mut buf: [u8; 32] = [0; 32];
    let n = reader.read(&mut buf).ok()?;
    let buf = &buf[..n];
    PREFIX_TO_EXT
        .iter()
        .find(|(prefix, _)| buf.starts_with(prefix))
        .map(|(_, ext)| *ext)
        // Try to parse as JSON
        // Note: it won't detected JSON files that start with a BOM
        .or_else(|| if match serde_json::from_slice::<serde_json::Value>(buf) {
            Ok(_) => true,
            Err(e) if e.is_eof() => true,
            _ => false,
        } {
            Some("json")
        } else {
            None
        })
}


/// Error in a WAD file
#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum WadError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("parsing error")]
    Parsing(#[from] ParseError),
    #[error("invalid WAD entry data format: {0}")]
    InvalidDataFormat(u8),
    #[error("WAD version not supported: {0}.{1}")]
    UnsupportedVersion(u8, u8),
    #[error("WAD entry data format not supported for reading: {0:?}")]
    UnsupportedDataFormat(WadDataFormat),
    #[error("WAD V2 entry size not supported: {0}")]
    UnsupportedV2EntrySize(u16),
    #[error("missing subchunk TOC to read chunked entry")]
    MissingSubchunkToc,
}

