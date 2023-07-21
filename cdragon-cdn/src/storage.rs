//! Store files from Riot's CDN
//!
//! Files are stored in a directory with the following structure:
//! ```
//! cdragon/  -- cdragon specific files
//!   extract/  -- files extracted from manifests, one subdirectory per manifest ID
//!     <manifest-id>/
//!   shared/  -- shared extracted files (if symlinks are enabled)
//!   releases/  -- release informations
//! channels/  -- files from Riot's CDN (same structure)
//! ```
//!
//! In order to reduce storage usage, identical files extracted from different releases can be
//! shared using symlinks.
//! Extracted files are stored under `shared/` and named after a hash of their chunks.

use std::fs;
use std::io;
use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};
use cdragon_rman::{Rman, FileBundleRanges, FileEntry};
use cdragon_utils::StringError;
use cdragon_utils::fstools::symlink_file;
use super::CdnDownloader;
use super::Result;
use super::guarded_map::GuardedMmap;


/// Configuration of the storage
pub struct CdnStorageConf {
    /// Storage root path
    pub path: PathBuf,
    /// True to share extracted files using symlinks
    pub use_extract_symlinks: bool,
}

/// Store files from League patches
///
/// Download methods are lazy. If a file to download is already in the storage it will not be
/// downloaded again.
pub struct CdnStorage {
    conf: CdnStorageConf,
    downloader: CdnDownloader,
}

impl CdnStorage {
    pub fn new(conf: CdnStorageConf) -> Result<Self> {
        Ok(Self { conf, downloader: CdnDownloader::new()? })
    }

    /// Download a manifest from its ID, return its filesystem path
    pub fn download_manifest(&self, id: u64) -> Result<PathBuf> {
        let path = CdnDownloader::manifest_path(id);
        let fspath = self.conf.path.join(&path);
        if !fspath.exists() {
            self.downloader.download_path(&path, &fspath)?;
        }
        Ok(fspath)
    }

    /// Download a manifest from its URL, return its filesystem path
    ///
    /// URL basename must match manifest paths format used on CDN
    pub fn download_manifest_url(&self, url: &str) -> Result<PathBuf> {
        let id = parse_manifest_id(url)?;
        let path = CdnDownloader::manifest_path(id);
        let fspath = self.conf.path.join(path);
        if !fspath.exists() {
            self.downloader.download_url(url, &fspath)?;
        }
        Ok(fspath)
    }

    /// Download and extract manifest from its ID
    pub fn download_and_extract_manifest(&self, id: u64, output: &Path) -> Result<()> {
        let path = self.download_manifest(id)?;
        let rman = Rman::open(&path)?;
        self.download_manifest_bundles(&rman)?;
        //TODO extract to a temporary directory and rename it on success
        self.extract_manifest_files(&rman, output)?;
        Ok(())
    }

    /// Download bundles of a manifest
    fn download_manifest_bundles(&self, rman: &Rman) -> Result<()> {
        for entry in rman.iter_bundles() {
            let path = CdnDownloader::bundle_path(entry.id);
            let fspath = self.conf.path.join(&path);
            if !fspath.exists() {
                self.downloader.download_path(&path, &fspath)?;
            }
        }
        Ok(())
    }

    /// Extract files from a manifest
    ///
    /// Bundles are assumed to be available.
    fn extract_manifest_files(&self, rman: &Rman, output: &Path) -> Result<()> {
        let dir_paths = rman.dir_paths();
        let bundle_chunks = rman.bundle_chunks();
        for file_entry in rman.iter_files() {
            let path = file_entry.path(&dir_paths);
            // Note: some .dll/.exe are common to game and client manifests, but are slightly
            // different. Ignore if the target file already exists, even if symlinked.
            let target_path = output.join(&path);
            if target_path.exists() {
                continue;  // already extracted
            }

            // Group chunks by bundle ID to reduce open calls
            let (file_size, ranges) = file_entry.bundle_chunks(&bundle_chunks);
            if self.conf.use_extract_symlinks {
                let fspath = self.conf.path.join("cdragon/shared").join(Self::shared_file_hash(&file_entry));
                self.extract_chunks_to_file(file_size as u64, &ranges, &fspath)?;
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent)?;
                    let src_path = pathdiff::diff_paths(&fspath, parent).unwrap_or(fspath);
                    symlink_file(&src_path, &target_path)?;
                } else {
                    symlink_file(&fspath, &target_path)?;
                }
            } else {
                self.extract_chunks_to_file(file_size as u64, &ranges, &target_path)?;
            }
        }
        Ok(())
    }

    /// Extract a single file from a manifest
    fn extract_chunks_to_file(&self, file_size: u64, bundle_ranges: &FileBundleRanges, output: &Path) -> Result<()> {
        // Open output file, map it to memory
        let mut mmap = GuardedMmap::create(output, file_size)?;
        let buf: &mut [u8] = mmap.mmap();

        // Download chunks, bundle per bundle
        for (bundle_id, ranges) in bundle_ranges {
            let path = CdnDownloader::bundle_path(*bundle_id);
            //XXX use mmaping?
            let file = fs::File::open(self.conf.path.join(path))?;
            let mut reader = BufReader::new(file);

            for range in ranges {
                let (target_begin, target_end) = range.target;
                let (bundle_begin, bundle_end) = range.bundle;
                reader.seek(io::SeekFrom::Start(bundle_begin as u64))?;
                let reader = reader.by_ref().take((bundle_end - bundle_begin) as u64);
                let mut decoder = zstd::stream::Decoder::new(reader)?;
                decoder.read_exact(&mut buf[target_begin as usize .. target_end as usize])?;
            }
        }

        mmap.persist();
        Ok(())
    }

    /// Compute hash of an extracted file, from its chunks
    fn shared_file_hash(file_entry: &FileEntry) -> String {
        //XXX could be improved (or file hash format could change)
        let mut m = sha1_smol::Sha1::new();
        for chunk_id in file_entry.iter_chunks() {
            m.update(format!("{:016X}", chunk_id).as_bytes());
        }
        m.hexdigest()
    }
}


/// Get manifest ID from a path or URL
fn parse_manifest_id(url: &str) -> Result<u64> {
    let basename = url.rsplit('/').next().ok_or(StringError("cannot find basename of manifest in URL path".into()))?;
    if basename.len() != "0123456789ABCDEF.manifest".len() || !basename.ends_with(".manifest") {
        Err(StringError(format!("invalid manifest basename: {}", basename)).into())
    } else {
        let id = u64::from_str_radix(&basename[..16], 16)?;
        Ok(id)
    }
}

