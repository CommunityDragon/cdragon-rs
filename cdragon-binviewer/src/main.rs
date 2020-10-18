use cdragon_binviewer::Model;

#[cfg(target_arch = "wasm32")]
fn main() {
    web_logger::init();
    yew::start_app::<Model>();
}
