#[macro_use]
pub mod hashes;
pub mod locale;
pub mod parsing;
pub mod fstools;

/// Default generic result type
pub type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

