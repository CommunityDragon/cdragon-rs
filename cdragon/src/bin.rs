use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use clap::{App, SubCommand, Arg, AppSettings};
use walkdir::{WalkDir, DirEntry};

use cdragon::prop::{
    PropFile,
    BinHashMappers,
    BinHashSets,
    BinHashKind,
    BinSerializer,
    BinEntriesSerializer,
    BinHashFinder,
    BinHashGuesser,
    TextTreeSerializer,
    JsonSerializer,
};
use cdragon::rman::{
    Rman,
    FileEntry,
};
use cdragon::wad::{
    Wad,
    WadEntry,
    WadHashMapper,
    WadHashKind,
};
use cdragon::cdn::CdnDownloader;
use cdragon::utils::{
    PathPattern,
    HashValuePattern,
};
use cdragon::Result;
use cdragon::fstools::canonicalize_path;

type BinEntryScanner = cdragon::prop::BinEntryScanner<io::BufReader<fs::File>>;


fn is_binfile_direntry(entry: &DirEntry) -> bool{
    let ftype = entry.file_type();
    if ftype.is_file() {
        if entry.path().extension().map(|s| s == "bin").unwrap_or(false) {
            // Some files are not actual 'PROP' files
            entry.file_name() != "tftoutofgamecharacterdata.bin"
        } else {
            false
        }
    } else if ftype.is_dir() {
        true
    } else {
        false
    }
}


/// Serialize a scanner to an entry serializer
fn serialize_bin_scanner(scanner: BinEntryScanner, serializer: &mut dyn BinEntriesSerializer) -> Result<()> {
    scanner.parse().try_for_each(|entry| -> Result<()> {
        serializer.write_entry(&entry?).map_err(|e| e.into())
    })
}


/// Iterate on bin files from a directory
fn bin_files_from_dir<P: AsRef<Path>>(root: P) -> impl Iterator<Item=PathBuf> {
    WalkDir::new(&root)
        .into_iter()
        .filter_entry(is_binfile_direntry)
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| canonicalize_path(&e.into_path()).ok())
}

/// Collect hashes from a directory
fn collect_bin_hashes_from_dir<P: AsRef<Path>>(root: P) -> Result<BinHashSets> {
    let mut hashes = BinHashSets::default();
    for path in bin_files_from_dir(root) {
        let scanner = PropFile::scan_entries_from_path(path)?;
        for entry in scanner.parse() {
            entry?.gather_bin_hashes(&mut hashes);
        }
    }
    Ok(hashes)
}


/// Read WAD from path parameter
fn wad_and_hmapper_from_paths(wad_path: &str, hashes_dir: Option<&str>) -> Result<(Wad, io::BufReader<fs::File>, WadHashMapper)> {
    let (wad, reader) = Wad::open(wad_path)?;
    let mut hmapper = WadHashMapper::new();
    if let Some(dir) = hashes_dir {
        if let Some(kind) = WadHashKind::from_wad_path(wad_path) {
            let path = Path::new(dir).join(kind.mapper_path());
            hmapper.load_path(&path)?;
        }
    }
    Ok((wad, reader, hmapper))
}


/// Simple macro to ease subcommand parsing
macro_rules! match_subcommand {
    (($appm:expr, $subm:ident) { $($name:literal => $block:block)* }) => {
        match $appm.subcommand() {
            $(($name, Some($subm)) => $block)*
            _ => std::unreachable!()
        }
    }
}


fn main() -> Result<()> {
    let appm = App::new("cdragon")
        .about("CDragon toolbox CLI")
        .setting(AppSettings::ArgRequiredElseHelp)
        .subcommand(
            SubCommand::with_name("bin")
            .about("Work on BIN files")
            .setting(AppSettings::ArgRequiredElseHelp)
            .subcommand(
                SubCommand::with_name("dump")
                .about("Dump a BIN file as a text tree")
                .arg(Arg::with_name("input")
                     .value_name("bin")
                     .required(true)
                     .help("`.bin` file or directory to extract"))
                .arg(Arg::with_name("hashes")
                     .short("H")
                     .value_name("dir")
                     .help("Directory with hash lists"))
                .arg(Arg::with_name("recursive")
                     .short("r")
                     .help("Scan `.bin` files in the provided directory, recursively"))
                .arg(Arg::with_name("json")
                     .short("j")
                     .help("Dump as JSON (with `-r`, output one object per `.bin` file)"))
                )
            .subcommand(
                SubCommand::with_name("unknown-hashes")
                .about("Gather unknown hashes from BIN files")
                .arg(Arg::with_name("input")
                     .value_name("bin")
                     .required(true)
                     .help("Directory with `.bin` files to scan"))
                .arg(Arg::with_name("hashes")
                     .short("H")
                     .value_name("dir")
                     .required(true)
                     .help("Directory with known hash lists"))
                .arg(Arg::with_name("output")
                     .short("o")
                     .value_name("dir")
                     .default_value(".")
                     .help("Output directory for unknown hashes files (default: `.`)"))
                )
            .subcommand(
                SubCommand::with_name("guess-hashes")
                .about("Guess unknown hashes from BIN files")
                .arg(Arg::with_name("input")
                     .value_name("bin")
                     .required(true)
                     .help("Directory with `.bin` files to scan"))
                .arg(Arg::with_name("hashes")
                     .short("H")
                     .value_name("dir")
                     .required(true)
                     .help("Directory with known hash lists"))
                )
            )
        .subcommand(
            SubCommand::with_name("rman")
            .about("Work on release manifests (RMAN files)")
            .setting(AppSettings::ArgRequiredElseHelp)
            .subcommand(
                SubCommand::with_name("bundles")
                .about("List bundles")
                .arg(Arg::with_name("manifest")
                     .required(true)
                     .help("Manifest file to parse"))
                .arg(Arg::with_name("chunks")
                     .short("c")
                     .help("Also list chunks within each bundle"))
                )
            .subcommand(
                SubCommand::with_name("files")
                .about("List files")
                .arg(Arg::with_name("manifest")
                     .required(true)
                     .help("Manifest file to parse"))
                )
            .subcommand(
                SubCommand::with_name("download")
                .about("Download files")
                .arg(Arg::with_name("output")
                     .short("o")
                     .value_name("dir")
                     .help("Output directory for downloaded files"))
                .arg(Arg::with_name("manifest")
                     .required(true)
                     .index(1)
                     .help("Manifest file to parse"))
                .arg(Arg::with_name("patterns")
                     .required(true)
                     .multiple(true)
                     .help("Paths of files to download, `*` wildcards are supported"))
                )
            )
        .subcommand(
            SubCommand::with_name("wad")
            .about("Work on WAD archives")
            .setting(AppSettings::ArgRequiredElseHelp)
            .subcommand(
                SubCommand::with_name("list")
                .about("List WAD entries")
                .arg(Arg::with_name("wad")
                     .required(true)
                     .help("WAD file to parse"))
                .arg(Arg::with_name("hashes")
                     .short("H")
                     .value_name("dir")
                     .help("Directory with hash list"))
                )
            .subcommand(
                SubCommand::with_name("extract")
                .about("Extract WAD entries")
                .arg(Arg::with_name("wad")
                     .required(true)
                     .help("WAD file to parse"))
                .arg(Arg::with_name("output")
                     .short("o")
                     .value_name("dir")
                     .help("Output directory for extracted files"))
                .arg(Arg::with_name("unknown")
                     .short("u")
                     .value_name("subdir")
                     .help("Output unknown files to given subdirectory (empty to not output them)"))
                .arg(Arg::with_name("hashes")
                     .short("H")
                     .value_name("dir")
                     .help("Directory with hash list"))
                .arg(Arg::with_name("patterns")
                     .multiple(true)
                     .help("Hashes or paths of files to download, `*` wildcards are supported for paths"))
                )
            )
        .get_matches();

    match_subcommand!((appm, subm) {
        "bin" => {
            match_subcommand!((subm, subm) {
                "dump" => {
                    let path = subm.value_of("input").unwrap();
                    let hmappers = match subm.value_of("hashes") {
                        Some(dir) => BinHashMappers::from_dirpath(Path::new(dir))?,
                        _ => BinHashMappers::default(),
                    };
                    let json = subm.is_present("json");

                    let mut writer = io::BufWriter::new(io::stdout());
                    let mut serializer = if json {
                        Box::new(JsonSerializer::new(&mut writer, &hmappers).write_entries()?) as Box<dyn BinEntriesSerializer>
                    } else {
                        Box::new(TextTreeSerializer::new(&mut writer, &hmappers).write_entries()?) as Box<dyn BinEntriesSerializer>
                    };

                    if subm.is_present("recursive") {
                        for path in bin_files_from_dir(path) {
                            let scanner = PropFile::scan_entries_from_path(path)?;
                            serialize_bin_scanner(scanner, &mut *serializer)?;
                        }
                    } else {
                        let scanner = PropFile::scan_entries_from_path(path)?;
                        serialize_bin_scanner(scanner, &mut *serializer)?;
                    }

                    serializer.end()?;
                }
                "unknown-hashes" => {
                    let path = subm.value_of("input").unwrap();
                    let hmappers = {
                        let dir = subm.value_of("hashes").unwrap();
                        BinHashMappers::from_dirpath(Path::new(dir))?
                    };

                    let hashes = collect_bin_hashes_from_dir(path)?;
                    let output = Path::new(subm.value_of("output").unwrap());
                    fs::create_dir_all(output)?;

                    // Then write the result, after excluding known hashes
                    for kind in BinHashKind::variants() {
                        let path = output.join(match kind {
                            BinHashKind::EntryPath => "unknown.binentries.txt",
                            BinHashKind::ClassName => "unknown.bintypes.txt",
                            BinHashKind::FieldName => "unknown.binfields.txt",
                            BinHashKind::HashValue => "unknown.binhashes.txt",
                        });
                        let file = fs::File::create(path)?;
                        let mut writer = io::BufWriter::new(file);
                        let mapper = hmappers.get(kind);
                        for hash in hashes.get(kind).iter() {
                            if !mapper.is_known(*hash) {
                                writeln!(writer, "{:08x}", hash)?;
                            }
                        }
                    }
                }
                "guess-hashes" => {
                    let path = subm.value_of("input").unwrap();
                    let hdir = Path::new(subm.value_of("hashes").unwrap());
                    let mut hmappers = BinHashMappers::from_dirpath(hdir)?;

                    // Collect unknown hashes
                    let mut hashes = collect_bin_hashes_from_dir(path)?;
                    for kind in BinHashKind::variants() {
                        let mapper = hmappers.get(kind);
                        hashes.get_mut(kind).retain(|&h| !mapper.is_known(h));
                    }

                    let mut finder = BinHashFinder::new(&mut hashes, &mut hmappers);
                    finder.on_found = |h, s| println!("{:08x} {}", h, s);

                    let mut guesser = BinHashGuesser::new(path, finder);
                    guesser.guess_all()?;

                    //TODO don't write if nothing has been found?
                    hmappers.write_dirpath(hdir)?;
                }
            })
        }

        "rman" => {
            match_subcommand!((subm, subm) {
                "bundles" => {
                    let rman = Rman::open(subm.value_of("manifest").unwrap())?;
                    let show_chunks = subm.is_present("chunks");
                    for bundle in rman.iter_bundles() {
                        println!("{:016x}  chunks: {}", bundle.id, bundle.chunks_count());
                        if show_chunks {
                            for chunk in bundle.iter_chunks() {
                                println!("  {:016x}  size: {} -> {}", chunk.id, chunk.bundle_size, chunk.target_size);
                            }
                        }
                    }
                }
                "files" => {
                    let rman = Rman::open(subm.value_of("manifest").unwrap())?;
                    let dir_paths = rman.dir_paths();
                    for file in rman.iter_files() {
                        println!("{}", file.path(&dir_paths));
                    }
                }
                "download" => {
                    let rman = Rman::open(subm.value_of("manifest").unwrap())?;
                    let patterns = subm.values_of_lossy("patterns").unwrap();
                    let path_patterns: Vec<PathPattern> = patterns.iter().map(|v| PathPattern::new(v)).collect();

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

                    let output = Path::new(subm.value_of("output").unwrap_or("."));
                    fs::create_dir_all(output)?;

                    let cdn = CdnDownloader::new()?;

                    // Process each file, one by one
                    for (path, file_entry) in file_entries.into_iter() {
                        let (file_size, ranges) = file_entry.bundle_chunks(&bundle_chunks);
                        println!("Downloading {} ({} bytes)", path, file_size);
                        cdn.download_bundle_chunks(file_size as u64, &ranges, &output.join(path))?;
                    }
                }
            })
        }

        "wad" => {
            match_subcommand!((subm, subm) {
                "list" => {
                    let (wad, _, hmapper) = wad_and_hmapper_from_paths(subm.value_of("wad").unwrap(), subm.value_of("hashes"))?;
                    for entry in wad.iter_entries() {
                        println!("{:x}  {}", entry.path, hmapper.get(entry.path.hash).unwrap_or("?"));
                    }
                }
                "extract" => {
                    let (wad, mut reader, hmapper) = wad_and_hmapper_from_paths(subm.value_of("wad").unwrap(), subm.value_of("hashes"))?;
                    let patterns = subm.values_of_lossy("patterns");
                    let hash_patterns: Option<Vec<HashValuePattern<u64>>> =
                        patterns.as_ref().map(|p| p.iter().map(|v| HashValuePattern::new(v)).collect());

                    let output = Path::new(subm.value_of("output").unwrap_or("."));
                    let unknown = subm.value_of("unknown").map(|p| output.join(p));

                    let entries = wad
                        .iter_entries()
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
                        entry.extract(&mut reader, &path)?;
                    }
                }
            })
        }
    });

    Ok(())
}

