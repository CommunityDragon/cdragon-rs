use gloo_console::error;
use cdragon_prop::BinHashMappers;
use crate::binloadservice::BinLoadService;
use crate::entrydb::EntryDatabase;

/// Services shared by all components, created by the root
///
/// `hash_mappers` and `entrydb` must first be loaded (using `binload_service`).
/// To avoid issues mutability issues, a default state is used then replaced by the "loaded" one
/// when ready. As a result, it is not possible to use the state until its completely loaded, but
/// that should be fine. An alternative could be to use `Rc` for loaded fields.
/// Also, the BinLoadService could be separated from the rest, but it is more convenient to have
/// everything together.
#[derive(Default)]
pub struct Services {
    pub binload_service: BinLoadService,
    pub hash_mappers: BinHashMappers,
    pub entrydb: EntryDatabase,
}

impl Services {
    /// Load a state, asynchronously
    pub async fn load() -> Self {
        let binload_service = BinLoadService::default();

        let future_hmappers = binload_service.fetch_hash_maps();
        let future_entrydb = binload_service.fetch_entrydb();

        let hash_mappers = match future_hmappers.await {
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

        Services { binload_service, hash_mappers, entrydb }
    }
}

