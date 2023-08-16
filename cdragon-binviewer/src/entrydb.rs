use std::fmt;
use std::io::BufRead;
use std::collections::HashMap;
use gloo_console::debug;
use regex::{RegexSet, RegexSetBuilder};
use byteorder::{LittleEndian, ReadBytesExt};
use cdragon_hashes::{
    HashDef,
    bin::binhash_from_str,
};
use cdragon_prop::{
    BinEntryPath,
    BinClassName,
    BinHashMappers,
};
use crate::Result;


#[derive(Debug)]
pub enum EntryDbError {
    InvalidSearchPattern(regex::Error),
}

impl fmt::Display for EntryDbError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            EntryDbError::InvalidSearchPattern(_) => write!(f, "invalid search pattern"),
        }
    }
}

impl std::error::Error for EntryDbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EntryDbError::InvalidSearchPattern(e) => Some(e),
        }
    }
}


/// Store entry information, provide search methods
#[derive(Default)]
pub struct EntryDatabase {
    /// Associate entry hash to its type and file's index filenames
    entries: HashMap<BinEntryPath, (BinClassName, usize)>,
    types: Vec<BinClassName>,
    filenames: Vec<String>,
}

impl EntryDatabase {
    /// Load a database from a stream
    pub fn load<R: BufRead>(mut r: R) -> Result<Self> {
        macro_rules! read_u32 {
            ($r:expr) => ($r.read_u32::<LittleEndian>())
        }
        macro_rules! read_u32_into {
            ($r:expr, $data:expr) => ($r.read_u32_into::<LittleEndian>($data))
        }

        // Read filenames
        let filenames = {
            let len = read_u32!(r)? as usize;
            // Note: using a `Vec<Box<[str]>>` would save few bytes per file
            let mut filenames = Vec::<String>::with_capacity(len);
            for _ in 0..len {
                let mut s = String::new();
                // `read_line()` will return 0 on EOF
                // We could check it after each read... or just wait it to fail when reading
                // the following data.
                r.read_line(&mut s)?;
                s.pop();  // remove trailing LF
                filenames.push(s);
            }
            filenames
        };

        // Read types
        let types: Vec<BinClassName> = {
            let len = read_u32!(r)? as usize;
            // Note: this would be better to directly parse values as BinClassName
            // or to avoid initialization by other means.
            // However, `types` is small so that's not really a problem.
            let mut data = vec![0u32; len];
            read_u32_into!(r, &mut data)?;
            data.iter().map(|v| BinClassName::from(*v)).collect()
        };

        // Load entries
        let entries = {
            let len = read_u32!(r)? as usize;
            let mut entries = HashMap::<BinEntryPath, (BinClassName, usize)>::with_capacity(len);
            for _ in 0..len {
                let mut data = [0u32; 3];
                read_u32_into!(r, &mut data)?;
                entries.insert(
                    BinEntryPath::from(data[0]),
                    (BinClassName::from(data[1]), data[2] as usize));
            }
            entries
        };

        debug!(format!("entry database loaded ({} entries, {} types, {} files)",
            entries.len(), types.len(), filenames.len()));

        Ok(Self { entries, types, filenames })
    }

    /// Return true if entry exists
    pub fn has_entry(&self, hash: BinEntryPath) -> bool {
        self.entries.contains_key(&hash)
    }

    /// Get an entry type and file index
    pub fn get_entry(&self, hash: BinEntryPath) -> Option<(BinClassName, usize)> {
        self.entries.get(&hash).copied()
    }

    /// Get a file path from its index
    pub fn get_filename(&self, ifile: usize) -> Option<&String> {
        self.filenames.get(ifile)
    }

    /// Return the number of entries
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Run a "smart" search on words
    pub fn search_words<'a>(&'a self, words: &'a [&str], mappers: &'a BinHashMappers) -> Result<impl Iterator<Item=BinEntryPath> + 'a> {
        #[derive(Default)]
        struct MergedCriteria<'a> {
            entry_paths: Vec<&'a str>,
            entry_hpaths: Vec<BinEntryPath>,
            entry_types: Vec<BinClassName>,
            file_suffixes: Vec<String>,
            excluded_entry_types: Vec<BinClassName>,
            excluded_entry_paths: Vec<&'a str>,
        }

        let mut criterias = MergedCriteria::default();
        for criteria in words.iter().map(|w| self.parse_criteria(w)) {
            match criteria {
                SearchCriteria::EntryPath(s) => criterias.entry_paths.push(s),
                SearchCriteria::EntryPathHash(h) => criterias.entry_hpaths.push(h),
                SearchCriteria::EntryType(h) => criterias.entry_types.push(h),
                SearchCriteria::FilePath(s) => {
                    let mut suffix = format!("/{}", s);
                    suffix.make_ascii_lowercase();
                    criterias.file_suffixes.push(suffix);
                }
                SearchCriteria::ExcludeEntryType(h) => criterias.excluded_entry_types.push(h),
                SearchCriteria::ExcludeEntryPath(s) => criterias.excluded_entry_paths.push(s),
            }
        }

        let regex_include = if criterias.entry_paths.is_empty() {
            None
        } else {
            Some(Self::regex_from_words(&criterias.entry_paths)?)
        };
        let regex_exclude = if criterias.excluded_entry_paths.is_empty() {
            None
        } else {
            Some(Self::regex_from_words(&criterias.excluded_entry_paths)?)
        };

        let it = self.entries.iter()
            .filter(move |(hpath, (htype, findex))| {
                let file = &self.filenames[*findex];
                // Don't bother too much using a "smart" filtering
                // Keep in my that results are "truncated".
                (criterias.entry_types.is_empty() || criterias.entry_types.contains(htype)) &&
                !criterias.excluded_entry_types.contains(htype) &&
                (criterias.entry_hpaths.is_empty() || criterias.entry_hpaths.contains(hpath)) &&
                (criterias.file_suffixes.is_empty() || criterias.file_suffixes.iter().any(|suffix| {
                    file == &suffix[1..] || file.ends_with(suffix)
                })) &&
                regex_include.as_ref().map(|re| hpath.get_str(mappers).map(|s| re.is_match(s)).unwrap_or(false)).unwrap_or(true) &&
                !regex_exclude.as_ref().map(|re| hpath.get_str(mappers).map(|s| re.is_match(s)).unwrap_or(false)).unwrap_or(false)
            }).map(|(hpath, _)| *hpath);
        Ok(it)
    }

    /// Iterate on entries that use the given type
    pub fn iter_by_type(&self, htype: BinClassName) -> impl Iterator<Item=BinEntryPath> + '_ {
        self.entries.iter()
            .filter(move |(_, (t, _))| *t == htype)
            .map(|(k, _)| *k)
    }

    fn regex_from_words(words: &[&str]) -> Result<RegexSet, EntryDbError> {
        let patterns = words.iter().map(|s| regex::escape(s));
        RegexSetBuilder::new(patterns)
            .unicode(false)
            .case_insensitive(true)
            .build()
            .map_err(EntryDbError::InvalidSearchPattern)
    }

    /// Parse a search criteria, using database information to resolve hashes
    fn parse_criteria<'a>(&'a self, word: &'a str) -> SearchCriteria<'a> {
        if let Some(hash) = word.strip_prefix('-') {
            let htype = BinClassName::hashed(hash);
            if self.types.contains(&htype) {
                SearchCriteria::ExcludeEntryType(htype)
            } else {
                SearchCriteria::ExcludeEntryPath(hash)
            }
        } else {
            let hash = binhash_from_str(word);
            if self.entries.contains_key(&hash.into()) {
                SearchCriteria::EntryPathHash(hash.into())
            } else if self.types.contains(&hash.into()) {
                SearchCriteria::EntryType(hash.into())
            } else if word.ends_with(".bin") {
                SearchCriteria::FilePath(word)
            } else {
                SearchCriteria::EntryPath(word)
            }
        }
    }
}


/// Search criteria, parsed
enum SearchCriteria<'a> {
    EntryPath(&'a str),
    EntryPathHash(BinEntryPath),
    EntryType(BinClassName),
    FilePath(&'a str),
    ExcludeEntryType(BinClassName),
    ExcludeEntryPath(&'a str),
}

