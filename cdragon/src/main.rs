//! CDragon toolbox CLI
use std::collections::HashMap;

mod cli;
mod utils;
mod cmd_bin;
mod cmd_rman;
mod cmd_wad;
#[cfg(feature = "hashes")]
mod cmd_hashes;

use cli::*;

struct Cli {
    command: Command,
    handlers: HashMap<&'static str, fn(&ArgMatches) -> CliResult>,
}

impl Cli {
    fn new() -> Self {
        Self {
            command: parent_command("cdragon").about("CDragon toolbox CLI"),
            handlers: Default::default(),
        }
    }

    /// Register a subcommand
    fn register(self, name: &'static str, source: fn(&'static str) -> Subcommand) -> Self {
        let Self { command, mut handlers } = self;
        let (subcmd, handler) = source(name);
        handlers.insert(name, handler);
        Self {
            command: command.subcommand(subcmd),
            handlers
        }
    }

    #[cfg(feature = "hashes")]
    fn register_hashes(self) -> Self {
        self.register("hashes", cmd_hashes::subcommand)
    }
    #[cfg(not(feature = "hashes"))]
    fn register_hashes(self) -> Self {
        self
    }

    fn process(self) -> CliResult {
        let Self { command, handlers } = self;
        let matches = command.get_matches();
        let (name, submatches) = matches.subcommand().unwrap();
        let handler = handlers.get(name).unwrap();
        handler(submatches)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Cli::new()
        .register("bin", cmd_bin::subcommand)
        .register("rman", cmd_rman::subcommand)
        .register("wad", cmd_wad::subcommand)
        .register_hashes()
        .process()
}

