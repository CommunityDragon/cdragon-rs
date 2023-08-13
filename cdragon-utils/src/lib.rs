//! Various utilities used by other CDragon crates
#[cfg(feature = "parsing")]
pub mod parsing;
#[cfg(feature = "guarded_file")]
mod guarded_file;
#[cfg(feature = "guarded_file")]
pub use guarded_file::GuardedFile;
