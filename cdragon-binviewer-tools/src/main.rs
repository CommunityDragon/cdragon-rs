use std::io;
use std::path::{Path, PathBuf};
use std::collections::{HashSet, HashMap};
use walkdir::{WalkDir, DirEntry};
use clap::{Command, Arg, value_parser};
use byteorder::{LittleEndian, WriteBytesExt};
use cdragon_prop::{
    is_binfile_path,
    BinEntryPath,
    BinClassName,
    PropFile,
};
use cdragon_utils::GuardedFile;

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;


fn is_binfile_direntry(entry: &DirEntry) -> bool {
    let ftype = entry.file_type();
    if ftype.is_file() {
        is_binfile_path(entry.path())
    } else {
        ftype.is_dir()
    }
}

/// Normalize binfile path into a String, use only forward slashes
fn normalize_binfile_path(path: &Path) -> String {
    let filepath = path.to_str().unwrap();
    if cfg!(target_os = "windows") {
        filepath.replace('\\', "/")
    } else {
        filepath.to_string()
    }
}


#[derive(Default)]
struct Builder {
    entries: HashMap<BinEntryPath, (BinClassName, String)>,
    files: HashSet<String>,
    types: HashSet<BinClassName>,
    verbose: bool,
}

/// Build an entry database
impl Builder {
    fn new(verbose: bool) -> Self {
        Self { verbose, ..Default::default() }
    }

    /// Parse entry data from a directory of bin files
    fn load_dir<P: AsRef<Path>>(&mut self, root: P) -> Result<()> {
        for entry in WalkDir::new(&root).into_iter().filter_entry(is_binfile_direntry) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue
            }

            let path = entry.into_path();
            let scanner = PropFile::scan_entries_from_path(&path)?;
            if scanner.is_patch {
                continue;  // don't include patch entries
            }
            let filepath = normalize_binfile_path(path.strip_prefix(&root)?);
            self.files.insert(filepath.clone());
            for result in scanner.headers() {
                let (hpath, htype) = result?;
                let previous = self.entries.insert(hpath, (htype, filepath.clone()));
                if self.verbose {
                    if let Some((_, other_filepath)) = previous {
                        println!("duplicate entry: {:x} found in '{}' then '{}'", hpath, filepath, other_filepath);
                    }
                }
                self.types.insert(htype);
            }
        }

        Ok(())
    }

    /// Write the database to a file
    fn write<W: io::Write>(&self, mut w: W) -> io::Result<()> {
        macro_rules! write_u32 {
            ($w:expr, $v:expr) => ($w.write_u32::<LittleEndian>($v as u32))
        }

        // Write all filenames, prefixed by their count
        // Use `\n` as delimiter to be able to easily read them back
        // using `BufRead::read_line()`.
        // Also keep the "string to index" association
        let mut file_indexes = HashMap::<&str, u32>::new();
        write_u32!(w, self.files.len())?;
        for (i, file) in self.files.iter().enumerate() {
            writeln!(w, "{}", file)?;
            file_indexes.insert(file, i as u32);
        }

        // Write types, prefixed by their count
        write_u32!(w, self.types.len())?;
        for htype in &self.types {
            write_u32!(w, htype.hash)?;
        }

        // Write entries as (hpath, htype, file_begin, file_end)), prefixed by the entry count
        write_u32!(w, self.entries.len())?;
        for (hpath, (htype, file)) in &self.entries {
            write_u32!(w, hpath.hash)?;
            write_u32!(w, htype.hash)?;
            write_u32!(w, file_indexes[file.as_str()])?;
        }

        Ok(())
    }
}


/// Build a database from a list of bin files
fn build_entrydb<P: AsRef<Path>, Q: AsRef<Path>>(root: P, output: Q, verbose: bool) -> Result<()> {
    let mut builder = Builder::new(verbose);
    builder.load_dir(root)?;

    let output = output.as_ref();
    GuardedFile::for_scope(output, |file| {
        let writer = io::BufWriter::new(file);
        builder.write(writer)
    })?;

    if verbose {
        println!("Database written to {}", output.display());
        println!("  entries: {}", builder.entries.len());
        println!("  files: {}", builder.files.len());
        println!("  types: {}", builder.types.len());
    }

    Ok(())
}


fn main() {
    let appm = Command::new("cdragon-binviewer")
        .about("Tools for CDragon bin viewer")
        .arg(Arg::new("verbose")
             .short('v')
             .action(clap::ArgAction::SetTrue)
             .help("be more verbose"))
        .subcommand(
            Command::new("create-entrydb")
            .about("create DB for bin entries")
            .arg(Arg::new("db")
                 .short('o')
                 .value_name("FILE")
                 .value_parser(value_parser!(PathBuf))
                 .default_value("entries.db")
                 .help("database file to create"))
            .arg(Arg::new("dir")
                 .value_name("DIR")
                 .required(true)
                 .value_parser(value_parser!(PathBuf))
                 .help("root path for BIN files"))
            )
        .get_matches();

    let verbose = appm.get_flag("verbose");

    match appm.subcommand() {
        Some(("create-entrydb", subm)) => {
            let dirpath = subm.get_one::<PathBuf>("dir").unwrap();
            let dbpath = subm.get_one::<PathBuf>("db").unwrap();
            build_entrydb(dirpath, dbpath, verbose).unwrap();
        },
        _ => {
            eprintln!("Unexpected subcommand");
            std::process::exit(2);
        }
    }
}

