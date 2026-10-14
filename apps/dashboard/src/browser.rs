use dioxus::prelude::*;
use dioxus_web::launch::launch_cfg;

use crate::app::WebApp;

fn browser_root() -> Element {
    rsx! {
        WebApp { initial_path: "/".to_owned() }
    }
}

#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    launch_cfg(
        browser_root,
        dioxus_web::Config::new().hydrate(true).rootname("main"),
    );
}
