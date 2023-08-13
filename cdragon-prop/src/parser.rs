use std::any::Any;
use std::io::Read;
use nom::{
    number::complete::{le_u8, le_i8, le_u16, le_i16, le_u32, le_i32, le_u64, le_i64, le_f32},
    bytes::complete::{tag, take},
    combinator::{map, flat_map, opt},
    sequence::{pair, tuple},
    multi::count,
};
use super::{
    PropFile,
    BinEntry,
    BinEntryHeader,
    data::*,
    binvalue_map_keytype,
    binvalue_map_type,
};
use cdragon_utils::{
    hashes::HashDef,
    parsing::{ParseError, IResult, ReadArray},
    parse_buf,
};

type Result<T> = std::result::Result<T, ParseError>;


/// Trait satisfied by values that can be parsed from binary data
pub(super) trait BinParsable where Self: Sized {
    fn binparse(i: &[u8]) -> IResult<&[u8], Self>;
}

pub(super) fn binparse<T: BinParsable>(i: &[u8]) -> Result<T> {
    match T::binparse(i) {
        Ok((i, v)) => {
            if !i.is_empty() {
                Err(ParseError::TooMuchData)
            } else {
                Ok(v)
            }
        },
        Err(e) => Err(e.into())
    }
}

/// Similar to nom::multi::count, but get count from a parser
fn length_count<I, O1, O2, F, G>(f: F, g: G) -> impl Fn(I) -> IResult<I, Vec<O2>>
where
  I: Clone + PartialEq,
  F: Fn(I) -> IResult<I, O1>,
  G: Fn(I) -> IResult<I, O2>,
  O1: nom::ToUsize,
{
    move |i: I| {
        let (i, n) = f(i)?;
        let (i, v) = nom::multi::count(&g, n.to_usize())(i)?;
        Ok((i, v))
    }
}


macro_rules! impl_binparsable {
    ($type:ty, $expr:expr) => {
        impl BinParsable for $type {
            fn binparse(i: &[u8]) -> IResult<&[u8], Self> { $expr(i) }
        }
    };
    ($type:ty, =$parser:expr) => {
        impl_binparsable!($type, map($parser, |v| Self(v)));
    };
    ($type:ty, =>($($parser:expr),* $(,)?)) => {
        impl_binparsable!($type, map(tuple(($($parser,)*)), <$type>::from));
    };
}

impl BinParsable for PropFile {
    fn binparse(i: &[u8]) -> IResult<&[u8], Self> {
        // Parse header
        let (i, opt_ptch) = opt(tag("PTCH"))(i)?;
        let (i, is_patch) = match opt_ptch {
            Some(_) => {
                let (i, header) = tuple((le_u32, le_u32))(i)?;
                assert_eq!(header, (1, 0));
                (i, true)
            }
            None => (i, false)
        };

        let (i, (_, version)) = tuple((tag("PROP"), le_u32))(i)?;
        let (i, linked_files) =
            if version >= 2 {
                length_count(le_u32, parse_binstring)(i)?
            } else {
                (i, vec![])
            };

        let (i, entry_types) = length_count(le_u32, BinClassName::binparse)(i)?;
        // Parse entries
        let (i, entries) = {
            let (mut i, mut entries) = (i, Vec::<BinEntry>::with_capacity(entry_types.len()));
            for ctype in entry_types {
                i = {
                    let (i, entry) = parse_entry_from_type(i, ctype)?;
                    entries.push(entry);
                    i
                }
            }
            (i, entries)
        };

        Ok((i, Self { version, is_patch, linked_files, entries }))
    }
}


/// Scan entries from a bin file
pub struct BinEntryScanner<R: Read> {
    reader: R,
    htypes_iter: std::vec::IntoIter<BinClassName>,
    /// `true` if scanning a patch
    ///
    /// See [PropFile::is_patch] for details.
    pub is_patch: bool,
}

impl<R: Read> BinEntryScanner<R> {
    /// Create a scanner, parse the headers
    pub fn new(mut reader: R) -> Result<Self> {
        // Parse header
        let (is_patch, version): (bool, u32) = {
            let mut buf = [0u8; 4 + 4 + 4];  // maximum size needed
            reader.read_exact(&mut buf[..8])?;
            let is_patch = match parse_buf!(buf[..4], opt(tag("PTCH"))) {
                Some(_) => {
                    reader.read_exact(&mut buf[8..12])?;
                    let header = parse_buf!(buf[4..12], tuple((le_u32, le_u32)));
                    assert_eq!(header, (1, 0));
                    reader.read_exact(&mut buf[..8])?;
                    true
                }
                None => false
            };

            let (_, version) = parse_buf!(buf[..8], tuple((tag("PROP"), le_u32)));
            (is_patch, version)
        };

        if version >= 2 {
            // Skip linked files
            let buf = reader.read_array::<4>()?;
            let n = parse_buf!(buf, le_u32);
            for _ in 0..n {
                let buf = reader.read_array::<2>()?;
                let n = parse_buf!(buf, le_u16);
                std::io::copy(&mut reader.by_ref().take(n as u64), &mut std::io::sink())?;
            }
        };

        // Parse entry types
        let entry_types: Vec<BinClassName> = {
            let buf = reader.read_array::<4>()?;
            let n = parse_buf!(buf, le_u32);
            let mut buf = Vec::<u8>::new();
            reader.by_ref().take(4 * n as u64).read_to_end(&mut buf)?;
            let entry_types = parse_buf!(buf, count(BinClassName::binparse, n as usize));
            entry_types
        };

        Ok(Self { reader, htypes_iter: entry_types.into_iter(), is_patch })
    }

    /// Scan entries, allow to parse or skip each entry
    ///
    /// The result behaves provides `next()` but is not an `Iterator`.
    pub fn scan(self) -> BinEntryScanScan<R> {
        BinEntryScanScan {
            reader: self.reader,
            htypes_iter: self.htypes_iter,
            length: None,
        }
    }

    /// Scan entries, iterate on headers (path, type)
    pub fn headers(self) -> BinEntryScanHeaders<R> {
        BinEntryScanHeaders {
            reader: self.reader,
            htypes_iter: self.htypes_iter,
        }
    }

    /// Scan entries, parse filtered ones
    pub fn filter_parse<F>(self, f: F) -> BinEntryScanFilterParse<R, F>
    where F: Fn(BinEntryPath, BinClassName) -> bool {
        BinEntryScanFilterParse {
            reader: self.reader,
            htypes_iter: self.htypes_iter,
            filter: f,
        }
    }

    /// Parse entries, iterate on them
    pub fn parse(self) -> BinEntryScanParse<R> {
        BinEntryScanParse {
            reader: self.reader,
            htypes_iter: self.htypes_iter,
        }
    }
}

// Note: A trait alias would be better, but they are not available
/// Item type for entry scanning
pub type BinEntryScannerItem = Result<BinEntry>;


/// Common methods for BinEntryScanner iterators
trait BinEntryScan {
    type Reader: Read;
    type Output;

    /// Read the next entry header, return the remaining length and the path
    fn next_scan(reader: &mut Self::Reader) -> Result<(u32, BinEntryPath)> {
        let buf = reader.read_array::<{4 + 4}>()?;
        let (length, path) = parse_buf!(buf, tuple((le_u32, BinEntryPath::binparse)));
        Ok((length - 4, path))  // path has been read, deduct it from length
    }

    /// Read entry fields
    fn read_fields(reader: &mut Self::Reader, length: u32) -> Result<Vec<BinField>> {
        let mut buf = Vec::<u8>::new();
        reader.by_ref().take(length as u64).read_to_end(&mut buf)?;
        let fields = parse_buf!(buf, length_count(le_u16, BinField::binparse));
        Ok(fields)
    }

    /// Skip entry fields
    fn skip_fields(reader: &mut Self::Reader, length: u32) -> Result<()> {
        // There is no seek-like method implemented on &[u8]
        //reader.seek(SeekFrom::Current(length as i64))?;
        std::io::copy(&mut reader.by_ref().take(length as u64), &mut std::io::sink())?;
        Ok(())
    }

    fn next_result(&mut self, ctype: BinClassName) -> Result<Self::Output>;
}


pub struct BinEntryScanHeaders<R>
where R: Read {
    reader: R,
    htypes_iter: std::vec::IntoIter<BinClassName>,
}

impl<R: Read> BinEntryScan for BinEntryScanHeaders<R> {
    type Reader = R;
    type Output = (BinEntryPath, BinClassName);

    fn next_result(&mut self, ctype: BinClassName) -> Result<Self::Output> {
        let (length, path) = Self::next_scan(&mut self.reader)?;
        Self::skip_fields(&mut self.reader, length)?;
        Ok((path, ctype))
    }
}

impl<R: Read> Iterator for BinEntryScanHeaders<R> {
    type Item = Result<(BinEntryPath, BinClassName)>;

    fn next(&mut self) -> Option<Self::Item> {
        let ctype = self.htypes_iter.next()?;
        Some(self.next_result(ctype))
    }
}


pub struct BinEntryScanFilterParse<R, F>
where R: Read, F: Fn(BinEntryPath, BinClassName) -> bool {
    reader: R,
    htypes_iter: std::vec::IntoIter<BinClassName>,
    filter: F,
}

impl<R, F> BinEntryScan for BinEntryScanFilterParse<R, F>
where R: Read, F: Fn(BinEntryPath, BinClassName) -> bool {
    type Reader = R;
    type Output = Option<BinEntry>;

    fn next_result(&mut self, ctype: BinClassName) -> Result<Self::Output> {
        let (length, path) = Self::next_scan(&mut self.reader)?;
        if (self.filter)(path, ctype) {
            let fields = Self::read_fields(&mut self.reader, length)?;
            Ok(Some(BinEntry { path, ctype, fields }))
        } else {
            Self::skip_fields(&mut self.reader, length)?;
            Ok(None)
        }
    }
}

impl<R, F> Iterator for BinEntryScanFilterParse<R, F>
where R: Read, F: Fn(BinEntryPath, BinClassName) -> bool {
    type Item = BinEntryScannerItem;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let ctype = self.htypes_iter.next()?;
            match self.next_result(ctype) {
                Ok(None) => continue,
                Ok(Some(v)) => return Some(Ok(v)),
                Err(e) => return Some(Err(e)),
            }
        }
    }
}


pub struct BinEntryScanParse<R>
where R: Read {
    reader: R,
    htypes_iter: std::vec::IntoIter<BinClassName>,
}

impl<R: Read> BinEntryScan for BinEntryScanParse<R> {
    type Reader = R;
    type Output = BinEntry;

    fn next_result(&mut self, ctype: BinClassName) -> Result<Self::Output> {
        let (length, path) = Self::next_scan(&mut self.reader)?;
        let fields = Self::read_fields(&mut self.reader, length)?;
        Ok(BinEntry { path, ctype, fields })
    }
}

impl<R: Read> Iterator for BinEntryScanParse<R> {
    type Item = BinEntryScannerItem;

    fn next(&mut self) -> Option<Self::Item> {
        let ctype = self.htypes_iter.next()?;
        Some(self.next_result(ctype))
    }
}


// Iterator-like
//
// It does NOT implemented `Iterator` but behaves similarly.
pub struct BinEntryScanScan<R>
where R: Read {
    reader: R,
    length: Option<u32>,
    htypes_iter: std::vec::IntoIter<BinClassName>,
}

pub struct BinEntryScanItem<'a, R>
where R: Read {
    owner: &'a mut BinEntryScanScan<R>,
    pub path: BinEntryPath,
    pub ctype: BinClassName,
}

impl<'a, R> BinEntryScanItem<'a, R>
where R: Read {
    pub fn read(self) -> Result<BinEntry> {
        self.owner.read_entry(self.path, self.ctype)
    }
}


impl<R> BinEntryScan for BinEntryScanScan<R>
where R: Read {
    type Reader = R;
    type Output = (u32, BinEntryPath, BinClassName);

    fn next_result(&mut self, ctype: BinClassName) -> Result<Self::Output> {
        let (length, path) = Self::next_scan(&mut self.reader)?;
        Ok((length, path, ctype))
    }
}

impl<R> BinEntryScanScan<R>
where R: Read {
    pub fn next(&mut self) -> Option<Result<BinEntryScanItem<'_, R>>> {
        // Note: the entry is skipped and thus fails at the next iteration
        if let Some(length) = self.length.take() {
            if let Err(err) = Self::skip_fields(&mut self.reader, length) {
                return Some(Err(err));
            }
        }
        let ctype = self.htypes_iter.next()?;
        match self.next_result(ctype) {
            Ok((length, path, ctype)) => {
                self.length = Some(length);
                Some(Ok(BinEntryScanItem { owner: self, path, ctype }))
            }
            Err(err) => Some(Err(err)),
        }
        
    }

    fn read_entry(&mut self, path: BinEntryPath, ctype: BinClassName) -> Result<BinEntry> {
        // Double calls are not possible using public API
        let length = self.length.take().unwrap();
        let fields = Self::read_fields(&mut self.reader, length)?;
        Ok(BinEntry { path, ctype, fields })
    }
}



/// Parse a single BinEntry, starts at its header
fn parse_entry_from_type(i: &[u8], ctype: BinClassName) -> IResult<&[u8], BinEntry> {
    let (i, (_length, path)) = tuple((le_u32, BinEntryPath::binparse))(i)?;
    parse_entry_from_header(i, (path, ctype))
}

/// Parse a single BinEntry, starts before its field count
fn parse_entry_from_header(i: &[u8], (path, ctype): BinEntryHeader) -> IResult<&[u8], BinEntry> {
    map(length_count(le_u16, BinField::binparse),
        |fields| BinEntry { path, ctype, fields })(i)
}

fn parse_binstring(i: &[u8]) -> IResult<&[u8], String> {
    map(flat_map(le_u16, take), |s| std::str::from_utf8(s).expect("invalid UTF-8 string in BIN").to_string())(i)
}


impl BinParsable for BinField {
    fn binparse(i: &[u8]) -> IResult<&[u8], Self> {
        let (i, (name, vtype)) = tuple((BinFieldName::binparse, BinType::binparse))(i)?;
        let (i, value) = binvalue_map_type!(vtype, T, map(T::binparse, |v| { Box::new(v) as Box<dyn Any> })(i)?);
        Ok((i, Self { name, vtype, value }))
    }
}

impl_binparsable!(BinHashValue, map(le_u32, Self::from));
impl_binparsable!(BinEntryPath, map(le_u32, Self::from));
impl_binparsable!(BinClassName, map(le_u32, Self::from));
impl_binparsable!(BinFieldName, map(le_u32, Self::from));
impl_binparsable!(BinPathValue, map(le_u64, Self::from));

impl_binparsable!(BinNone, map(take(6usize), |_| Self()));
impl_binparsable!(BinBool, map(le_u8, |v| Self(v != 0u8)));
impl_binparsable!(BinS8, =le_i8);
impl_binparsable!(BinU8, =le_u8);
impl_binparsable!(BinS16, =le_i16);
impl_binparsable!(BinU16, =le_u16);
impl_binparsable!(BinS32, =le_i32);
impl_binparsable!(BinU32, =le_u32);
impl_binparsable!(BinS64, =le_i64);
impl_binparsable!(BinU64, =le_u64);
impl_binparsable!(BinFloat, =le_f32);
impl_binparsable!(BinVec2, =>(le_f32, le_f32));
impl_binparsable!(BinVec3, =>(le_f32, le_f32, le_f32));
impl_binparsable!(BinVec4, =>(le_f32, le_f32, le_f32, le_f32));
impl_binparsable!(BinColor, map(tuple((le_u8, le_u8, le_u8, le_u8)), |t| Self { r: t.0, g: t.1, b: t.2, a: t.3 }));
impl_binparsable!(BinMatrix, map(tuple((le_f32, le_f32, le_f32, le_f32,
                                           le_f32, le_f32, le_f32, le_f32,
                                           le_f32, le_f32, le_f32, le_f32,
                                           le_f32, le_f32, le_f32, le_f32)),
                                           |t| Self([
                                           [t.0, t.1, t.2, t.3],
                                           [t.4, t.5, t.6, t.7],
                                           [t.8, t.9, t.10, t.11],
                                           [t.12, t.13, t.14, t.15]])
                                           ));

impl BinParsable for BinList {
    fn binparse(i: &[u8]) -> IResult<&[u8], Self> {
        let (i, (vtype, _)) = tuple((BinType::binparse, le_u32))(i)?;
        let (i, values) = binvalue_map_type!(vtype, T, map(length_count(le_u32, T::binparse), |v| { Box::new(v) as Box<dyn Any> })(i)?);
        Ok((i, Self { vtype, values }))
    }
}

impl BinParsable for BinStruct {
    fn binparse(i: &[u8]) -> IResult<&[u8], Self> {
        let (i, ctype) = BinClassName::binparse(i)?;
        if ctype.is_null() {
            Ok((i, Self { ctype, fields: vec![] }))
        } else {
            let (i, (_, fields)) = tuple((le_u32, length_count(le_u16, BinField::binparse)))(i)?;
            Ok((i, Self { ctype, fields }))
        }
    }
}

impl BinParsable for BinEmbed {
    fn binparse(i: &[u8]) -> IResult<&[u8], Self> {
        let (i, ctype) = BinClassName::binparse(i)?;
        if ctype.is_null() {
            Ok((i, Self { ctype, fields: vec![] }))
        } else {
            let (i, (_, fields)) = tuple((le_u32, length_count(le_u16, BinField::binparse)))(i)?;
            Ok((i, Self { ctype, fields }))
        }
    }
}

impl BinParsable for BinOption {
    fn binparse(i: &[u8]) -> IResult<&[u8], Self> {
        let (i, vtype) = BinType::binparse(i)?;
        let (i, n) = le_u8(i)?;
        let (i, value) = match n {
            0 => (i, None),
            1 => {
                let (i, v) = binvalue_map_type!(vtype, T, map(T::binparse, |v| Box::new(v) as Box<dyn Any>)(i)?);
                (i, Some(v))
            }
            _ => panic!("unexpected option count: {}", n),
        };
        Ok((i, Self { vtype, value }))
    }
}

impl BinParsable for BinMap {
    fn binparse(i: &[u8]) -> IResult<&[u8], Self> {
        let (i, (ktype, vtype, _, n)) = tuple((BinType::binparse, BinType::binparse, le_u32, le_u32))(i)?;
        let (i, values) =
            binvalue_map_keytype!(
                ktype, K, binvalue_map_type!(
                    vtype, V, map(count(pair(K::binparse, V::binparse), n as usize), |v| {
                        let v: Vec<(K, V)> = v.into_iter().collect();
                        Box::new(v) as Box<dyn Any>
                    })(i)?));
        Ok((i, Self { ktype, vtype, values }))
    }
}

impl_binparsable!(BinHash, =BinHashValue::binparse);
impl_binparsable!(BinPath, =BinPathValue::binparse);
impl_binparsable!(BinLink, =BinEntryPath::binparse);
impl_binparsable!(BinFlag, map(le_u8, |v| Self(v != 0u8)));
impl_binparsable!(BinString, =parse_binstring);
impl_binparsable!(BinType, map(le_u8, |mut v| {
    if v >= 0x80 {
        v = v - 0x80 + BinType::List as u8;
    }
    Self::try_from(v).expect("invalid BIN type")
}));

