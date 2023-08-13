use std::fs;
use std::io;
use std::io::{BufRead, Write};
use std::collections::HashSet;
use std::path::{PathBuf, Path};
use cdragon_hashes::{
    bin::binhash_from_str,
    HashError,
};
use cdragon_prop::{
    BinHashMappers,
    BinEntry,
    BinVisitor,
    PropFile,
    PropError,
    BinHashSets,
    data::*,
};
use cdragon_utils::GuardedFile;
use crate::cli::*;
use crate::utils::{
    bin_files_from_dir,
    build_bin_entry_serializer,
};

mod guess;
mod visitors;

use guess::*;
use visitors::*;


pub fn subcommand(name: &'static str) -> Subcommand {
    let arg_bin_dir = || Arg::new("input")
        .value_name("bin")
        .value_parser(value_parser!(PathBuf))
        .required(true)
        .help("Directory with `.bin` files to scan");

    let cmd = parent_command(name)
        .about("Tools to collect and guess hashes from BIN files")
        .subcommand(
            Command::new("get-unknown")
            .about("Collect unknown hashes from BIN files")
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
            Command::new("guess")
            .about("Guess unknown hashes from BIN files")
            .arg(arg_bin_dir())
            .arg(arg_hashes_dir().required(true))
            .arg(Arg::new("unknown")
                .short('u')
                .value_name("dir")
                .value_parser(value_parser!(PathBuf))
                .help("Directory with unknown hash lists"))
        )
        .subcommand(
            Command::new("get-strings")
            .about("Collect strings BIN files")
            .arg(arg_bin_dir())
        )
        .subcommand(
            Command::new("search-entries")
            .about("Dump BIN entries containing the provided string")
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
            Command::new("hashes-matching-entries")
            .about("Print (partial) information on hash values matching entry paths")
            .arg(arg_bin_dir())
            .arg(arg_hashes_dir().required(true))
        )
        ;
    (cmd, handle)
}

fn handle(matches: &ArgMatches) -> CliResult {
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
            remove_known_from_unknown(&mut hashes, &hmappers);

            let output = matches.get_one::<PathBuf>("output").unwrap();
            write_unknown(output.into(), &hashes)?;

            Ok(())
        }
        Some(("guess", matches)) => {
            let path = matches.get_one::<PathBuf>("input").unwrap();
            let hdir = Path::new(matches.get_one::<PathBuf>("hashes").unwrap());
            let hmappers = BinHashMappers::from_dirpath(hdir)?;
            let udir = matches.get_one::<PathBuf>("unknown").map(Path::new);
            let mut hashes = if let Some(udir) = udir {
                load_unknown(udir.into())?
            } else {
                // Collect unknown hashes
                println!("Collecting unknown hashes...");
                CollectHashesVisitor::default()
                    .traverse_dir(path)?
                    .take_result()
            };
            remove_known_from_unknown(&mut hashes, &hmappers);

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
                write_unknown(udir.into(), &finder.hashes)?;
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


fn unknown_path(kind: BinHashKind) -> &'static str {
    match kind {
        BinHashKind::EntryPath => "unknown.binentries.txt",
        BinHashKind::ClassName => "unknown.bintypes.txt",
        BinHashKind::FieldName => "unknown.binfields.txt",
        BinHashKind::HashValue => "unknown.binhashes.txt",
    }
}

fn load_unknown_file<P: AsRef<Path>>(path: P) -> Result<HashSet<u32>, HashError> {
    let file = fs::File::open(&path)?;
    let reader = io::BufReader::new(file);
    reader.lines()
        .map(|line| -> Result<u32, HashError> {
            line.map_err(HashError::Io).and_then(|line| {
                let line = line.trim_end();
                u32::from_str_radix(line, 16).map_err(|_| HashError::InvalidHashLine(line.to_owned()))
            })
        })
        .collect()
}

/// Load unknown hashes from text files in a directory
fn load_unknown(path: PathBuf) -> Result<BinHashSets, HashError> {
    let mut unknown = BinHashSets::default();
    for &kind in &BinHashKind::VARIANTS {
        *unknown.get_mut(kind) = load_unknown_file(path.join(unknown_path(kind)))?;
    }
    Ok(unknown)
}

/// Write (unknown) hashes to text files in a directory
fn write_unknown(path: PathBuf, hashes: &BinHashSets) -> Result<(), HashError> {
    std::fs::create_dir_all(&path)?;
    for &kind in &BinHashKind::VARIANTS {
        GuardedFile::for_scope(path.join(unknown_path(kind)), |file| {
            let mut writer = io::BufWriter::new(file);
            for hash in hashes.get(kind).iter() {
                writeln!(writer, "{:08x}", hash)?;
            }
            Ok(())
        })?;
    }
    Ok(())
}

/// Remove known hashes from `BinHashSets`
fn remove_known_from_unknown(unknown: &mut BinHashSets, hmappers: &BinHashMappers) {
    for &kind in &BinHashKind::VARIANTS {
        let mapper = hmappers.get(kind);
        unknown.get_mut(kind).retain(|h| !mapper.is_known(*h));
    }
}


/// Trait to visit a directory using a BinVisitor
trait BinDirectoryVisitor: BinVisitor<Error=()> {
    fn traverse_dir<P: AsRef<Path>>(&mut self, root: P) -> Result<&mut Self, PropError> {
        for path in bin_files_from_dir(root) {
            let scanner = PropFile::scan_entries_from_path(path)?;
            for entry in scanner.parse() {
                self.traverse_entry(&entry?).unwrap();  // never fails
            }
        }
        Ok(self)
    }
}

impl<T> BinDirectoryVisitor for T where T: BinVisitor<Error=()> + ?Sized {}

