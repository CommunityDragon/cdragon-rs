//! Helpers for building clap commands
use std::path::PathBuf;
pub use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command, value_parser};

pub type CliResult = Result<(), Box<dyn std::error::Error>>;
pub type Subcommand = (Command, fn(&ArgMatches) -> CliResult);

pub fn parent_command(name: &'static str) -> Command {
    Command::new(name)
        .arg_required_else_help(true)
        .subcommand_required(true)
        .after_help(
            "CDRAGON_DATA is used as a fallback to find hash files.\n\
             It should be set to the root of a CDragon `Data` repository."
        )
}

pub fn arg_hashes_dir() -> Arg {
    Arg::new("hashes")
        .short('H')
        .env("CDRAGONTOOLBOX_HASHES_DIR")
        .value_name("dir")
        .value_parser(value_parser!(PathBuf))
        .help("Directory with lists of known hashes")
}

/// Get hashes directory from `hashes` arg or `CDRAGON_DATA` 
pub fn get_hashes_dir(matches: &ArgMatches) -> Option<PathBuf> {
    if let Some(path) = matches.get_one::<PathBuf>("hashes") {
        Some(path.into())
    } else {
        let data = std::env::var_os("CDRAGON_DATA")?;
        let mut path: PathBuf = data.into();
        path.push("hashes");
        path.push("lol");
        Some(path)
    }
}

