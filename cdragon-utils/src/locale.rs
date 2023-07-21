//! Language codes used for locales

use std::fmt;

/// Language code (e.g. `en_US`)
///
/// Codes are made of two parts: language (e.g. `en`) and territory (e.g. `US`).
/// The territory is always normalized uppercase but is accepted as lowercase.
#[derive(Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
pub struct Locale {
    code: [u8; 5],
}

impl Locale {
    /// Parse a locale code from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if !Self::is_valid_code(bytes) {
            Err(Error::InvalidCode)
        } else {
            let code: [u8; 5] = [
                bytes[0],
                bytes[1],
                b'_',
                bytes[3].to_ascii_uppercase(),
                bytes[4].to_ascii_uppercase(),
            ];
            Ok(Self { code })
        }
    }

    /// Check if bytes make a valid locale code
    fn is_valid_code(bytes: &[u8]) -> bool {
        bytes.len() == 5 &&
        bytes[0].is_ascii_lowercase() &&
        bytes[1].is_ascii_lowercase() &&
        bytes[2] == b'_' &&
        (bytes[3].is_ascii_uppercase() || bytes[3].is_ascii_lowercase()) &&
        (bytes[4].is_ascii_uppercase() || bytes[4].is_ascii_lowercase())
    }

    /// Get code as an UTF-8 string
    pub fn as_str(&self) -> &str {
        unsafe {
            // safe because `code` is enforced to be ASCII
            std::str::from_utf8_unchecked(&self.code)
        }
    }
}

impl std::str::FromStr for Locale {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        Self::from_bytes(s.as_bytes())
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}


#[derive(Debug)]
pub enum Error {
    /// Invalid code format
    InvalidCode,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            Error::InvalidCode => &"invalid locale code",
        };
        f.write_str(s)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

