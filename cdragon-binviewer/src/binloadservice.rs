use std::fmt;
use std::rc::Rc;
use std::cell::RefCell;
use log::debug;
use yew::callback::Callback;
use yew::services::Task;
use yew::format::{Binary, Nothing};
use yew::services::fetch::{FetchService, Request, Response, FetchTask, StatusCode};
use cdragon_prop::{
    PropFile,
    BinEntryPath,
    BinEntry,
    BinHashKind,
    BinHashKindMapping,
    BinHashMapper,
};
use crate::Result;
use crate::entrydb::EntryDatabase;


#[derive(Debug)]
pub enum BinLoadError {
    FetchFailed(StatusCode),
    EntryNotFound,
}

impl fmt::Display for BinLoadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BinLoadError::FetchFailed(code) => write!(f, "failed to fetch data: {}", code),
            BinLoadError::EntryNotFound => write!(f, "entry not found"),
        }
    }
}

impl std::error::Error for BinLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}



#[must_use]
pub struct BinFetchTask(Option<FetchTask>);

impl Task for BinFetchTask {
    fn is_active(&self) -> bool {
        if let Some(t) = &self.0 {
            t.is_active()
        } else {
            false  // not cancelable
        }
    }

    fn cancel(&mut self) {
        if let Some(t) = &mut self.0 {
            t.cancel()
        } else {
            // not cancelable
        }
    }
}

impl Drop for BinFetchTask {
    fn drop(&mut self) {
        if self.is_active() {
            self.cancel();
        }
    }
}


//TODO handle path_value
#[must_use]
pub struct HashMappersFetchTask(BinHashKindMapping<FetchTask, ()>);

impl Task for HashMappersFetchTask {
    fn is_active(&self) -> bool {
        for kind in BinHashKind::variants() {
            if self.0.get(kind).is_active() {
                return false;
            }
        }
        true
    }

    fn cancel(&mut self) {
        for kind in BinHashKind::variants() {
            self.0.get_mut(kind).cancel();
        }
    }
}

impl Drop for HashMappersFetchTask {
    fn drop(&mut self) {
        for kind in BinHashKind::variants() {
            let task = self.0.get_mut(kind);
            if task.is_active() {
                task.cancel();
            }
        }
    }
}


#[must_use]
pub type EntryDbFetchTask = FetchTask;


/// Cached binfile data, to avoid refetching it
type BinDataCache = Rc<(String, Vec<u8>)>;

/// Load bin entries
#[derive(Default)]
pub struct BinLoadService {
    cached_binfile: Rc<RefCell<Option<BinDataCache>>>,
    fetch_service: FetchService,
}

impl BinLoadService {
    /// Fetch the entry from given file with given path
    pub fn fetch_entry(&mut self, file: &str, hpath: BinEntryPath, callback: Callback<Result<BinEntry>>) -> BinFetchTask {
        let callback = callback.reform(move |data: Result<BinDataCache>| {
            debug!("scanning bin file for entry {:x}", hpath);
            let data = data?;
            let scanner = PropFile::scan_entries_from_reader(data.1.as_slice())?;
            match scanner.filter_parse(|h, _| h == hpath).next() {
                Some(v) => Ok(v?),
                None => Err(BinLoadError::EntryNotFound)?,
            }
        });

        // If binfile is in cache, process now
        if let Some(cache) = self.cached_binfile.borrow().clone() {
            if cache.0 == file {
                callback.emit(Ok(cache));
                return BinFetchTask(None);
            }
        }

        self.fetch_binfile(file, callback)
    }

    fn fetch_binfile(&mut self, file: &str, callback: Callback<Result<BinDataCache>>) -> BinFetchTask {
        let uri = static_uri!("bins/{}", file);
        let request = Request::get(uri).body(Nothing).unwrap();
        let file_str = file.to_string();

        let cached_binfile = self.cached_binfile.clone();
        let callback = callback.reform(move |response: Response<Binary>| {
            let (meta, binary) = response.into_parts();
            if let (true, Ok(data)) = (meta.status.is_success(), binary) {
                let cache = BinDataCache::new((file_str.clone(), data));
                cached_binfile.replace(Some(cache.clone()));
                Ok(cache)
            } else {
                Err(BinLoadError::FetchFailed(meta.status))?
            }
        });

        BinFetchTask(Some(self.fetch_service.fetch_binary(request, callback)))
    }

    /// Load all hash maps in the background, the callback will be called once for each map
    pub fn fetch_hash_maps(&mut self, callback: Callback<Result<(BinHashKind, BinHashMapper)>>) -> HashMappersFetchTask {
        HashMappersFetchTask(BinHashKindMapping {
            entry_path: self.fetch_single_hash_map(BinHashKind::EntryPath, callback.clone()),
            class_name: self.fetch_single_hash_map(BinHashKind::ClassName, callback.clone()),
            field_name: self.fetch_single_hash_map(BinHashKind::FieldName, callback.clone()),
            hash_value: self.fetch_single_hash_map(BinHashKind::HashValue, callback),
            path_value: (),
        })
    }

    fn fetch_single_hash_map(&mut self, kind: BinHashKind, callback: Callback<Result<(BinHashKind, BinHashMapper)>>) -> FetchTask {
        let uri = static_uri!("hashes/{}", kind.mapper_path());
        let request = Request::get(uri).body(Nothing).unwrap();
        let callback = callback.reform(move |response: Response<Binary>| {
            let (meta, binary) = response.into_parts();
            if let (true, Ok(data)) = (meta.status.is_success(), binary) {
                let mapper = BinHashMapper::from_reader(data.as_ref())?;
                Ok((kind, mapper))
            } else {
                Err(BinLoadError::FetchFailed(meta.status))?
            }
        });
        self.fetch_service.fetch_binary(request, callback)
    }

    /// Load the entry database in the background
    pub fn fetch_entrydb(&mut self, callback: Callback<Result<EntryDatabase>>) -> EntryDbFetchTask {
        let request = Request::get(static_uri!("entries.db")).body(Nothing).unwrap();
        let callback = callback.reform(move |response: Response<Binary>| {
            let (meta, binary) = response.into_parts();
            if let (true, Ok(data)) = (meta.status.is_success(), binary) {
                let db = EntryDatabase::load(data.as_slice())?;
                Ok(db)
            } else {
                Err(BinLoadError::FetchFailed(meta.status))?
            }
        });
        self.fetch_service.fetch_binary(request, callback)
    }

}

