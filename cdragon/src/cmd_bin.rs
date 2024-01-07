use std::io;
use std::path::PathBuf;
use anyhow::{Context, Result};
use cdragon_hashes::bin::binhash_from_str;
use cdragon_prop::{
    BinHashMappers,
    BinEntryPath,
    BinClassName,
    BinEntriesSerializer,
    PropFile,
};
use crate::cli::*;
use crate::utils::{
    bin_files_from_dir,
    build_bin_entry_serializer,
};

pub fn subcommand(name: &'static str) -> Subcommand {
    let cmd = parent_command(name)
        .about("Work on BIN files")
        .subcommand(
            Command::new("dump")
            .about("Dump a BIN file as a plain text or JSON")
            .arg(Arg::new("input")
                .value_name("bin")
                .required(true)
                .num_args(1..)
                .value_parser(value_parser!(PathBuf))
                .help("`.bin` files or directories to extract (recursively for directories)"))
            .arg(arg_hashes_dir())
            .arg(Arg::new("json")
                .short('j')
                .action(ArgAction::SetTrue)
                .help("Dump as JSON (output one object per `.bin` file)"))
            .arg(Arg::new("entry-type")
                .short('e')
                .value_name("type")
                .help("Dump only entries with the given type"))
        )
        ;
    (cmd, handle)
}

fn handle(matches: &ArgMatches) -> CliResult {
    match matches.subcommand() {
        Some(("dump", matches)) => {
            let hmappers = match get_hashes_dir(matches) {
                Some(dir) => BinHashMappers::from_dirpath(&dir)
                    .with_context(|| format!("failed to load hash mappers from {}", dir.display()))?,
                _ => BinHashMappers::default(),
            };

            let mut writer = io::BufWriter::new(io::stdout());
            let mut serializer = build_bin_entry_serializer(&mut writer, &hmappers, matches.get_flag("json"))?;
            let filter: Box<dyn Fn(BinEntryPath, BinClassName) -> bool> = match matches.get_one::<String>("entry-type") {
                Some(s) => {
                    let ctype: BinClassName = binhash_from_str(s).into();
                    Box::new(move |_, t| t == ctype)
                }
                None => Box::new(|_, _| true)
            };

            for path in matches.get_many::<PathBuf>("input").unwrap() {
                if path.is_dir() {
                    for path in bin_files_from_dir(path) {
                        serialize_bin_path(&path, &mut *serializer, &filter)?;
                    }
                } else {
                    serialize_bin_path(path, &mut *serializer, &filter)?;
                }
            }

            serializer.end()?;
            Ok(())
        }
        _ => unreachable!(),
    }
}

/// Serialize entries from a given bin file path
pub fn serialize_bin_path<F: Fn(BinEntryPath, BinClassName) -> bool>(path: &PathBuf, serializer: &mut dyn BinEntriesSerializer, filter: F) -> Result<()> {
    let scanner = PropFile::scan_entries_from_path(path)?;
    scanner.filter_parse(filter).try_for_each(|entry| -> Result<(), _> {
        serializer.write_entry(&entry?).map_err(|e| e.into())
    })
}

