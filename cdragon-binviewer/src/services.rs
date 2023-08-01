use std::rc::Rc;
use std::cell::RefCell;
use futures::try_join;
use gloo_console::{debug, error};
use gloo_net::http::Request;
use thiserror::Error;
use lru::LruCache;
use cdragon_prop::{
    PropFile,
    BinEntry,
    BinEntryPath,
    BinHashKind,
    BinHashKindMapping,
    BinHashMapper,
    BinHashMappers,
};
use crate::{
    entrydb::EntryDatabase,
    Result,
};

/// Gather services shared by all components
///
/// Static data must first be loaded. Then, data and `fetch_entry()` can be used.
/// To avoid mutability issues, a default state can be created then swapped with a loaded one.
pub struct Services {
    pub hmappers: BinHashMappers,
    pub entrydb: EntryDatabase,
    binfile_cache: RefCell<LruCache<String, Rc<Vec<u8>>>>,
}

impl Default for Services {
    fn default() -> Self {
        Self {
            hmappers: BinHashMappers::default(),
            entrydb: EntryDatabase::default(),
            binfile_cache: default_binfile_cache(),
        }
    }
}

impl Services {
    /// Load services data, asynchronously
    pub async fn load() -> Self {
        let future_hmappers = fetch_hash_mappers();
        let future_entrydb = fetch_entrydb();

        let hmappers = match future_hmappers.await {
            Ok(mappers) => {
                BinHashMappers {
                    entry_path: mappers.entry_path,
                    class_name: mappers.class_name,
                    field_name: mappers.field_name,
                    hash_value: mappers.hash_value,
                    path_value: Default::default(),
                }
            }
            Err(e) => {
                error!(format!("failed to load hash mappers: {}", e));
                Default::default()
            }
        };

        let entrydb = match future_entrydb.await {
            Ok(db) => db,
            Err(e) => {
                error!(format!("failed to load entry db: {}", e));
                Default::default()
            }
        };

        Self { hmappers, entrydb, binfile_cache: default_binfile_cache() }
    }

    /// Fetch an entry from given file, use cache if possible
    pub async fn fetch_entry(&self, file: &str, hpath: BinEntryPath) -> Result<BinEntry> {
        gloo_console::console_dbg!(file, hpath);
        let data = self.binfile_cache.borrow_mut().get(file).cloned();
        let data = match data {
            Some(data) => data,
            None => {
                let data = Rc::new(fetch_binfile(file).await?);
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
}


fn default_binfile_cache() -> RefCell<LruCache<String, Rc<Vec<u8>>>> {
    LruCache::new(std::num::NonZeroUsize::new(3).unwrap()).into()
}


type FetchedHashMappers = BinHashKindMapping<BinHashMapper, ()>;

/// Load hash mappers, asynchronously
async fn fetch_hash_mappers() -> Result<FetchedHashMappers> {
    let (entry_path, class_name, field_name, hash_value) = try_join!(
        fetch_one_hash_mapper(BinHashKind::EntryPath),
        fetch_one_hash_mapper(BinHashKind::ClassName),
        fetch_one_hash_mapper(BinHashKind::FieldName),
        fetch_one_hash_mapper(BinHashKind::HashValue),
    )?;
    Ok(FetchedHashMappers {
        entry_path, class_name, field_name, hash_value,
        path_value: ()
    })
}

async fn fetch_one_hash_mapper(kind: BinHashKind) -> Result<BinHashMapper> {
    let uri = static_uri!("hashes/{}", kind.mapper_path());
    let data = Request::get(&uri)
        .send().await?
        .binary().await?;
    let hmap = BinHashMapper::from_reader(data.as_slice())?;
    Ok(hmap)
}

/// Load the entry database, asynchronously
async fn fetch_entrydb() -> Result<EntryDatabase> {
    let uri = static_uri!("entries.db");
    let data = Request::get(&uri)
        .send().await?
        .binary().await?;
    EntryDatabase::load(data.as_slice())
}


/// Fetch a bin file, asynchronously
async fn fetch_binfile(file: &str) -> Result<Vec<u8>> {
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


#[derive(Error, Debug)]
pub enum BinLoadError {
    #[error("entry not found: {0:?}")]
    EntryNotFound(BinEntryPath),
    #[error("HTTP error ({0})")]
    HttpError(u16),
}

