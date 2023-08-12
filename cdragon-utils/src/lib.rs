pub mod hashes;
pub mod locale;
pub mod parsing;
mod guarded_file;
pub use guarded_file::GuardedFile;

/// Generic string error
#[derive(Debug)]
pub struct StringError(pub String);

impl std::fmt::Display for StringError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

