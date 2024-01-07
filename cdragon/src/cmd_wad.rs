use std::path::{PathBuf, Path};
use anyhow::{Context, Result};
use cdragon_hashes::HashKind;
use cdragon_wad::{WadEntry, WadFile, WadHashMapper};
use crate::cli::*;
use crate::utils::HashValuePattern;

pub fn subcommand(name: &'static str) -> Subcommand {
    let arg_wad = || Arg::new("wad")
        .required(true)
        .value_parser(value_parser!(PathBuf))
        .help("WAD file to parse");

    let cmd = parent_command(name)
        .about("Work on WAD archives")
        .subcommand(
            Command::new("list")
            .about("List WAD entries")
            .arg(arg_wad())
            .arg(arg_hashes_dir())
        )
        .subcommand(
            Command::new("extract")
            .about("Extract WAD entries")
            .arg(arg_wad())
            .arg(Arg::new("output")
                .short('o')
                .value_name("dir")
                .value_parser(value_parser!(PathBuf))
                .default_value(".")
                .help("Output directory for extracted files"))
            .arg(Arg::new("unknown")
                .short('u')
                .value_name("subdir")
                .value_parser(value_parser!(PathBuf))
                .help("Output unknown files to given subdirectory (empty to not output them)"))
            .arg(arg_hashes_dir())
            .arg(Arg::new("patterns")
                .num_args(0..)
                .help("Hashes or paths of files to download, `*` wildcards are supported for paths"))
        )
        ;
    (cmd, handle)
}

fn handle(matches: &ArgMatches) -> CliResult {
    match matches.subcommand() {
        Some(("list", matches)) => {
            let (wad, hmapper) = wad_and_hmapper_from_paths(matches.get_one::<PathBuf>("wad").unwrap(), get_hashes_dir(matches))?;
            for entry in wad.iter_entries() {
                let entry = entry?;
                println!("{:x}  {}", entry.path, hmapper.get(entry.path.hash).unwrap_or("?"));
            }
            Ok(())
        }
        Some(("extract", matches)) => {
            let (mut wad, hmapper) = wad_and_hmapper_from_paths(matches.get_one::<PathBuf>("wad").unwrap(), get_hashes_dir(matches))?;
            let patterns = matches.get_many::<String>("patterns");
            let hash_patterns: Option<Vec<HashValuePattern<u64>>> =
                patterns.map(|p| p.map(|v| HashValuePattern::new(v)).collect());

            let output = Path::new(matches.get_one::<PathBuf>("output").unwrap());
            let unknown = matches.get_one::<PathBuf>("unknown").map(|p| output.join(p));

            let entries = wad
                .iter_entries()
                .map(|res| res.expect("entry error"))
                .filter(|e| !e.is_redirection());
            let entries: Vec<WadEntry> = match hash_patterns {
                Some(patterns) => {
                    let hmapper = &hmapper;
                    entries.filter(move |e| {
                        patterns.iter().any(|pat| pat.is_match(e.path.hash, hmapper))
                    }).collect()
                }
                None => entries.collect(),
            };
            for entry in entries {
                let path = match hmapper.get(entry.path.hash) {
                    Some(path) => output.join(path),
                    None => if let Some(p) = unknown.as_ref() {
                        p.join(format!("{:x}", entry.path))
                    } else {
                        println!("Skip unknown file: {:x}", entry.path);
                        continue;
                    }
                };
                println!("Extract {:x} to {}", entry.path, path.display());
                wad.extract_entry(&entry, &path)?;
            }

            Ok(())
        }
        _ => unreachable!(),
    }
}

/// Read WAD from path parameter
fn wad_and_hmapper_from_paths(wad_path: &Path, hashes_dir: Option<PathBuf>) -> Result<(WadFile, WadHashMapper)> {
    let wad = WadFile::open(wad_path).with_context(|| format!("failed to open WAD file {}", wad_path.display()))?;
    let mut hmapper = WadHashMapper::new();
    if let Some(dir) = hashes_dir {
        if let Some(kind) = HashKind::from_wad_path(wad_path) {
            let path = dir.join(kind.mapping_path());
            hmapper.load_path(&path).with_context(|| format!("failed to load hash mapping {}", path.display()))?;
        }
    }
    Ok((wad, hmapper))
}

