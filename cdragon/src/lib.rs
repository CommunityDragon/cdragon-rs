mod parsing;
#[macro_use]
mod hashes;
mod locale;
pub mod fstools;
pub mod utils;
pub mod prop;
pub mod rman;
pub mod wad;
pub mod cdn;

/// Default generic result type
pub type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

