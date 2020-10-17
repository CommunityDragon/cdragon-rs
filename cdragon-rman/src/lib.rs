//! Support of RMAN files

use std::io::{Read, BufReader};
use std::path::Path;
use std::convert::TryInto;
use std::collections::HashMap;
use nom::{
    number::complete::{le_u8, le_u16, le_u32, le_u64},
    bytes::complete::tag,
    sequence::tuple,
};
use cdragon_utils::Result;
use cdragon_utils::parsing::{ParseError, into_err};
use cdragon_utils::locale::Locale;


/// Riot manifest file
///
/// The body is decompressed and parsed on demand.
/// Entries are parsed each time they are iterated on.
/// They should be cached by the caller if needed
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
    offset_locales: i32,
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
            offset_locales: offsets.1,
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
            let mut buf = [0u8; MAGIC_VERSION_LEN];
            reader.read_exact(&mut buf)?;
            let (_, (_, major, minor)) = tuple((tag("RMAN"), le_u8, le_u8))(&buf).map_err(into_err)?;
            if (major, minor) != (2, 0) {
                return Err(ParseError::InvalidData(format!("unsupported version: {}.{}", major, minor)).into());
            }
            (major, minor)
        };

        let (flags, manifest_id, zstd_length) = {
            let mut buf = [0u8; FIELDS_LEN];
            reader.read_exact(&mut buf)?;
            let (_, (flags, offset, zstd_length, manifest_id, _body_length)) =
                tuple((le_u16, le_u32, le_u32, le_u64, le_u32))(&buf).map_err(into_err)?;
            if flags & (1 << 9) == 0 {
                return Err(ParseError::InvalidData(format!("unsupported flags: {:b}", flags)).into());
            }
            if offset < HEADER_LEN as u32 {
                return Err(ParseError::InvalidData(format!("invalid body offset (too short): {}", offset)).into());
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

    /// Iterate on locale codes
    pub fn iter_locales(&self) -> OffsetTableIter<'_, LocaleEntry> {
        let cursor = BodyCursor::new(&self.body, self.offset_locales);
        OffsetTableIter::new(cursor, parse_locale_entry)
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
/// Body size is guaranteed to fits in a u32, and should always fit in a i32.
/// Use `i32` for all offsets to simplify use and void numerous casts.
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
        let fields_offset = entry_offset - self.read_i32();
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

/// Same as BodyCursor, but suited to read indexed fields from entry
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
                let offset = self.entry_offset + o as i32;
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

    /// Read an offset value, return an absolute body offset
    fn get_offset(&self, field: u8) -> Option<i32> {
        self.get_i32(field).map(|o| self.entry_offset + o + self.field_offset(field))
    }

    /// Read an offset value, then string at given offset
    fn get_str(&self, field: u8) -> Option<&'a str> {
        self.get_offset(field).map(|o| {
            let mut cursor = BodyCursor::new(self.body, o);
            let len = cursor.read_i32();
            let slice = cursor.read_slice(len);
            std::str::from_utf8(slice).expect("invalid UTF-8 string in RMAN")
        })
    }

    fn get_subcursor(&self, field: u8) -> BodyCursor<'a> {
        let offset = self.field_offset(field);
        BodyCursor::new(self.body, self.entry_offset + offset)
    }

}


/// An iterator over invidual entries of an RMAN table
///
/// This `struct` is created by the various `iter_*()` methods on `Rman`.
pub struct OffsetTableIter<'a, I> {
    cursor: BodyCursor<'a>,
    count: u32,
    parser: fn(BodyCursor<'a>) -> I,
}

impl<'a, I> OffsetTableIter<'a, I> {
    /// Initialize the iterator, read count from the cursor
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


/// Locale defined in RMAN
pub struct LocaleEntry {
    /// Locale ID
    pub id: u8,
    /// Locale 
    pub locale: Locale,
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
    /// Offset of chunk in bundle
    pub bundle_offset: u32,
    /// Size of chunk in bundle, compressed
    pub bundle_size: u32,
    /// Size of chunk, uncompressed
    pub target_size: u32,
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
    /// For localized files, locales for which it is used
    pub locales: Option<LocaleSet>,
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
                let ranges = &mut bundle_ranges.entry(chunk.bundle_id).or_insert(Default::default());
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


/// Set of locales, as a bitmask
pub struct LocaleSet {
    mask: u64,
}

impl LocaleSet {
    /// Iterate on locales set in the mask
    pub fn iter<'a, I: Iterator<Item=&'a LocaleEntry>>(&self, locales_it: I) -> impl Iterator<Item=&'a Locale> {
        let mask = self.mask;
        locales_it.filter_map(move |e| {
            if mask & (1 << e.id) == 0 {
                None
            } else {
                Some(&e.locale)
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


fn parse_locale_entry(mut cursor: BodyCursor) -> LocaleEntry {
    // Skip field offsets, assume fixed ones
    cursor.skip(4);
    cursor.skip(3);
    let locale_id = cursor.read_u8();
    let locale = {
        let mut cursor = cursor.subcursor();
        let len = cursor.read_i32();
        let slice = cursor.read_slice(len);
        Locale::from_bytes(slice).expect("invalid locale in RMAN")
    };
    LocaleEntry { id: locale_id, locale }
}

fn parse_bundle_entry(mut cursor: BodyCursor) -> BundleEntry {
    // Skip field offsets, assume fixed ones
    cursor.skip(4);
    let header_size = cursor.read_i32();
    let bundle_id = cursor.read_u64();
    // skip remaining header part, if any
    cursor.skip(header_size - 12);

    BundleEntry { id: bundle_id, cursor }
}

fn parse_chunk_entry(mut cursor: BodyCursor) -> ChunkEntry {
    // Skip field offsets, assume fixed ones
    cursor.skip(4);
    let bundle_size = cursor.read_u32();
    let target_size = cursor.read_u32();
    let chunk_id = cursor.read_u64();

    // Note: bundle_offset is set later, by `BundleEntry::iter_chunks()`
    ChunkEntry { id: chunk_id, bundle_offset: 0, bundle_size, target_size }
}

fn parse_file_entry(cursor: BodyCursor) -> FileEntry {
    // Get field offsets
    //   0  size of field list, including this field
    //   1  end of fields (chunks list)
    //   2  file ID
    //   3  directory ID
    //   4  file size
    //   5  name (offset)
    //   6  locales (mask)
    //   7  ?
    //   8  ?
    //   9  ? (related to entry size)
    //  10  ?
    //  11  link (offset)
    //  12  ?
    //  13  ? (present and set to 1 for localized WADs)
    //  14  file type (1: executable, 2: regular)

    let cursor = cursor.fields_cursor();
    assert!(cursor.field_offset(0) >= 14);

    let file_id = cursor.get_u64(2).expect("missing file ID field");
    let directory_id = cursor.get_u64(3);
    let filesize = cursor.get_u32(4).expect("missing file size field");
    let name = cursor.get_str(5).expect("missing file name field");
    let locales = cursor.get_u64(6).map(|mask| LocaleSet { mask });
    let link = cursor.get_str(11).filter(|v| v.len() != 0);
    let chunks_cursor = cursor.get_subcursor(1);

    FileEntry {
        id: file_id, name, link, directory_id,
        filesize, locales, chunks_cursor,
    }
}

fn parse_directory_entry(cursor: BodyCursor) -> DirectoryEntry {
    let cursor = cursor.fields_cursor();
    let directory_id = cursor.get_u64(2).unwrap_or(0);
    let parent_id = cursor.get_u64(3);
    let name = cursor.get_str(4).expect("missing directory name field");

    DirectoryEntry { id: directory_id, parent_id, name }
}

