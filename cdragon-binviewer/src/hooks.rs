use std::rc::Rc;
use std::future::Future;
use yew::prelude::*;
use yew::platform::spawn_local;


pub struct UseAsyncHandle {
    run: Rc<dyn Fn()>,
}

impl UseAsyncHandle {
    pub fn run(&self) {
        (self.run)();
    }
}

/// Hook to execute a future
#[hook]
pub fn use_async<F>(future: F) -> UseAsyncHandle
where F: Future<Output=()> + 'static {
    let future = std::cell::Cell::new(Some(future));
    let run = Rc::new(move || {
        if let Some(f) = future.take() {
            spawn_local(f);
        }
    });
    UseAsyncHandle { run }
}

