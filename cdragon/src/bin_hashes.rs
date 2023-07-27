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
    PropError,
    PropFile,
    data::*,
};
use cdragon_utils::hashes::HashError;
use super::bin_files_from_dir;

#[derive(Default)]
struct CollectHashesVisitor {
    hashes: BinHashSets,
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

/// Collect hashes from a directory
pub fn collect_unknown_from_dir<P: AsRef<Path>>(root: P) -> Result<BinHashSets, PropError> {
    let mut visitor = CollectHashesVisitor::default();
    for path in bin_files_from_dir(root) {
        let scanner = PropFile::scan_entries_from_path(path)?;
        for entry in scanner.parse() {
            entry?.traverse_bin(&mut visitor);
        }
    }
    Ok(visitor.hashes)
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
struct CollectStringsVisitor {
    strings: HashSet<String>,
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

/// Collect strings from a directory
pub fn collect_strings_from_dir<P: AsRef<Path>>(root: P) -> Result<HashSet<String>, PropError> {
    let mut visitor = CollectStringsVisitor::default();
    for path in bin_files_from_dir(root) {
        let scanner = PropFile::scan_entries_from_path(path)?;
        for entry in scanner.parse() {
            entry?.traverse_bin(&mut visitor);
        }
    }
    Ok(visitor.strings)
}

