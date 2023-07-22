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
use crate::Result;
use crate::entrydb::EntryDatabase;


#[derive(Error, Debug)]
pub enum BinLoadError {
    #[error("entry not found: {0:?}")]
    EntryNotFound(BinEntryPath),
    #[error("HTTP error ({0})")]
    HttpError(u16),
}


/// Cached binfile data, to avoid refetching it
type BinDataCache = Rc<(String, Vec<u8>)>;

pub type FetchedHashMappers = BinHashKindMapping<BinHashMapper, ()>;

/// Load bin entries, hashes, and entry database
///
/// This service does not hold a state (aside its cached) and thus can be cloned.
/// It is not a problem to create new insteances of it.
#[derive(Default)]
pub struct BinLoadService {
    cached_binfile: RefCell<Option<BinDataCache>>,
}

impl Clone for BinLoadService {
    fn clone(&self) -> Self {
        Self { cached_binfile: RefCell::default() }
    }
}


impl BinLoadService {
    /// Fetch the entry from given file with given path
    pub async fn fetch_entry(&self, file: &str, hpath: BinEntryPath) -> Result<BinEntry> {
        gloo_console::console_dbg!(file, hpath);
        let data = {
            let cache = self.cached_binfile.borrow().clone();
            if let Some(data) = cache.filter(|d| d.0 == file) {
                data
            } else {
                let data = self.fetch_binfile(file).await?;
                self.cached_binfile.replace(Some(data.clone()));
                data
            }
        };

        debug!(format!("scanning bin file for entry: {:?}", hpath));
        let scanner = PropFile::scan_entries_from_reader(data.1.as_slice())?;
        match scanner.filter_parse(|h, _| h == hpath).next() {
            Some(v) => Ok(v?),
            None => Err(BinLoadError::EntryNotFound(hpath))?,
        }
    }

    async fn fetch_binfile(&self, file: &str) -> Result<BinDataCache> {
        debug!("fetching bin file", file);
        let uri = static_uri!("bins/{}", file);
        let response = Request::get(&uri).send().await?;
        if response.ok() {
            let data = response.binary().await?;
            Ok(BinDataCache::new((file.to_string(), data)))
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

