use std::rc::Rc;

use dioxus::history::History;
#[cfg(not(all(target_arch = "wasm32", feature = "browser")))]
use dioxus::history::MemoryHistory;
use dioxus::prelude::*;
use dioxus::router::components::HistoryProvider;

use crate::routes::RouteView;

#[component]
pub(crate) fn WebApp(initial_path: String) -> Element {
    let initial_path = initial_path.clone();

    rsx! {
        HistoryProvider {
            history: move |_| history_for_path(initial_path.clone()),
            Router::<RouteView> {}
        }
    }
}

#[cfg(all(target_arch = "wasm32", feature = "browser"))]
fn history_for_path(_initial_path: String) -> Rc<dyn History> {
    Rc::new(dioxus_web::WebHistory::default())
}

#[cfg(not(all(target_arch = "wasm32", feature = "browser")))]
fn history_for_path(initial_path: String) -> Rc<dyn History> {
    Rc::new(MemoryHistory::with_initial_path(initial_path))
}
