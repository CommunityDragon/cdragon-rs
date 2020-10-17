use std::path::Path;
use cdragon_utils::GuardedFile;
use memmap::MmapMut;

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

