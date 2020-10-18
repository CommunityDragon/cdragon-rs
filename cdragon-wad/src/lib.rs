//! Support of WAD files

use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, BufReader};
use std::path::Path;
use std::hash::Hasher;
use nom::{
    number::complete::{le_u8, le_u16, le_u32, le_u64},
    bytes::complete::tag,
    combinator::map,
    sequence::tuple,
};
use derive_try_from_primitive::TryFromPrimitive;
use twox_hash::XxHash64;
use cdragon_utils::Result;
use cdragon_utils::hashes::HashMapper;
use cdragon_utils::GuardedFile;
use cdragon_utils::parsing::{ParseError, into_err};
use cdragon_utils::declare_hash_type;


/// Riot WAD archive file
///
/// Entry headers are read and stored on the instance, but not parsed.
/// Entries are parsed each time they are iterated on.
pub struct Wad {
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

    /// Open a WAD from its path, also returns the created reader
    ///
    /// The reader can be used to read entries afterwards.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<(Self, BufReader<File>)> {
        let file = File::open(path.as_ref())?;
        let mut reader = BufReader::new(file);
        let wad = Wad::read(&mut reader)?;
        Ok((wad, reader))
    }

    /// Parse header, advance to the beginning of the body
    fn parse_header<R: Read + Seek>(reader: &mut R) -> Result<((u8, u8), u32, u64)> {
        const MAGIC_VERSION_LEN: usize = 2 + 2;

        let version = {
            let mut buf = [0u8; MAGIC_VERSION_LEN];
            reader.read_exact(&mut buf)?;
            let (_, (_, major, minor)) = tuple((tag("RW"), le_u8, le_u8))(&buf).map_err(into_err)?;
            (major, minor)
        };

        let (entry_count, entry_offset) = match version.0 {
            2 => {
                // Skip "useless" fields
                reader.seek(SeekFrom::Current(84 + 8))?;
                let mut buf = [0u8; 2 + 2 + 4];
                reader.read_exact(&mut buf)?;
                let (_, (entry_offset, entry_size, entry_count)) =
                    tuple((le_u16, le_u16, le_u32))(&buf).map_err(into_err)?;
                // Not supported because it's not needed, but could be
                if entry_size != 32 {
                    return Err(ParseError::InvalidData(format!("unexpected entry size: {}", entry_size)).into());
                }
                (entry_count, entry_offset as u64)
            }
            3 => {
                // Skip "useless" fields
                reader.seek(SeekFrom::Current(264))?;
                let mut buf = [0u8; 4];
                reader.read_exact(&mut buf)?;
                let (_, entry_count) = le_u32(&buf).map_err(into_err)?;
                let entry_offset = reader.seek(SeekFrom::Current(0))?;
                (entry_count, entry_offset)
            }
            // Note: version 1 could be supported
            _ => return Err(ParseError::InvalidData(format!("unsupported version: {}.{}", version.0, version.1)).into()),
        };

        Ok((version, entry_count, entry_offset))
    }

    /// Iterate on entries
    pub fn iter_entries(&self) -> impl Iterator<Item=WadEntry> + '_ {
        (0..self.entry_count as usize).map(move |i| self.parse_entry(i))
    }

    /// Parse entry at given index 
    fn parse_entry(&self, index: usize) -> WadEntry {
        let offset = index * Self::ENTRY_LEN;
        let buffer = &self.entry_data[offset .. offset + Self::ENTRY_LEN];

        let (_, (path, offset, size, target_size, data_type, duplicate, _, data_hash)) =
            tuple((
                    map(le_u64, WadEntryHash::from), le_u32, le_u32, le_u32,
                    map(le_u8, |v| WadDataType::try_from(v).expect("invalid WAD data type")),
                    map(le_u8, |v| v != 0), le_u16, le_u64,
                    ))(buffer).map_err(into_err).expect("failed to parse WAD entry");
        WadEntry { path, offset, size, target_size, data_type, duplicate, data_hash }
    }
}

/// Information on a single file in a WAD
pub struct WadEntry {
    /// File path of the entry, hashed
    pub path: WadEntryHash,
    /// Data offset in the WAD
    offset: u32,
    /// Size in the WAD (possibly compressed)
    size: u32,
    /// Uncompressed size
    pub target_size: u32,
    data_type: WadDataType,
    /// True for duplicate entries
    pub duplicate: bool,
    /// First 8 bytes of sha256 hash of data
    pub data_hash: u64,
}

impl WadEntry {
    pub fn is_redirection(&self) -> bool {
        self.data_type == WadDataType::Redirection
    }

    /// Read entry data from a reader
    ///
    /// The entry must not be a redirection.
    pub fn read<'a, R: Read + Seek>(&self, reader: &'a mut R) -> Result<Box<dyn Read + 'a>> {
        reader.seek(SeekFrom::Start(self.offset as u64))?;
        let reader = Read::take(reader, self.size as u64);
        match self.data_type {
            WadDataType::Uncompressed => {
                Ok(Box::new(reader))
            }
            WadDataType::Gzip => Err(WadError::NotSupported("gzip entries not supported").into()),
            WadDataType::Redirection => Err(WadError::UnexpectedRedirection.into()),
            WadDataType::Zstd => {
                let decoder = zstd::stream::read::Decoder::new(reader)?;
                Ok(Box::new(decoder))
            }
        }
    }

    /// Extract entry to the given path
    pub fn extract<R: Read + Seek>(&self, reader: &mut R, path: &Path) -> Result<()> {
        let mut gfile = GuardedFile::create(path)?;
        let mut reader = self.read(reader)?;
        std::io::copy(&mut *reader, gfile.as_file_mut())?;
        gfile.persist();
        Ok(())
    }

    /// Guess the extension of an entry
    pub fn guess_extension<R: Read + Seek>(&self, reader: &mut R) -> Option<&'static str> {
        if self.target_size == 0 {
            return None;
        }
        let mut reader = self.read(reader).ok()?;
        guess_extension(&mut reader)
    }
}


declare_hash_type! {
    /// Hash used by WAD entries
    WadEntryHash(u64) => ("{:016x}", compute_entry_hash)
}

pub fn compute_entry_hash(s: &str) -> u64 {
    let mut h = XxHash64::with_seed(0);
    h.write(s.as_bytes());
    h.finish()
}

/// Mapper used for WAD path hashes
pub type WadHashMapper = HashMapper<u64>;

/// Mapper for all types of WAD path hashes
#[derive(Default)]
pub struct WadHashMappers {
    pub lcu: WadHashMapper,
    pub game: WadHashMapper,
}

/// Enum to describe each set of WAD hashes
#[derive(Copy, Clone, Debug)]
pub enum WadHashKind {
    Lcu,
    Game,
}

impl WadHashKind {
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

    /// Return filename used to store the mapping of each hash kind
    pub fn mapper_path(&self) -> &'static str {
        match self {
            Self::Lcu => &"hashes.lcu.txt",
            Self::Game => &"hashes.game.txt",
        }
    }
}

impl WadHashMappers {
    /// Create mapper, load all sub-mappers from a directory path
    pub fn from_dirpath(path: &Path) -> Result<Self> {
        let mut this = Self::default();
        this.load_dirpath(&path)?;
        Ok(this)
    }

    /// Load all sub-mappers from a directory path
    pub fn load_dirpath(&mut self, path: &Path) -> Result<()> {
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
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, TryFromPrimitive, Debug)]
enum WadDataType {
    Uncompressed = 0,
    Gzip = 1,
    Redirection = 2,
    Zstd = 3,
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


#[derive(Debug)]
pub enum WadError {
    /// WAD format is not supported
    NotSupported(&'static str),
    /// Unexpected access to a redirection entry
    UnexpectedRedirection,
}

impl fmt::Display for WadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            WadError::NotSupported(s) => f.write_str(s),
            WadError::UnexpectedRedirection => write!(f, "unexpected redirection entry"),
        }
    }
}

impl std::error::Error for WadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

