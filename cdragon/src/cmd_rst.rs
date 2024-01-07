use std::path::{PathBuf, Path};
use anyhow::{Context, Result};
use cdragon_hashes::HashKind;
use cdragon_rst::{Rst, RstHashMapper};
use crate::cli::*;

pub fn subcommand(name: &'static str) -> Subcommand {
    let arg_rst = || Arg::new("rst")
        .required(true)
        .value_parser(value_parser!(PathBuf))
        .help("RST file to parse");

    let cmd = parent_command(name)
        .about("Work on RST files")
        .subcommand(
            Command::new("list")
            .about("List RST entries")
            .arg(arg_rst())
            .arg(arg_hashes_dir())
        )
        ;
    (cmd, handle)
}

fn handle(matches: &ArgMatches) -> CliResult {
    match matches.subcommand() {
        Some(("list", matches)) => {
            let (rst, hmapper) = rst_and_hmapper_from_paths(matches.get_one::<PathBuf>("rst").unwrap(), get_hashes_dir(matches))?;
            for (hash, value) in rst.iter() {
                println!("{} {}", hmapper.get(hash).unwrap_or("?"), value);
            }
            Ok(())
        }
        _ => unreachable!(),
    }
}

/// Read RST from path parameter
fn rst_and_hmapper_from_paths(rst_path: &Path, hashes_dir: Option<PathBuf>) -> Result<(Rst, RstHashMapper)> {
    let rst = Rst::open(rst_path).with_context(|| format!("failed to open RST file {}", rst_path.display()))?;
    let mut hmapper = RstHashMapper::new();
    if let Some(dir) = hashes_dir {
        let path = dir.join(HashKind::Rst.mapping_path());
        hmapper.load_path(&path).with_context(|| format!("failed to load hash mapping {}", path.display()))?;
    }
    Ok((rst, hmapper))
}

