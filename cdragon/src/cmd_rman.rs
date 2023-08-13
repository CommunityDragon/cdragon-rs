use std::fs;
use std::path::{PathBuf, Path};
use cdragon_cdn::CdnDownloader;
use cdragon_rman::{Rman, FileEntry};
use crate::cli::*;
use crate::utils::PathPattern;

pub fn subcommand(name: &'static str) -> Subcommand {
    let arg_manifest = || Arg::new("manifest")
        .required(true)
        .value_parser(value_parser!(PathBuf))
        .help("Manifest file to parse");

    let cmd = parent_command(name)
        .about("Work on release manifests (RMAN files)")
        .subcommand(
            Command::new("bundles")
            .about("List bundles")
            .arg(arg_manifest())
            .arg(Arg::new("chunks")
                .short('c')
                .action(ArgAction::SetTrue)
                .help("Also list chunks within each bundle"))
        )
        .subcommand(
            Command::new("files")
            .about("List files")
            .arg(arg_manifest())
            .arg(Arg::new("chunks")
                .short('c')
                .action(ArgAction::SetTrue)
                .help("Also list chunks within each bundle"))
        )
        .subcommand(
            Command::new("download")
            .about("Download files")
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
        ;

    (cmd, handle)
}

fn handle(matches: &ArgMatches) -> CliResult {
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

