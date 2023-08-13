use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use clap::{Arg, ArgAction, ArgGroup, Command, value_parser};

use cdragon_prop::{
    PropFile,
    data::{BinEntryPath, BinClassName},
    BinEntry,
    BinHashMappers,
    BinSerializer,
    BinEntriesSerializer,
    BinVisitor,
    TextTreeSerializer,
    JsonSerializer,
};
use cdragon_hashes::bin::binhash_from_str;
use cdragon_rman::{
    Rman,
    FileEntry,
};
use cdragon_wad::{
    WadFile,
    WadEntry,
    WadHashMapper,
    WadHashKind,
};
use cdragon_cdn::CdnDownloader;

mod utils;
use utils::{
    PathPattern,
    HashValuePattern,
    BinDirectoryVisitor,
    bin_files_from_dir,
};

mod bin_hashes;
use bin_hashes::{
    CollectHashesVisitor,
    CollectStringsVisitor,
    HashesMatchingEntriesVisitor,
    SearchBinValueVisitor,
};
mod guess_bin_hashes;
use guess_bin_hashes::{
    BinHashFinder,
    BinHashGuesser,
};

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;


/// Serialize entries from a given bin file path 
fn serialize_bin_path<F: Fn(BinEntryPath, BinClassName) -> bool>(path: &PathBuf, serializer: &mut dyn BinEntriesSerializer, filter: F) -> Result<()> {
    let scanner = PropFile::scan_entries_from_path(path)?;
    scanner.filter_parse(filter).try_for_each(|entry| -> Result<(), _> {
        serializer.write_entry(&entry?).map_err(|e| e.into())
    })
}


/// Read WAD from path parameter
fn wad_and_hmapper_from_paths(wad_path: &Path, hashes_dir: Option<&PathBuf>) -> Result<(WadFile, WadHashMapper)> {
    let wad = WadFile::open(wad_path)?;
    let mut hmapper = WadHashMapper::new();
    if let Some(dir) = hashes_dir {
        if let Some(kind) = WadHashKind::from_wad_path(wad_path) {
            let path = Path::new(dir).join(kind.mapper_path());
            hmapper.load_path(&path)?;
        }
    }
    Ok((wad, hmapper))
}

/// Create bin entry serializer
fn build_bin_entry_serializer<'a, W: io::Write>(writer: &'a mut W, hmappers: &'a BinHashMappers, json: bool) -> io::Result<Box<dyn BinEntriesSerializer + 'a>> {
    if json {
        Ok(Box::new(JsonSerializer::new(writer, hmappers).write_entries()?))
    } else {
        Ok(Box::new(TextTreeSerializer::new(writer, hmappers).write_entries()?))
    }
}


fn main() -> Result<(), Box<dyn std::error::Error>> {
    fn command(name: &'static str, about: &'static str) -> Command {
        Command::new(name)
            .about(about)
            .arg_required_else_help(true)
            .subcommand_required(true)
    }

    // Common arguments
    let arg_hashes_dir = || Arg::new("hashes")
        .short('H')
        .value_name("dir")
        .value_parser(value_parser!(PathBuf))
        .help("Directory with lists of known hashes");

    let cmd = command("cdragon", "CDragon toolbox CLI")
        .subcommand(
            // bin
            command("bin",  "Work on BIN files")
            .subcommand(
                command("dump", "Dump a BIN file as a plain text or JSON")
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
        )
        .subcommand({
            // rman
            let arg_manifest = || Arg::new("manifest")
                .required(true)
                .value_parser(value_parser!(PathBuf))
                .help("Manifest file to parse");

            command("rman", "Work on release manifests (RMAN files)")
            .subcommand(
                command("bundles", "List bundles")
                .arg(arg_manifest())
                .arg(Arg::new("chunks")
                    .short('c')
                    .action(ArgAction::SetTrue)
                    .help("Also list chunks within each bundle"))
            )
            .subcommand(
                command("files", "List files")
                .arg(arg_manifest())
                .arg(Arg::new("chunks")
                    .short('c')
                    .action(ArgAction::SetTrue)
                    .help("Also list chunks within each bundle"))
            )
            .subcommand(
                command("download", "Download files")
                .arg(Arg::new("output")
                     .short('o')
                     .value_name("dir")
                     .value_parser(value_parser!(PathBuf))
                     .default_value(".")
                     .help("Output directory for downloaded files"))
                .arg(arg_manifest().index(1))
                .arg(Arg::new("patterns")
                     .required(true)
                     .index(2)
                     .num_args(1..)
                     .help("Paths of files to download, `*` wildcards are supported"))
            )
        })
        .subcommand({
            // wad
            let arg_wad = || Arg::new("wad")
                .required(true)
                .value_parser(value_parser!(PathBuf))
                .help("WAD file to parse");

            command("wad", "Work on WAD archives")
            .subcommand(
                command("list", "List WAD entries")
                .arg(arg_wad())
                .arg(arg_hashes_dir())
            )
            .subcommand(
                command("extract", "Extract WAD entries")
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
        })
        .subcommand({
            // hashes
            let arg_bin_dir = || Arg::new("input")
                .value_name("bin")
                .value_parser(value_parser!(PathBuf))
                .required(true)
                .help("Directory with `.bin` files to scan");

            command("hashes", "Tools to collect and guess hashes from BIN files")
            .subcommand(
                command("get-unknown", "Collect unknown hashes from BIN files")
                .arg(arg_bin_dir())
                .arg(arg_hashes_dir().required(true))
                .arg(Arg::new("output")
                    .short('o')
                    .value_name("dir")
                    .default_value(".")
                    .value_parser(value_parser!(PathBuf))
                    .help("Output directory for unknown hashes files (default: `.`)"))
            )
            .subcommand(
                command("guess", "Guess unknown hashes from BIN files")
                .arg(arg_bin_dir())
                .arg(arg_hashes_dir().required(true))
                .arg(Arg::new("unknown")
                    .short('u')
                    .value_name("dir")
                    .value_parser(value_parser!(PathBuf))
                    .help("Directory with unknown hash lists"))
            )
            .subcommand(
                command("get-strings", "Collect strings BIN files")
                .arg(arg_bin_dir())
            )
            .subcommand(
                command("search-entries", "Dump BIN entries containing the provided string")
                .arg(arg_bin_dir())
                .arg(arg_hashes_dir().required(true))
                .arg(Arg::new("pattern")
                    .required(true)
                    .help("Value to search for (exact match)"))
                .arg(Arg::new("string").short('s').action(ArgAction::SetTrue))
                .arg(Arg::new("hash").short('a').action(ArgAction::SetTrue))
                .arg(Arg::new("link").short('l').action(ArgAction::SetTrue))
                .group(ArgGroup::new("type")
                    .required(true)
                    .args(["string", "hash", "link"]))
                .arg(Arg::new("json")
                    .short('j')
                    .action(ArgAction::SetTrue)
                    .help("Dump as JSON"))
            )
            .subcommand(
                command("hashes-matching-entries", "Print (partial) information on hash values matching entry paths")
                .arg(arg_bin_dir())
                .arg(arg_hashes_dir().required(true))
            )
        })
        ;

    let matches = cmd.get_matches();
    match matches.subcommand() {
        Some(("bin", matches)) => {
            match matches.subcommand() {
                Some(("dump", matches)) => {
                    let hmappers = match matches.get_one::<PathBuf>("hashes") {
                        Some(dir) => BinHashMappers::from_dirpath(Path::new(dir))?,
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
        Some(("rman", matches)) => {
            match matches.subcommand() {
                Some(("bundles", matches)) => {
                    let rman = Rman::open(matches.get_one::<PathBuf>("manifest").unwrap())?;
                    let show_chunks = matches.get_flag("chunks");
                    for bundle in rman.iter_bundles() {
                        println!("{:016x}  chunks: {}", bundle.id, bundle.chunks_count());
                        if show_chunks {
                            for chunk in bundle.iter_chunks() {
                                println!("  {:016x}  size: {} -> {}", chunk.id, chunk.bundle_size, chunk.target_size);
                            }
                        }
                    }

                    Ok(())
                }
                Some(("files", matches)) => {
                    let rman = Rman::open(matches.get_one::<PathBuf>("manifest").unwrap())?;
                    let dir_paths = rman.dir_paths();
                    for file in rman.iter_files() {
                        println!("{}", file.path(&dir_paths));
                    }

                    Ok(())
                }
                Some(("download", matches)) => {
                    let rman = Rman::open(matches.get_one::<PathBuf>("manifest").unwrap())?;
                    let patterns = matches.get_many::<String>("patterns").unwrap();
                    let path_patterns: Vec<PathPattern> = patterns.map(|v| PathPattern::new(v)).collect();

                    // Collect file entries to fetch
                    let file_entries: Vec<(String, FileEntry)> = {
                        let dir_paths = rman.dir_paths();
                        rman
                            .iter_files()
                            .filter_map(|entry| {
                                let path = entry.path(&dir_paths);
                                if path_patterns.iter().any(|pat| pat.is_match(&path)) {
                                    Some((path, entry))
                                } else {
                                    None
                                }
                            }).collect()
                    };
                    if file_entries.is_empty() {
                        eprintln!("No matching file found in manifest");
                        std::process::exit(2);
                    }
                    println!("Downloading {} file(s)", file_entries.len());

                    let bundle_chunks = rman.bundle_chunks();

                    let output = Path::new(matches.get_one::<PathBuf>("output").unwrap());
                    fs::create_dir_all(output)?;

                    let cdn = CdnDownloader::new()?;

                    // Process each file, one by one
                    for (path, file_entry) in file_entries.into_iter() {
                        let (file_size, ranges) = file_entry.bundle_chunks(&bundle_chunks);
                        println!("Downloading {} ({} bytes)", path, file_size);
                        cdn.download_bundle_chunks(file_size as u64, &ranges, &output.join(path))?;
                    }

                    Ok(())
                }
                _ => unreachable!(),
            }
        }
        Some(("wad", matches)) => {
            match matches.subcommand() {
                Some(("list", matches)) => {
                    let (wad, hmapper) = wad_and_hmapper_from_paths(matches.get_one::<PathBuf>("wad").unwrap(), matches.get_one::<PathBuf>("hashes"))?;
                    for entry in wad.iter_entries() {
                        let entry = entry?;
                        println!("{:x}  {}", entry.path, hmapper.get(entry.path.hash).unwrap_or("?"));
                    }
                    Ok(())
                }
                Some(("extract", matches)) => {
                    let (mut wad, hmapper) = wad_and_hmapper_from_paths(matches.get_one::<PathBuf>("wad").unwrap(), matches.get_one::<PathBuf>("hashes"))?;
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
        Some(("hashes", matches)) => {
            match matches.subcommand() {
                Some(("get-unknown", matches)) => {
                    let path = matches.get_one::<PathBuf>("input").unwrap();
                    let hmappers = {
                        let dir = matches.get_one::<PathBuf>("hashes").unwrap();
                        BinHashMappers::from_dirpath(Path::new(dir))?
                    };

                    let mut hashes = CollectHashesVisitor::default()
                        .traverse_dir(path)?
                        .take_result();
                    bin_hashes::remove_known_from_unknown(&mut hashes, &hmappers);

                    let output = matches.get_one::<PathBuf>("output").unwrap();
                    bin_hashes::write_unknown(output.into(), &hashes)?;

                    Ok(())
                }
                Some(("guess", matches)) => {
                    let path = matches.get_one::<PathBuf>("input").unwrap();
                    let hdir = Path::new(matches.get_one::<PathBuf>("hashes").unwrap());
                    let hmappers = BinHashMappers::from_dirpath(hdir)?;
                    let udir = matches.get_one::<PathBuf>("unknown").map(Path::new);
                    let mut hashes = if let Some(udir) = udir {
                        bin_hashes::load_unknown(udir.into())?
                    } else {
                        // Collect unknown hashes
                        println!("Collecting unknown hashes...");
                        CollectHashesVisitor::default()
                            .traverse_dir(path)?
                            .take_result()
                    };
                    bin_hashes::remove_known_from_unknown(&mut hashes, &hmappers);

                    println!("Guessing new hashes...");
                    let finder = BinHashFinder::new(hashes, hmappers)
                        .on_found(|h, s| println!("{:08x} {}", h, s));
                    let mut guesser = BinHashGuesser::new(finder)
                        .with_all_hooks();
                    //.with_entry_stats();
                    guesser.guess_dir(path);
                    let finder = guesser.result();

                    println!("Updating files...");
                    finder.hmappers.write_dirpath(hdir)?;

                    if let Some(udir) = udir {
                        bin_hashes::write_unknown(udir.into(), &finder.hashes)?;
                    }

                    Ok(())
                }
                Some(("get-strings", matches)) => {
                    let path = matches.get_one::<PathBuf>("input").unwrap();
                    let strings = CollectStringsVisitor::default()
                        .traverse_dir(path)?
                        .take_result();
                    for s in strings {
                        println!("{}", s);
                    }
                    Ok(())
                }
                Some(("search-entries", matches)) => {
                    let path = matches.get_one::<PathBuf>("input").unwrap();
                    let pattern = matches.get_one::<String>("pattern").unwrap();
                    let hdir = Path::new(matches.get_one::<PathBuf>("hashes").unwrap());
                    let hmappers = BinHashMappers::from_dirpath(hdir)?;

                    let mut writer = io::BufWriter::new(io::stdout());
                    let mut serializer = build_bin_entry_serializer(&mut writer, &hmappers, matches.get_flag("json"))?;
                    {
                        let serializer = &mut serializer;
                        let on_match = move |entry: &BinEntry| { serializer.write_entry(entry).unwrap(); };

                        use cdragon_prop::data::*;
                        let mut visitor: Box<dyn BinVisitor<Error=()>> = if matches.get_flag("string") {
                            Box::new(SearchBinValueVisitor::new(BinString(pattern.clone()), on_match))
                        } else if matches.get_flag("hash") {
                            let hash: BinHashValue = binhash_from_str(pattern).into();
                            Box::new(SearchBinValueVisitor::new(BinHash(hash), on_match))
                        } else if matches.get_flag("link") {
                            let hash: BinEntryPath = binhash_from_str(pattern).into();
                            Box::new(SearchBinValueVisitor::new(BinLink(hash), on_match))
                        } else {
                            unreachable!();
                        };
                        visitor.traverse_dir(path)?;
                    }
                    serializer.end()?;
                    Ok(())
                }
                Some(("hashes-matching-entries", matches)) => {
                    let path = matches.get_one::<PathBuf>("input").unwrap();
                    let hmappers = {
                        let dir = matches.get_one::<PathBuf>("hashes").unwrap();
                        BinHashMappers::from_dirpath(Path::new(dir))?
                    };
                    HashesMatchingEntriesVisitor::new(&hmappers).traverse_dir(path)?;
                    Ok(())
                }
                _ => unreachable!(),
            }
        }
        _ => unreachable!(),
    }
}

