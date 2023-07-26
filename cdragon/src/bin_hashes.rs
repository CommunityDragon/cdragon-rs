use std::path::Path;
use std::collections::HashSet;
use cdragon_prop::{
    BinHashSets,
    BinEntry,
    PropError,
    PropFile,
    data::*,
    visitor::{BinVisitor, BinTraversal},
};
use super::bin_files_from_dir;

#[derive(Default)]
struct CollectHashesVisitor {
    hashes: BinHashSets,
}

impl BinVisitor for CollectHashesVisitor {
    // Note: Don't collect WAD paths (BinPath)

    fn visit_type(&mut self, btype: BinType) -> bool {
        match btype {
            BinType::Hash |
            BinType::List | BinType::List2 |
            BinType::Struct |
            BinType::Embed |
            BinType::Link |
            BinType::Option |
            BinType::Map => true,
            _ => false,
        }
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


#[derive(Default)]
struct CollectStringsVisitor {
    strings: HashSet<String>,
}

impl BinVisitor for CollectStringsVisitor {
    fn visit_type(&mut self, btype: BinType) -> bool {
        match btype {
            BinType::String |
            BinType::List | BinType::List2 |
            BinType::Struct |
            BinType::Embed |
            BinType::Option |
            BinType::Map => true,
            _ => false,
        }
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

