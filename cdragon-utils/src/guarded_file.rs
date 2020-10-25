use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

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

