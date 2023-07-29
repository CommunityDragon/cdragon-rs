use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::collections::HashSet;
use cdragon_prop::{
    BinEntry,
    BinHashMappers,
    BinHashSets,
    BinVisitor,
    data::*,
};
use cdragon_utils::hashes::HashError;

#[derive(Default)]
pub struct CollectHashesVisitor {
    pub hashes: BinHashSets,
}

impl BinVisitor for CollectHashesVisitor {
    // Note: Don't collect WAD paths (BinPath)

    fn visit_type(&mut self, btype: BinType) -> bool {
        matches!(btype,
            BinType::Hash |
            BinType::List |
            BinType::List2 |
            BinType::Struct |
            BinType::Embed |
            BinType::Link |
            BinType::Option |
            BinType::Map)
    }

    fn visit_entry(&mut self, value: &BinEntry) -> bool {
        self.hashes.entry_path.insert(value.path.hash);
        self.hashes.class_name.insert(value.ctype.hash);
        true
    }

    fn visit_field(&mut self, value: &BinField) -> bool {
        self.hashes.field_name.insert(value.name.hash);
        self.visit_type(value.vtype)
    }

    fn visit_hash(&mut self, value: &BinHash) {
        self.hashes.hash_value.insert(value.0.hash);
    }

    fn visit_struct(&mut self, value: &BinStruct) -> bool {
        self.hashes.class_name.insert(value.ctype.hash);
        true
    }

    fn visit_embed(&mut self, value: &BinEmbed) -> bool {
        self.hashes.class_name.insert(value.ctype.hash);
        true
    }

    fn visit_link(&mut self, value: &BinLink) {
        self.hashes.entry_path.insert(value.0.hash);
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
    for kind in BinHashKind::variants() {
        *unknown.get_mut(kind) = load_unknown_file(path.join(unknown_path(kind)))?;
    }
    Ok(unknown)
}

/// Write (unknown) hashes to text files in a directory
pub fn write_unknown(path: PathBuf, hashes: &BinHashSets) -> Result<(), HashError> {
    std::fs::create_dir_all(&path)?;
    for kind in BinHashKind::variants() {
        let file = File::create(path.join(unknown_path(kind)))?;
        let mut writer = BufWriter::new(file);
        for hash in hashes.get(kind).iter() {
            writeln!(writer, "{:08x}", hash)?;
        }
    }
    Ok(())
}

/// Remove known hashes from `BinHashSets`
pub fn remove_known_from_unknown(unknown: &mut BinHashSets, hmappers: &BinHashMappers) {
    for kind in BinHashKind::variants() {
        let mapper = hmappers.get(kind);
        unknown.get_mut(kind).retain(|h| !mapper.is_known(*h));
    }
}



#[derive(Default)]
pub struct CollectStringsVisitor {
    pub strings: HashSet<String>,
}

impl BinVisitor for CollectStringsVisitor {
    fn visit_type(&mut self, btype: BinType) -> bool {
        matches!(btype,
            BinType::String |
            BinType::List |
            BinType::List2 |
            BinType::Struct |
            BinType::Embed |
            BinType::Option |
            BinType::Map)
    }

    fn visit_string(&mut self, value: &BinString) {
        if !self.strings.contains(&value.0) {
            self.strings.insert(value.0.clone());
        }
    }
}


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
    fn visit_type(&mut self, btype: BinType) -> bool {
        self.current_entry.is_some() &&
        matches!(btype,
            BinType::Hash |
            BinType::List |
            BinType::List2 |
            BinType::Struct |
            BinType::Embed |
            BinType::Option |
            BinType::Map)
    }

    fn visit_entry(&mut self, value: &BinEntry) -> bool {
        // Note: each type is checked only once
        // Even if the first entry does not cover all uses of hashes
        if self.types_seen.insert(value.ctype) {
            self.current_entry = Some((value.path, value.ctype));
            self.hashes_seen.clear();
            true
        } else {
            false
        }
    }

    fn visit_hash(&mut self, value: &BinHash) {
        if self.hashes_seen.insert(value.0) {
            if !self.mappers.hash_value.is_known(value.0.hash) && self.mappers.entry_path.is_known(value.0.hash) {
                let (path, htype) = self.current_entry.unwrap();
                println!("type {} , path {} , hash {:x}",
                    htype.try_str(self.mappers),
                    path.try_str(self.mappers),
                    value.0);
            }
        }
    }
}

