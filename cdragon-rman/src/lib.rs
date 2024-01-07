//! Support of RMAN files, Riot manifest files
//!
//! Use [Rman] to open an RMAN file and access its content.
//!
//! An RMAN file is made of a header and multiple tables (bundles, file names, ...).
//! When an instance is created, only the headers are read. Tables are then iterated on using the
//! `iter_*()` methods.
//!
//! # Example: list files
//! ```no_run
//! # use cdragon_rman::Rman;
//!
//! let rman = Rman::open("example.manifest").expect("failed to open or read headers");
//! // Directories are listed separately from files and their basenames
//! let dir_paths = rman.dir_paths();
//! // Iterate on files, print the full paths
//! for file in rman.iter_files() {
//!     println!("{}", file.path(&dir_paths));
//! }
//! ```

use std::io::{Read, BufReader};
use std::path::Path;
use std::convert::TryInto;
use std::collections::HashMap;
use nom::{
    number::complete::{le_u8, le_u16, le_u32, le_u64},
    bytes::complete::tag,
    sequence::tuple,
};
use thiserror::Error;
use cdragon_utils::{
    parsing::{ParseError, ReadArray},
    parse_buf,
};

/// Result type for RMAN errors
type Result<T, E = RmanError> = std::result::Result<T, E>;


/// Riot manifest file
///
/// The body is decompressed and parsed on demand.
/// Entries are parsed each time they are iterated on.
/// They should be cached by the caller if needed
///
/// # Note on errors
///
/// Most reading methods may panic on invalid offsets or invalid data.
/// This is especially true for the `iter_*()` methods.
pub struct Rman {
    /// RMAN version (`(major, minor)`)
    ///
    /// Currently, only version `(2, 0)` is supported.
    pub version: (u8, u8),
    /// RMAN flags
    pub flags: u16,
    /// Manifest ID
    ///
    /// Typically, it matches the manifest filename.
    pub manifest_id: u64,
    body: Vec<u8>,
    offset_bundles: i32,
    offset_flags: i32,
    offset_files: i32,
    offset_directories: i32,
}

/// Map directory ID to full paths
pub type DirPaths = HashMap<u64, String>;

/// Chunk, associated to a bundle
#[derive(Clone)]
pub struct BundleChunk {
    /// Bundle ID of the chunk
    pub bundle_id: u64,
    /// Offset of chunk in bundle
    pub bundle_offset: u32,
    /// Size of chunk in bundle, compressed
    pub bundle_size: u32,
    /// Size of chunk, uncompressed
    pub target_size: u32,
}

/// Map chunk IDs to their data
pub type BundleChunks = HashMap<u64, BundleChunk>;

impl Rman {
    /// Open an RMAN file from path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = std::fs::File::open(path.as_ref())?;
        let reader = BufReader::new(file);
        Rman::read(reader)
    }

    /// Read an RMAN file, check header and decompress body
    ///
    /// Body is assumed to have the expected size. It is not checked against header length values.
    pub fn read<R: Read>(mut reader: R) -> Result<Self> {
        let (version, flags, manifest_id, body_length) = {
            let r = reader.by_ref();
            Self::parse_header(r)?
        };
        let body = zstd::stream::decode_all(reader.take(body_length as u64))?;
        let offsets = Self::parse_body_header(&body);
        Ok(Self {
            version, flags, manifest_id, body,
            offset_bundles: offsets.0,
            offset_flags: offsets.1,
            offset_files: offsets.2,
            offset_directories: offsets.3,
        })
    }

    /// Parse header, advance to the beginning of the body
    fn parse_header<R: Read>(mut reader: R) -> Result<((u8, u8), u16, u64, u32)> {
        const MAGIC_VERSION_LEN: usize = 4 + 2;
        const FIELDS_LEN: usize = 2 + 4 + 4 + 8 + 4;
        const HEADER_LEN: usize = MAGIC_VERSION_LEN + FIELDS_LEN;

        let version = {
            let buf = reader.read_array::<MAGIC_VERSION_LEN>()?;
            let (_, major, minor) = parse_buf!(buf, tuple((tag("RMAN"), le_u8, le_u8)));
            if (major, minor) != (2, 0) {
                return Err(RmanError::UnsupportedVersion(major, minor));
            }
            (major, minor)
        };

        let (flags, manifest_id, zstd_length) = {
            let buf = reader.read_array::<FIELDS_LEN>()?;
            let (flags, offset, zstd_length, manifest_id, _body_length) =
                parse_buf!(buf, tuple((le_u16, le_u32, le_u32, le_u64, le_u32)));
            if flags & (1 << 9) == 0 {
                return Err(RmanError::UnsupportedFlags(flags));
            }
            if offset < HEADER_LEN as u32 {
                return Err(ParseError::Error.into());
            } else if offset > HEADER_LEN as u32 {
                let skipped_len = offset - HEADER_LEN as u32;
                std::io::copy(&mut reader.take(skipped_len as u64), &mut std::io::sink())?;
            }
            (flags, manifest_id, zstd_length)
        };

        Ok((version, flags, manifest_id, zstd_length))
    }

    /// Parse body header
    fn parse_body_header(body: &[u8]) -> (i32, i32, i32, i32) {
        let mut cursor = BodyCursor::new(body, 0);

        // Unknown header, skip it
        let header_len = cursor.read_i32();
        cursor.skip(header_len);

        (
            cursor.read_offset(),
            cursor.read_offset(),
            cursor.read_offset(),
            cursor.read_offset(),
            // Note: the last two tables are unknown
        )
    }

    /// Iterate on flags (locales, platforms)
    pub fn iter_flags(&self) -> OffsetTableIter<'_, FileFlagEntry> {
        let cursor = BodyCursor::new(&self.body, self.offset_flags);
        OffsetTableIter::new(cursor, parse_flag_entry)
    }

    /// Iterate on bundles
    pub fn iter_bundles(&self) -> OffsetTableIter<'_, BundleEntry<'_>> {
        let cursor = BodyCursor::new(&self.body, self.offset_bundles);
        OffsetTableIter::new(cursor, parse_bundle_entry)
    }

    /// Iterate on files
    pub fn iter_files(&self) -> OffsetTableIter<'_, FileEntry<'_>> {
        let cursor = BodyCursor::new(&self.body, self.offset_files);
        OffsetTableIter::new(cursor, parse_file_entry)
    }

    /// Iterate on directories (raw entries)
    pub fn iter_directories(&self) -> OffsetTableIter<'_, DirectoryEntry<'_>> {
        let cursor = BodyCursor::new(&self.body, self.offset_directories);
        OffsetTableIter::new(cursor, parse_directory_entry)
    }

    /// Build map of directory paths
    pub fn dir_paths(&self) -> DirPaths {
        let directories: Vec<DirectoryEntry> = self.iter_directories().collect();
        DirectoryEntry::build_path_map(&directories)
    }

    /// Build a map of chunks, with bundle information
    pub fn bundle_chunks(&self) -> BundleChunks {
        self.iter_bundles().flat_map(|bundle| {
            let bundle_id = bundle.id;
            bundle.iter_chunks().map(move |chunk| {
                (chunk.id, BundleChunk {
                    bundle_id,
                    bundle_offset: chunk.bundle_offset,
                    bundle_size: chunk.bundle_size,
                    target_size: chunk.target_size,
                })
            })
        }).collect()
    }
}


/// Parse data from RMAN body
///
/// RMAN parsing uses a lot of negative indexes. Regular slices don't allow to go backwards.
/// Implement our own parsing helpers for cleaner and easier parsing.
/// There is no error handling: parsers panic if there is not enough data.
///
/// # Implementation note
///
/// Body size is guaranteed to fits in a `u32`, and should always fit in a `i32`.
/// Use `i32` for all offsets to simplify use and avoid numerous casts.
///
/// # Errors
///
/// Parsing methods will panic on attempts to read outside the buffer.
#[derive(Clone)]
struct BodyCursor<'a> {
    body: &'a [u8],
    offset: i32,
}

impl<'a> BodyCursor<'a> {
    fn new(body: &'a [u8], offset: i32) -> Self {
        Self { body, offset }
    }

    fn offset(&self) -> i32 {
        self.offset
    }

    fn read_slice(&mut self, n: i32) -> &'a [u8] {
        let slice = &self.body[self.offset as usize .. (self.offset + n) as usize];
        self.offset += n;
        slice
    }

    fn peek_slice(&self, n: i32) -> &'a [u8] {
        &self.body[self.offset as usize .. (self.offset + n) as usize]
    }

    fn fields_cursor(mut self) -> BodyFieldsCursor<'a> {
        let entry_offset = self.offset();
        let fields_offset = entry_offset - self.read_i32() + 2 * 2;  // Note: skip the 2 header fields
        BodyFieldsCursor { body: self.body, fields_offset, entry_offset }
    }

    /// Read an offset and return a new cursor pointing to it
    fn subcursor(&mut self) -> Self {
        Self::new(self.body, self.read_offset())
    }

    /// Skip `n` bytes, rewind of negative
    fn skip(&mut self, n: i32) {
        self.offset += n;
    }

    fn read_u8(&mut self) -> u8 {
        let v = self.body[self.offset as usize];
        self.offset += 1;
        v
    }

    fn read_i32(&mut self) -> i32 {
        i32::from_le_bytes(self.read_slice(4).try_into().unwrap())
    }

    fn read_u32(&mut self) -> u32 {
        u32::from_le_bytes(self.read_slice(4).try_into().unwrap())
    }

    fn read_u64(&mut self) -> u64 {
        u64::from_le_bytes(self.read_slice(8).try_into().unwrap())
    }

    /// Read an offset value, return an absolute body offset
    fn read_offset(&mut self) -> i32 {
        let base_offset = self.offset;
        let offset = self.read_i32();
        base_offset + offset
    }

    fn peek_u32(&self) -> u32 {
        u32::from_le_bytes(self.peek_slice(4).try_into().unwrap())
    }
}

/// Same as [BodyCursor], but suited to read indexed fields from entry
///
/// The first two fields are always:
/// - the size of the field list itself
/// - the size of the entry (which is the end of the fields)
struct BodyFieldsCursor<'a> {
    body: &'a [u8],
    fields_offset: i32,
    entry_offset: i32,
}

impl<'a> BodyFieldsCursor<'a> {
    fn field_slice(&self, field: u8, n: i32) -> Option<&'a [u8]> {
        match self.field_offset(field) {
            0 => None,
            o => {
                let offset = self.entry_offset + o;
                Some(&self.body[offset as usize .. (offset + n) as usize])
            }
        }
    }

    /// Get field offset value
    fn field_offset(&self, field: u8) -> i32 {
        let offset = (self.fields_offset + 2 * field as i32) as usize;
        let slice = &self.body[offset .. offset + 2];
        u16::from_le_bytes(slice.try_into().unwrap()) as i32
    }

    fn get_i32(&self, field: u8) -> Option<i32> {
        self.field_slice(field, 4).map(|s| i32::from_le_bytes(s.try_into().unwrap()))
    }

    fn get_u32(&self, field: u8) -> Option<u32> {
        self.field_slice(field, 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()))
    }

    fn get_u64(&self, field: u8) -> Option<u64> {
        self.field_slice(field, 8).map(|s| u64::from_le_bytes(s.try_into().unwrap()))
    }

    /// Read an offset value, return a body cursor at this offset
    fn get_offset_cursor(&self, field: u8) -> Option<BodyCursor<'a>> {
        self.get_i32(field).map(|o| {
            let offset = self.entry_offset + o + self.field_offset(field);
            BodyCursor::new(self.body, offset)
        })
    }

    /// Read an offset value, then string at given offset
    fn get_str(&self, field: u8) -> Option<&'a str> {
        self.get_offset_cursor(field).map(|mut cursor| {
            let len = cursor.read_i32();
            let slice = cursor.read_slice(len);
            std::str::from_utf8(slice).expect("invalid UTF-8 string in RMAN")
        })
    }
}


/// An iterator over invidual entries of an RMAN table
///
/// This struct is created by the various `iter_*()` methods on [Rman].
pub struct OffsetTableIter<'a, I> {
    cursor: BodyCursor<'a>,
    count: u32,
    parser: fn(BodyCursor<'a>) -> I,
}

impl<'a, I> OffsetTableIter<'a, I> {
    /// Initialize the iterator, read item count from the cursor
    fn new(mut cursor: BodyCursor<'a>, parser: fn(BodyCursor<'a>) -> I) -> Self {
        let count = cursor.read_u32();
        Self { cursor, count, parser }
    }
}

impl<'a, I> Iterator for OffsetTableIter<'a, I> {
    type Item = I;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            None
        } else {
            self.count -= 1;
            Some((self.parser)(self.cursor.subcursor()))
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.count as usize, Some(self.count as usize))
    }

    fn count(self) -> usize {
        self.count as usize
    }
}


/// File flag defined in RMAN
///
/// Flags are locale codes (e.g. `en_US`) or platform (e.g. `macos`).
pub struct FileFlagEntry<'a> {
    /// Flag ID
    pub id: u8,
    /// Flag value
    pub flag: &'a str,
}


/// Bundle information from RMAN
pub struct BundleEntry<'a> {
    /// Bundle ID
    pub id: u64,
    cursor: BodyCursor<'a>,
}

impl<'a> BundleEntry<'a> {
    /// Iterate of bundle chunks
    pub fn iter_chunks(&self) -> impl Iterator<Item=ChunkEntry> + 'a {
        OffsetTableIter::new(self.cursor.clone(), parse_chunk_entry)
            .scan(0u32, |offset, mut e| {
                e.bundle_offset = *offset;
                *offset += e.bundle_size;
                Some(e)
            })
    }

    /// Return the number of chunks in the bundle
    pub fn chunks_count(&self) -> u32 {
       self.cursor.peek_u32()
    }
}

/// Chunk information from RMAN
pub struct ChunkEntry {
    /// Chunk ID
    pub id: u64,
    /// Size of chunk in bundle, compressed
    pub bundle_size: u32,
    /// Size of chunk, uncompressed
    pub target_size: u32,
    /// Offset of chunk in bundle
    pub bundle_offset: u32,
}

/// File information from RMAN
pub struct FileEntry<'a> {
    /// File ID
    pub id: u64,
    /// File name (without directory)
    pub name: &'a str,
    /// For links, target of the link
    pub link: Option<&'a str>,
    /// ID of the directory the file is into
    pub directory_id: Option<u64>,
    /// Size of the file, when extracted
    pub filesize: u32,
    /// Flags, used to filter which files need to be installed
    pub flags: Option<FileFlagSet>,
    chunks_cursor: BodyCursor<'a>,
}

/// Data byte range for an RMAN file
pub struct FileChunkRange {
    /// Byte range of the chunk in its bundle
    pub bundle: (u32, u32),
    /// Byte range of the chunk in the target file
    pub target: (u32, u32),
}

/// Chunk data information for an RMAN file
///
/// Store chunks of a file, grouped by bundle.
/// For each entry in the map, key is the bundle ID and value a list of chunk data ranges.
pub type FileBundleRanges = HashMap<u64, Vec<FileChunkRange>>;

impl<'a> FileEntry<'a> {
    /// Iterate on the chunks the file is built from
    pub fn iter_chunks(&self) -> FileChunksIter<'a> {
        FileChunksIter::new(self.chunks_cursor.clone())
    }

    /// Return full file path, using given directory path map
    pub fn path(&self, dirs: &DirPaths) -> String {
        match self.directory_id {
            None => self.name.to_owned(),
            Some(id) => format!("{}/{}", dirs[&id], self.name),
        }
    }

    /// Collect file chunks grouped by bundle, and the total file size
    pub fn bundle_chunks(&self, bundle_chunks: &BundleChunks) -> (u32, FileBundleRanges) {
        // Group chunks by bundle
        // For each bundle, get its list of ranges to download and target file ranges
        // Also compute the total file size
        let mut bundle_ranges = FileBundleRanges::new();
        let file_size = self
            .iter_chunks()
            .fold(0u32, |offset, chunk_id| {
                let chunk = &bundle_chunks[&chunk_id];
                let ranges = &mut bundle_ranges.entry(chunk.bundle_id).or_default();
                ranges.push(FileChunkRange {
                    bundle: (chunk.bundle_offset, chunk.bundle_offset + chunk.bundle_size),
                    target: (offset, offset + chunk.target_size),
                });
                offset + chunk.target_size
            });
        (file_size, bundle_ranges)
    }
}

/// An iterator over the chunks of an RMAN file
///
/// This `struct` is created by `FileEntry::iter_chunks` method.
pub struct FileChunksIter<'a> {
    cursor: BodyCursor<'a>,
    count: u32,
}

impl<'a> FileChunksIter<'a> {
    fn new(mut cursor: BodyCursor<'a>) -> Self {
        let count = cursor.read_u32();
        Self { cursor, count }
    }
}

impl<'a> Iterator for FileChunksIter<'a> {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            None
        } else {
            self.count -= 1;
            Some(self.cursor.read_u64())
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.count as usize, Some(self.count as usize))
    }

    fn count(self) -> usize {
        self.count as usize
    }
}


/// Set of RMAN file flags, as a bitmask
pub struct FileFlagSet {
    mask: u64,
}

impl FileFlagSet {
    /// Iterate on flags set in the mask
    pub fn iter<'a, I: Iterator<Item=&'a FileFlagEntry<'a>>>(&self, flags_it: I) -> impl Iterator<Item=&'a str> {
        let mask = self.mask;
        flags_it.filter_map(move |e| {
            if mask & (1 << e.id) == 0 {
                None
            } else {
                Some(e.flag)
            }
        })
    }
}


/// Directory defined in RMAN
pub struct DirectoryEntry<'a> {
    /// Directory ID
    pub id: u64,
    /// Parent directory, if any
    pub parent_id: Option<u64>,
    /// Directory name
    pub name: &'a str,
}

impl<'a> DirectoryEntry<'a> {
    /// Build absolute path, using list of all directories
    pub fn path(&self, dirs: &[DirectoryEntry]) -> String {
        let mut path = self.name.to_owned();
        let mut parent_id = self.parent_id;
        while parent_id.is_some() {
            let pid = parent_id.unwrap();
            let parent = dirs.iter().find(|e| e.id == pid).expect("RMAN parent directory ID not found");
            path = format!("{}/{}", parent.name, path);
            parent_id = parent.parent_id;
        }
        path
    }

    /// Resolve directory paths, return a map indexed by ID
    pub fn build_path_map(entries: &[DirectoryEntry]) -> DirPaths {
        // Note: don't process recursively. Path of intermediate directories will be formatted
        // multiple times. There are only few directories, so that should not be an issue.
        entries.iter().map(|e| (e.id, e.path(entries))).collect()
    }
}


fn parse_flag_entry(mut cursor: BodyCursor) -> FileFlagEntry {
    // Skip field offsets, assume fixed ones
    cursor.skip(4);
    cursor.skip(3);
    let flag_id = cursor.read_u8();
    let flag = {
        let mut cursor = cursor.subcursor();
        let len = cursor.read_i32();
        let slice = cursor.read_slice(len);
        std::str::from_utf8(slice).expect("invalid UTF-8 string for RMAN file flag")
    };
    FileFlagEntry { id: flag_id, flag }
}

fn parse_bundle_entry(cursor: BodyCursor) -> BundleEntry {
    // Field offsets
    //   0  bundle ID
    //   1  chunks offset
    let cursor = cursor.fields_cursor();

    let bundle_id = cursor.get_u64(0).expect("missing bundle ID field");
    let chunks_cursor = cursor.get_offset_cursor(1).expect("missing chunks offset field");

    BundleEntry { id: bundle_id, cursor: chunks_cursor }
}

fn parse_chunk_entry(cursor: BodyCursor) -> ChunkEntry {
    // Field offsets
    //   0  chunk ID
    //   1  bundle size, compressed
    //   2  chunk size, uncompressed

    let cursor = cursor.fields_cursor();

    let chunk_id = cursor.get_u64(0).expect("missing chunk ID field");
    let bundle_size = cursor.get_u32(1).expect("missing chunk compressed size");
    let target_size = cursor.get_u32(2).expect("missing chunk uncompressed size");

    // Note: bundle_offset is set later, by `BundleEntry::iter_chunks()`
    ChunkEntry { id: chunk_id, bundle_size, target_size, bundle_offset: 0 }
}

fn parse_file_entry(cursor: BodyCursor) -> FileEntry {
    // Field offsets
    //   0  file ID
    //   1  directory ID
    //   2  file size
    //   3  name (offset)
    //   4  flags (mask)
    //   5  ?
    //   6  ?
    //   7  chunks (offset)
    //   8  ?
    //   9  link (str, offset)
    //  10  ?
    //  11  ? (present and set to 1 for localized WADs)
    //  12  file type (1: executable, 2: regular)
    let cursor = cursor.fields_cursor();

    let file_id = cursor.get_u64(0).expect("missing file ID field");
    let directory_id = cursor.get_u64(1);
    let filesize = cursor.get_u32(2).expect("missing file size field");
    let name = cursor.get_str(3).expect("missing file name field");
    let flags = cursor.get_u64(4).map(|mask| FileFlagSet { mask });
    let chunks_cursor = cursor.get_offset_cursor(7).expect("missing chunks cursor field");
    let link = cursor.get_str(9).filter(|v| !v.is_empty());

    FileEntry {
        id: file_id, name, link, directory_id,
        filesize, flags, chunks_cursor,
    }
}

fn parse_directory_entry(cursor: BodyCursor) -> DirectoryEntry {
    let cursor = cursor.fields_cursor();
    let directory_id = cursor.get_u64(0).unwrap_or(0);
    let parent_id = cursor.get_u64(1);
    let name = cursor.get_str(2).expect("missing directory name field");

    DirectoryEntry { id: directory_id, parent_id, name }
}


/// Error in an RMAN file
#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum RmanError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("parsing error")]
    Parsing(#[from] ParseError),
    #[error("version not supported: {0}.{1}")]
    UnsupportedVersion(u8, u8),
    #[error("flags not supported: {0:b}")]
    UnsupportedFlags(u16),
}

