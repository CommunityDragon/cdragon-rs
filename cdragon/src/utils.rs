//! Various tools

use std::hash::Hash;
use num_traits::Num;
use crate::hashes::HashMapper;


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
pub enum HashValuePattern<'a, T: Num + Eq + Hash> {
    Hash(T),
    Path(PathPattern<'a>),
}

impl<'a, T: Num + Eq + Hash> HashValuePattern<'a, T> {
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


