use std::rc::Rc;
use std::cell::RefCell;
use futures::try_join;
use gloo_console::debug;
use gloo_net::http::Request;
use thiserror::Error;
use cdragon_prop::{
    PropFile,
    BinEntry,
    BinEntryPath,
    BinHashKind,
    BinHashKindMapping,
    BinHashMapper,
};
use lru::LruCache;
use crate::Result;
use crate::entrydb::EntryDatabase;


#[derive(Error, Debug)]
pub enum BinLoadError {
    #[error("entry not found: {0:?}")]
    EntryNotFound(BinEntryPath),
    #[error("HTTP error ({0})")]
    HttpError(u16),
}


pub type FetchedHashMappers = BinHashKindMapping<BinHashMapper, ()>;

/// Load bin entries, hashes, and entry database
///
/// This service does not hold a state (aside its cache) and thus can be cloned.
/// It is not a problem to create new instances of it.
/// In practice, it will not be cloned past the initial loading.
pub struct BinLoadService {
    binfile_cache: RefCell<LruCache<String, Rc<Vec<u8>>>>,
}

impl Clone for BinLoadService {
    fn clone(&self) -> Self {
        debug!("Cloning BinLoadService");
        // Just return a new instance
        Self::default()
    }
}

impl Default for BinLoadService {
    fn default() -> Self {
        Self {
            // Note: cache size values have not been tweaked
            binfile_cache: LruCache::new(std::num::NonZeroUsize::new(3).unwrap()).into(),
        }
    }
}


impl BinLoadService {
    /// Fetch the entry from given file with given path
    pub async fn fetch_entry(&self, file: &str, hpath: BinEntryPath) -> Result<BinEntry> {
        gloo_console::console_dbg!(file, hpath);
        let data = self.binfile_cache.borrow_mut().get(file).cloned();
        let data = match data {
            Some(data) => data,
            None => {
                let data = Rc::new(self.fetch_binfile(file).await?);
                self.binfile_cache.borrow_mut().put(file.into(), data.clone());
                data
            }
        };

        debug!(format!("scanning bin file for entry: {:?}", hpath));
        let scanner = PropFile::scan_entries_from_reader(data.as_slice())?;
        match scanner.filter_parse(|h, _| h == hpath).next() {
            Some(v) => Ok(v?),
            None => Err(BinLoadError::EntryNotFound(hpath))?,
        }
    }

    async fn fetch_binfile(&self, file: &str) -> Result<Vec<u8>> {
        debug!("fetching bin file", file);
        let uri = static_uri!("bins/{}", file);
        let response = Request::get(&uri).send().await?;
        if response.ok() {
            let data = response.binary().await?;
            Ok(data)
        } else {
            Err(BinLoadError::HttpError(response.status()).into())
        }
    }

    /// Load all hash maps in the background, the callback will be called once for each map
    pub async fn fetch_hash_maps(&self) -> Result<FetchedHashMappers> {
        let (entry_path, class_name, field_name, hash_value) = try_join!(
            self.fetch_single_hash_map(BinHashKind::EntryPath),
            self.fetch_single_hash_map(BinHashKind::ClassName),
            self.fetch_single_hash_map(BinHashKind::FieldName),
            self.fetch_single_hash_map(BinHashKind::HashValue),
        )?;
        Ok(FetchedHashMappers {
            entry_path, class_name, field_name, hash_value,
            path_value: ()
        })
    }

    async fn fetch_single_hash_map(&self, kind: BinHashKind) -> Result<BinHashMapper> {
        let uri = static_uri!("hashes/{}", kind.mapper_path());
        let data = Request::get(&uri)
            .send().await?
            .binary().await?;
        let hmap = BinHashMapper::from_reader(data.as_slice())?;
        Ok(hmap)
    }

    /// Load the entry database in the background
    pub async fn fetch_entrydb(&self) -> Result<EntryDatabase> {
        let uri = static_uri!("entries.db");
        let data = Request::get(&uri)
            .send().await?
            .binary().await?;
        EntryDatabase::load(data.as_slice())
    }

}

