//! Filesystem tools

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use memmap::MmapMut;

/// Open a temporary file for writing, remove it unless explicitely kept
///
/// Parent directory is created if needed.
/// File will be created with a temporary `.tmp` suffix.
/// The temporary file will only be removed on drop, not on Ctrl-C.
pub struct GuardedFile<P: AsRef<Path>> {
    // The Option is only there to be able to drop (and close) the file in drop().
    file: Option<File>,
    path: P,
}

impl<P: AsRef<Path>> GuardedFile<P> {
    /// Open file using given options
    ///
    /// Create parent directory if needed
    pub fn create(path: P) -> std::io::Result<Self> {
        let dirname = path.as_ref().parent().expect("invalid file name");
        fs::create_dir_all(dirname)?;

        let file = OpenOptions::new()
            .read(true).write(true).create(true).truncate(true)
            .open(Self::build_tmp_path(path.as_ref()))?;
        Ok(Self { file: Some(file), path })
    }

    /// Persist the temporary file
    pub fn persist(mut self) -> File {
        fs::rename(Self::build_tmp_path(self.path.as_ref()), self.path.as_ref()).expect("failed to persist file");
        self.file.take().unwrap()
    }

    /// Return a reference to the underlying file
    pub fn as_file_mut(&mut self) -> &mut File {
        self.file.as_mut().unwrap()
    }

    fn build_tmp_path(path: &Path) -> PathBuf {
        let mut s = path.as_os_str().to_owned();
        s.push(".tmp");
        s.into()
    }
}

impl<P: AsRef<Path>> Drop for GuardedFile<P> {
    fn drop(&mut self) {
        let _ = fs::remove_file(Self::build_tmp_path(self.path.as_ref()));  // ignore errors
        // note: file will be close afterwards
    }
}


/// Same as `GuardedFile`, but return a memory mapping
pub struct GuardedMmap<P: AsRef<Path>> {
    gfile: GuardedFile<P>,
    mmap: MmapMut,
}

impl<P: AsRef<Path>> GuardedMmap<P> {
    /// Open file and map it to memory
    ///
    /// Create parent directory if needed.
    pub fn create(path: P, size: u64) -> std::io::Result<Self> {
        let mut gfile = GuardedFile::create(path)?;
        let file = gfile.as_file_mut();
        file.set_len(size)?; 
        let mmap = unsafe { MmapMut::map_mut(file)? };
        Ok(Self { gfile, mmap })
    }

    /// Return a reference to the underlying memory buffer
    pub fn mmap(&mut self) -> &mut MmapMut {
        &mut self.mmap
    }

    /// Persist the memory mapped file
    pub fn persist(self) {
        self.gfile.persist();
    }
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

