#[macro_use]
pub mod hashes;
pub mod locale;
pub mod parsing;
mod guarded_file;
pub use guarded_file::GuardedFile;

/// Default generic result type
pub type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

