//! Various tools

use std::hash::Hash;
use std::path::{Path, PathBuf};
use num_traits::Num;
use walkdir::{WalkDir, DirEntry};
use cdragon_prop::{
    BinVisitor,
    PropError,
    PropFile,
};
use cdragon_utils::hashes::HashMapper;


/// Match strings against pattern with `*` wildcards
pub struct PathPattern<'a> {
    prefix: &'a str,
    suffix: Option<&'a str>,
    parts: Vec<&'a str>,
}

impl<'a> PathPattern<'a> {
    pub fn new(pattern: &'a str) -> Self {
        let mut it = pattern.split('*');
        let prefix = it.next().unwrap();  // `pattern` cannot be empty
        let mut parts: Vec<&str> = it.collect();
        let suffix = parts.pop();
        Self { prefix, suffix, parts }
    }

    pub fn is_match(&self, mut s: &str) -> bool {
        // No suffix means no `*`, compare the whole string
        if self.suffix.is_none() {
            return self.prefix == s;
        }

        // Prefix and suffix must match
        if !s.starts_with(self.prefix) {
            return false;
        }
        s = &s[self.prefix.len()..];
        if !s.ends_with(self.suffix.unwrap()) {
            return false;
        }
        s = &s[.. s.len() - self.suffix.unwrap().len()];

        // Find parts, one after the other
        for part in self.parts.iter() {
            s = match s.find(part) {
                None => return false,
                Some(i) => &s[i + part.len() ..],
            };
        }
        true
    }
}

/// Match hash value against pattern
///
/// Pattern can be the hex representation of a hash value or a string pattern with `*` wildcards.
pub enum HashValuePattern<'a, T: Num + Eq + Hash + Copy> {
    Hash(T),
    Path(PathPattern<'a>),
}

impl<'a, T: Num + Eq + Hash + Copy> HashValuePattern<'a, T> {
    pub fn new(pattern: &'a str) -> Self {
        // If pattern matches a hash value, consider it's a hash
        if pattern.len() == HashMapper::<T>::HASH_LEN {
            if let Ok(hash) = T::from_str_radix(pattern, 16) {
                return Self::Hash(hash);
            }
        }

        // Otherwise, parse as a path pattern
        Self::Path(PathPattern::new(pattern))
    }

    pub fn is_match(&self, hash: T, mapper: &HashMapper<T>) -> bool {
        match self {
            Self::Hash(h) => hash == *h,
            Self::Path(pattern) => {
                if let Some(path) = mapper.get(hash) {
                    pattern.is_match(path)
                } else {
                    false
                }
            }
        }
    }
}


/// Canonicalize a path, avoid errors on long file names
///
/// `canonicalize()` is needed to open long files on Windows, but it still fails if the path is too
/// long. `canonicalize()` the directory name then manually join the file name.
pub fn canonicalize_path(path: &Path) -> std::io::Result<PathBuf> {
    if cfg!(target_os = "windows") {
        if let Some(mut parent) = path.parent() {
            if let Some(base) = path.file_name() {
                if parent.as_os_str() == "" {
                    parent = Path::new(".");
                }
                return Ok(parent.canonicalize()?.join(base))
            }
        }
    }
    Ok(path.to_path_buf())
}


fn is_binfile_direntry(entry: &DirEntry) -> bool {
    let ftype = entry.file_type();
    if ftype.is_file() {
        if entry.path().extension().map(|s| s == "bin").unwrap_or(false) {
            // Some files are not actual 'PROP' files
            entry.file_name().to_str()
                .map(|s| !cdragon_prop::NON_PROP_BASENAMES.contains(&s))
                .unwrap_or(false)
        } else {
            false
        }
    } else {
        ftype.is_dir()
    }
}

/// Iterate on bin files from a directory
pub fn bin_files_from_dir<P: AsRef<Path>>(root: P) -> impl Iterator<Item=PathBuf> {
    WalkDir::new(&root)
        .into_iter()
        .filter_entry(is_binfile_direntry)
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| canonicalize_path(&e.into_path()).ok())
}


/// Trait to visit a directory using a BinVisitor
pub trait BinDirectoryVisitor: BinVisitor<Error=()> {
    fn traverse_dir<P: AsRef<Path>>(&mut self, root: P) -> Result<&mut Self, PropError> {
        for path in bin_files_from_dir(root) {
            let scanner = PropFile::scan_entries_from_path(path)?;
            for entry in scanner.parse() {
                self.traverse_entry(&entry?).unwrap();  // never fails
            }
        }
        Ok(self)
    }
}

impl<T> BinDirectoryVisitor for T where T: BinVisitor<Error=()> + ?Sized {}

