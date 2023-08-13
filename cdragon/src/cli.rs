//! Helpers for building clap commands
use std::path::PathBuf;
pub use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command, value_parser};

pub type CliResult = Result<(), Box<dyn std::error::Error>>;
pub type Subcommand = (Command, fn(&ArgMatches) -> CliResult);

pub fn parent_command(name: &'static str) -> Command {
    Command::new(name)
        .arg_required_else_help(true)
        .subcommand_required(true)
}

pub fn arg_hashes_dir() -> Arg {
    Arg::new("hashes")
        .short('H')
        .value_name("dir")
        .value_parser(value_parser!(PathBuf))
        .help("Directory with lists of known hashes")
}

