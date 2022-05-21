use std::fmt;
use std::rc::Rc;
use std::cell::RefCell;
use futures::try_join;
use log::debug;
use reqwasm::http::Request;
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


#[derive(Debug)]
pub enum BinLoadError {
    EntryNotFound,
}

impl fmt::Display for BinLoadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BinLoadError::EntryNotFound => write!(f, "entry not found"),
        }
    }
}

impl std::error::Error for BinLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}


/// Cached binfile data, to avoid refetching it
type BinDataCache = Rc<(String, Vec<u8>)>;

pub type FetchedHashMappers = BinHashKindMapping<BinHashMapper, ()>;

/// Load bin entries
#[derive(Default)]
pub struct BinLoadService {
    cached_binfile: Rc<RefCell<Option<BinDataCache>>>,
}

impl BinLoadService {
    /// Fetch the entry from given file with given path
    pub async fn fetch_entry(&mut self, file: &str, hpath: BinEntryPath) -> Result<BinEntry> {
        let data = if let Some(cache) = self.cached_binfile.borrow().clone() {
            cache
        } else {
            self.fetch_binfile(file).await?
        };

        debug!("scanning bin file for entry {:x}", hpath);
        let scanner = PropFile::scan_entries_from_reader(data.1.as_slice())?;
        match scanner.filter_parse(|h, _| h == hpath).next() {
            Some(v) => Ok(v?),
            None => Err(BinLoadError::EntryNotFound)?,
        }
    }

    async fn fetch_binfile(&self, file: &str) -> Result<BinDataCache> {
        let uri = static_uri!("bins/{}", file);
        let data = Request::get(&uri)
            .send().await?
            .binary().await?;
        Ok(BinDataCache::new((file.to_string(), data)))
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
        BinHashMapper::from_reader(data.as_slice())
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

