use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::collections::HashSet;
use cdragon_prop::{
    BinEntry,
    BinHashMappers,
    BinHashSets,
    BinTraversal,
    BinVisitor,
    data::*,
};
use cdragon_hashes::HashError;
use cdragon_utils::GuardedFile;

#[derive(Default)]
pub struct CollectHashesVisitor {
    pub hashes: BinHashSets,
}

impl CollectHashesVisitor {
    // Used to chain with `traverse_dir()`
    pub fn take_result(&mut self) -> BinHashSets {
        std::mem::take(&mut self.hashes)
    }
}

impl BinVisitor for CollectHashesVisitor {
    type Error = ();

    // Note: Don't collect WAD paths (BinPath)

    fn visit_type(&mut self, btype: BinType) -> bool {
        btype == BinType::Hash || btype == BinType::Link || btype.is_nested()
    }

    fn visit_entry(&mut self, value: &BinEntry) -> Result<bool, ()> {
        self.hashes.entry_path.insert(value.path.hash);
        self.hashes.class_name.insert(value.ctype.hash);
        Ok(true)
    }

    fn visit_field(&mut self, value: &BinField) -> Result<bool, ()> {
        self.hashes.field_name.insert(value.name.hash);
        Ok(self.visit_type(value.vtype))
    }

    fn visit_hash(&mut self, value: &BinHash) -> Result<(), ()> {
        self.hashes.hash_value.insert(value.0.hash);
        Ok(())
    }

    fn visit_struct(&mut self, value: &BinStruct) -> Result<bool, ()> {
        self.hashes.class_name.insert(value.ctype.hash);
        Ok(true)
    }

    fn visit_embed(&mut self, value: &BinEmbed) -> Result<bool, ()> {
        self.hashes.class_name.insert(value.ctype.hash);
        Ok(true)
    }

    fn visit_link(&mut self, value: &BinLink) -> Result<(), ()> {
        self.hashes.entry_path.insert(value.0.hash);
        Ok(())
    }
}

fn unknown_path(kind: BinHashKind) -> &'static str {
    match kind {
        BinHashKind::EntryPath => "unknown.binentries.txt",
        BinHashKind::ClassName => "unknown.bintypes.txt",
        BinHashKind::FieldName => "unknown.binfields.txt",
        BinHashKind::HashValue => "unknown.binhashes.txt",
    }
}

fn load_unknown_file<P: AsRef<Path>>(path: P) -> Result<HashSet<u32>, HashError> {
    let file = File::open(&path)?;
    let reader = BufReader::new(file);
    reader.lines()
        .map(|line| -> Result<u32, HashError> {
            line.map_err(HashError::Io).and_then(|line| {
                let line = line.trim_end();
                u32::from_str_radix(line, 16).map_err(|_| HashError::InvalidHashLine(line.to_owned()))
            })
        })
        .collect()
}

/// Load unknown hashes from text files in a directory
pub fn load_unknown(path: PathBuf) -> Result<BinHashSets, HashError> {
    let mut unknown = BinHashSets::default();
    for &kind in &BinHashKind::VARIANTS {
        *unknown.get_mut(kind) = load_unknown_file(path.join(unknown_path(kind)))?;
    }
    Ok(unknown)
}

/// Write (unknown) hashes to text files in a directory
pub fn write_unknown(path: PathBuf, hashes: &BinHashSets) -> Result<(), HashError> {
    std::fs::create_dir_all(&path)?;
    for &kind in &BinHashKind::VARIANTS {
        GuardedFile::for_scope(path.join(unknown_path(kind)), |file| {
            let mut writer = BufWriter::new(file);
            for hash in hashes.get(kind).iter() {
                writeln!(writer, "{:08x}", hash)?;
            }
            Ok(())
        })?;
    }
    Ok(())
}

/// Remove known hashes from `BinHashSets`
pub fn remove_known_from_unknown(unknown: &mut BinHashSets, hmappers: &BinHashMappers) {
    for &kind in &BinHashKind::VARIANTS {
        let mapper = hmappers.get(kind);
        unknown.get_mut(kind).retain(|h| !mapper.is_known(*h));
    }
}



#[derive(Default)]
pub struct CollectStringsVisitor {
    pub strings: HashSet<String>,
}

impl CollectStringsVisitor {
    // Used to chain with `traverse_dir()`
    pub fn take_result(&mut self) -> HashSet<String> {
        std::mem::take(&mut self.strings)
    }
}

impl BinVisitor for CollectStringsVisitor {
    type Error = ();

    fn visit_type(&mut self, btype: BinType) -> bool {
        btype == BinType::String || btype.is_nested()
    }

    fn visit_string(&mut self, value: &BinString) -> Result<(), ()> {
        if !self.strings.contains(&value.0) {
            self.strings.insert(value.0.clone());
        }
        Ok(())
    }
}


/// Visitor to search entries containing a given bin value (hash, string, ...)
#[derive(Default)]
pub struct SearchBinValueVisitor<T, F: FnMut(&BinEntry)> {
    pattern: T,
    on_match: F,
    matched: bool,
}

impl<T, F: FnMut(&BinEntry)> SearchBinValueVisitor<T, F> {
    pub fn new(pattern: T, on_match: F) -> Self {
        Self { pattern, on_match, matched: false }
    }
}

macro_rules! impl_search_bin_value_visitor {
    ($typ:ty, $visit_func:ident) => {
        impl<F: FnMut(&BinEntry)> BinVisitor for SearchBinValueVisitor<$typ, F> {
            type Error = ();

            fn traverse_entry(&mut self, entry: &BinEntry) -> Result<(), ()> {
                self.matched = false;
                entry.traverse_bin(self)?;
                if self.matched {
                    (self.on_match)(entry);
                }
                Ok(())
            }

            fn visit_type(&mut self, btype: BinType) -> bool {
                !self.matched && (btype == <$typ as BinValue>::TYPE || btype.is_nested())
            }

            fn $visit_func(&mut self, value: &$typ) -> Result<(), ()> {
                if value == &self.pattern {
                    self.matched = true;
                }
                Ok(())
            }
        }
    }
}

impl_search_bin_value_visitor!(BinString, visit_string);
impl_search_bin_value_visitor!(BinHash, visit_hash);
impl_search_bin_value_visitor!(BinLink, visit_link);


pub struct HashesMatchingEntriesVisitor<'a> {
    mappers: &'a BinHashMappers,
    types_seen: HashSet<BinClassName>,
    hashes_seen: HashSet<BinHashValue>,
    current_entry: Option<(BinEntryPath, BinClassName)>,
}

impl<'a> HashesMatchingEntriesVisitor<'a> {
    pub fn new(mappers: &'a BinHashMappers) -> Self {
        Self {
            mappers,
            types_seen: HashSet::default(),
            hashes_seen: HashSet::default(),
            current_entry: None,
        }
    }
}

impl<'a> BinVisitor for HashesMatchingEntriesVisitor<'a> {
    type Error = ();

    fn visit_type(&mut self, btype: BinType) -> bool {
        self.current_entry.is_some() && btype == BinType::Hash || btype.is_nested()
    }

    fn visit_entry(&mut self, value: &BinEntry) -> Result<bool, ()> {
        // Note: each type is checked only once
        // Even if the first entry does not cover all uses of hashes
        if self.types_seen.insert(value.ctype) {
            self.current_entry = Some((value.path, value.ctype));
            self.hashes_seen.clear();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn visit_hash(&mut self, value: &BinHash) -> Result<(), ()> {
        if self.hashes_seen.insert(value.0) {
            if !self.mappers.hash_value.is_known(value.0.hash) && self.mappers.entry_path.is_known(value.0.hash) {
                let (path, htype) = self.current_entry.unwrap();
                println!("type {} , path {} , hash {:x}",
                    htype.seek_str(self.mappers),
                    path.seek_str(self.mappers),
                    value.0);
            }
        }
        Ok(())
    }
}

