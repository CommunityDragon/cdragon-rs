
#![macro_use]
use std::rc::Rc;
use std::cell::{Cell, RefCell};
use cdragon_prop::BinHashMappers;
use crate::binloadservice::BinLoadService;
use crate::entrydb::EntryDatabase;

/// State shared with sub-components
#[derive(Default)]
pub struct SharedState {
    pub binload_service: Cell<BinLoadService>,
    pub hash_mappers: BinHashMappers,
    pub entrydb: EntryDatabase,
}

pub type SharedStateRef = Rc<RefCell<SharedState>>;

// Helper macros to access the shared state
// They could be implemented as a new trait with derive macro, but it's simplier this way.
// A `state: SharedStateRef` field is expected on the provided expression.
macro_rules! get_state { ($e:expr) => ($e.state.borrow()) }
macro_rules! get_state_mut { ($e:expr) => ($e.state.borrow_mut()) }
macro_rules! get_binload_service { ($e:expr) => (get_state_mut!($e).binload_service.get_mut()) }

