use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use clap::{Arg, AppSettings};
use walkdir::{WalkDir, DirEntry};

use cdragon_prop::{
    PropFile,
    BinHashMappers,
    BinHashSets,
    BinHashKind,
    BinSerializer,
    BinEntriesSerializer,
    TextTreeSerializer,
    JsonSerializer,
};
use cdragon_rman::{
    Rman,
    FileEntry,
};
use cdragon_wad::{
    Wad,
    WadEntry,
    WadHashMapper,
    WadHashKind,
};
use cdragon_cdn::CdnDownloader;
use cdragon_utils::Result;

mod cli;
use cli::NestedCommand;

mod utils;
use utils::{
    PathPattern,
    HashValuePattern,
};

mod guess_bin_hashes;
use guess_bin_hashes::{
    BinHashFinder,
    BinHashGuesser,
};

type BinEntryScanner = cdragon_prop::BinEntryScanner<io::BufReader<fs::File>>;


fn is_binfile_direntry(entry: &DirEntry) -> bool {
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

/// Canonicalize a path, avoid errors on long file names
///
/// `canonicalize()` is needed to open long files on Windows, but it still fails if the path is too
/// long. `canonicalize()` the directory name then manually join the file name.
pub fn canonicalize_path(path: &Path) -> std::io::Result<PathBuf> {
    if cfg!(target_os = "windows") {
        if let Some(mut parent) = path.parent() {
            if let Some(base) = path.file_name() {
                if parent.as_os_str() == "" {
                    parent = Path::new(".");
                }
                return Ok(parent.canonicalize()?.join(base))
            }
        }
    }
    Ok(path.to_path_buf())
}


fn main() -> Result<()> {
    let cmd = NestedCommand::new("cdragon")
        .options(|app| {
            app
                .about("CDragon toolbox CLI")
                .setting(AppSettings::ArgRequiredElseHelp)
        })
    .add_nested(
        NestedCommand::new("bin")
        .options(|app| {
            app
                .about("Work on BIN files")
                .setting(AppSettings::ArgRequiredElseHelp)
        })
        .add_nested(
            NestedCommand::new("dump")
            .options(|app| {
                app
                    .about("Dump a BIN file as a text tree")
                    .arg(Arg::with_name("input")
                         .value_name("bin")
                         .required(true)
                         .multiple(true)
                         .help("`.bin` files or directories to extract (recursively for directories)"))
                    .arg(Arg::with_name("hashes")
                         .short("H")
                         .value_name("dir")
                         .help("Directory with hash lists"))
                    .arg(Arg::with_name("json")
                         .short("j")
                         .help("Dump as JSON (output one object per `.bin` file)"))
            })
            .runner(|subm| {
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

                for path in subm.values_of("input").unwrap() {
                    let path = Path::new(path);
                    if path.is_dir() {
                        for path in bin_files_from_dir(path) {
                            let scanner = PropFile::scan_entries_from_path(path)?;
                            serialize_bin_scanner(scanner, &mut *serializer)?;
                        }
                    } else {
                        let scanner = PropFile::scan_entries_from_path(path)?;
                        serialize_bin_scanner(scanner, &mut *serializer)?;
                    }
                }

                serializer.end()?;
                Ok(())
            })
            )
        .add_nested(
            NestedCommand::new("unknown-hashes")
            .options(|app| {
                app
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
            })
            .runner(|subm| {
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

                Ok(())
            })
            )
        .add_nested(
            NestedCommand::new("guess-hashes")
            .options(|app| {
                app
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
            })
            .runner(|subm| {
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

                Ok(())
            })
            )
        )
    .add_nested(
        NestedCommand::new("rman")
        .options(|app| {
            app
                .about("Work on release manifests (RMAN files)")
                .setting(AppSettings::ArgRequiredElseHelp)
        })
        .add_nested(
            NestedCommand::new("bundles")
            .options(|app| {
                app
                    .about("List bundles")
                    .arg(Arg::with_name("manifest")
                         .required(true)
                         .help("Manifest file to parse"))
                    .arg(Arg::with_name("chunks")
                         .short("c")
                         .help("Also list chunks within each bundle"))
            })
            .runner(|subm| {
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

                Ok(())
            })
            )
        .add_nested(
            NestedCommand::new("files")
            .options(|app| {
                app
                    .about("List files")
                    .arg(Arg::with_name("manifest")
                         .required(true)
                         .help("Manifest file to parse"))
            })
            .runner(|subm| {
                let rman = Rman::open(subm.value_of("manifest").unwrap())?;
                let dir_paths = rman.dir_paths();
                for file in rman.iter_files() {
                    println!("{}", file.path(&dir_paths));
                }

                Ok(())
            })
            )
        .add_nested(
            NestedCommand::new("download")
            .options(|app| {
                app
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
            })
            .runner(|subm| {
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

                Ok(())
            })
            )
        )
    .add_nested(
        NestedCommand::new("wad")
        .options(|app| {
            app
                .about("Work on WAD archives")
                .setting(AppSettings::ArgRequiredElseHelp)
        })
        .add_nested(
            NestedCommand::new("list")
            .options(|app| {
                app
                    .about("List WAD entries")
                    .arg(Arg::with_name("wad")
                         .required(true)
                         .help("WAD file to parse"))
                    .arg(Arg::with_name("hashes")
                         .short("H")
                         .value_name("dir")
                         .help("Directory with hash list"))
            })
            .runner(|subm| {
                let (wad, _, hmapper) = wad_and_hmapper_from_paths(subm.value_of("wad").unwrap(), subm.value_of("hashes"))?;
                for entry in wad.iter_entries() {
                    println!("{:x}  {}", entry.path, hmapper.get(entry.path.hash).unwrap_or("?"));
                }

                Ok(())
            })
            )
        .add_nested(
            NestedCommand::new("extract")
            .options(|app| {
                app
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
            })
            .runner(|subm| {
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

                Ok(())
            })
            )
        )
    ;

    cmd.run()
}

