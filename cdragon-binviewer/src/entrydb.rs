use std::fmt;
use std::io::BufRead;
use std::collections::{HashMap, hash_map};
use gloo_console::debug;
use regex::{RegexSet, RegexSetBuilder};
use byteorder::{LittleEndian, ReadBytesExt};
use cdragon_prop::{
    BinEntryPath,
    BinClassName,
    BinHashMappers,
    binhash_from_str,
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

    /// Get an entry type from his hash
    pub fn get_entry_type(&self, hash: BinEntryPath) -> Option<BinClassName> {
        let (hpath, _) = self.entries.get(&hash)?;
        Some(*hpath)
    }

    /// Get an entry file from his hash
    pub fn get_entry_file(&self, hash: BinEntryPath) -> Option<&str> {
        let (_, ifile) = self.entries.get(&hash)?;
        Some(&self.filenames[*ifile])
    }

    /// Return the number of entries
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Run a "smart" search on words
    pub fn search_words<'a>(&'a self, words: &[&str], mappers: &'a BinHashMappers) -> Result<Box<dyn Iterator<Item = BinEntryPath> + 'a>> {
        if words.len() == 1 {
            // Use the word looks like a hash, us it as-is, other hash it
            // This won't work if the word is not a hash but looks like it
            let word = &words[0];
            let hash = binhash_from_str(word);
            if self.entries.contains_key(&BinEntryPath::from(hash)) {
                let hpath = BinEntryPath::from(hash);
                return Ok(Box::new(vec![hpath].into_iter()));
            }
            if self.types.contains(&BinClassName::from(hash)) {
                let htype = BinClassName::from(hash);
                return Ok(Box::new(self.iter_by_type(htype)));
            }
            let word = words[0].to_ascii_lowercase();
            if word.ends_with(".bin") {
                // match on whole path components
                let suffix = format!("/{}", word.to_lowercase());
                let iter = self.entries.iter()
                    .filter(move |(_, (_, i))| {
                        let fname = &self.filenames[*i];
                        fname == &suffix[1..] || fname.ends_with(&suffix)
                    }).map(|(h, _)| *h);
                return Ok(Box::new(iter));
            }
        }

        Ok(Box::new(self.search_entries(words, mappers)?))
    }

    /// Iterate on entries whose path match all the given words
    pub fn search_entries<'a>(&'a self, words: &[&str], mappers: &'a BinHashMappers) -> Result<SearchEntryIter<'a>> {
        Ok(SearchEntryIter {
            it: self.entries.keys(),
            regex: Self::regex_from_words(words)?,
            hmappers: &mappers,
        })
    }

    /// Iterate on entries that use the given type
    pub fn iter_by_type<'a>(&'a self, htype: BinClassName) -> ByTypeIter<'a> {
        ByTypeIter {
            it: self.entries.iter(),
            htype,
        }
    }

    fn regex_from_words(words: &[&str]) -> Result<RegexSet, EntryDbError> {
        let patterns = words.iter().map(|s| regex::escape(s));
        RegexSetBuilder::new(patterns)
            .unicode(false)
            .case_insensitive(true)
            .build()
            .map_err(|e| EntryDbError::InvalidSearchPattern(e))
    }
}


pub struct SearchEntryIter<'a> {
    it: hash_map::Keys<'a, BinEntryPath, (BinClassName, usize)>,
    regex: RegexSet,
    hmappers: &'a BinHashMappers,
}

impl<'a> Iterator for SearchEntryIter<'a> {
    type Item = BinEntryPath;

    fn next(&mut self) -> Option<Self::Item> {
        let n = self.regex.len();
        loop {
            match self.it.next() {
                None => return None,
                Some(v) => match v.get_str(self.hmappers) {
                    Some(path) if self.regex.matches(path).into_iter().count() == n => {
                        return Some(*v)
                    }
                    None | Some(_) => {}
                }
            }
        }
    }
}


pub struct ByTypeIter<'a> {
    it: hash_map::Iter<'a, BinEntryPath, (BinClassName, usize)>,
    htype: BinClassName,
}

impl<'a> Iterator for ByTypeIter<'a> {
    type Item = BinEntryPath;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.it.next() {
                None => return None,
                Some((k, (t, _))) if *t == self.htype => return Some(*k),
                Some(_) => {}
            };
        }
    }
}

