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
            .arg(Arg::new("hexa")
                .short('x')
                .action(ArgAction::SetTrue)
                .help("Dump keys as hexadecimal instead of reversed strings"))
            .arg(arg_rst())
            .arg(arg_hashes_dir())
        )
        ;
    (cmd, handle)
}

fn handle(matches: &ArgMatches) -> CliResult {
    match matches.subcommand() {
        Some(("list", matches)) => {
            let rst = rst_from_path(matches.get_one::<PathBuf>("rst").unwrap())?;
            if matches.get_flag("hexa") {
                let nchars = rst.hash_bits().div_ceil(4) as usize;
                for (hash, value) in rst.iter() {
                    println!("{:0w$x} {}", hash, value, w = nchars);
                }
            } else {
                let hmapper = hmapper_from_path(get_hashes_dir(matches))?;
                for (hash, value) in rst.iter() {
                    println!("{} {}", hmapper.get(hash).unwrap_or("?"), value);
                }
            }
            Ok(())
        }
        _ => unreachable!(),
    }
}

/// Read RST from path parameter
fn rst_from_path(rst_path: &Path) -> Result<Rst> {
    Rst::open(rst_path).with_context(|| format!("failed to open RST file {}", rst_path.display()))
}

/// Read RstHashMapper from path parameter
fn hmapper_from_path(hashes_dir: Option<PathBuf>) -> Result<RstHashMapper> {
    let mut hmapper = RstHashMapper::new();
    if let Some(dir) = hashes_dir {
        let path = dir.join(HashKind::Rst.mapping_path());
        hmapper.load_path(&path).with_context(|| format!("failed to load hash mapping {}", path.display()))?;
    }
    Ok(hmapper)
}

