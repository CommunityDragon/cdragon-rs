//! Tools shared by different subcommands
use std::io;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use num_traits::Num;
use walkdir::{WalkDir, DirEntry};
use cdragon_prop::{
    is_binfile_path,
    BinHashMappers,
    JsonSerializer,
    TextTreeSerializer,
    BinSerializer,
    BinEntriesSerializer,
};
use cdragon_hashes::HashMapper;

pub type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;


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
        if pattern.len() == std::mem::size_of::<T>() * 2 {
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
fn canonicalize_path(path: &Path) -> std::io::Result<PathBuf> {
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
        is_binfile_path(entry.path())
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


/// Create bin entry serializer
pub fn build_bin_entry_serializer<'a, W: io::Write>(writer: &'a mut W, hmappers: &'a BinHashMappers, json: bool) -> io::Result<Box<dyn BinEntriesSerializer + 'a>> {
    if json {
        Ok(Box::new(JsonSerializer::new(writer, hmappers).write_entries()?))
    } else {
        Ok(Box::new(TextTreeSerializer::new(writer, hmappers).write_entries()?))
    }
}

