#[cfg(any(feature = "browser", test))]
mod state;
mod ui;

pub(crate) use ui::ServiceBriefPage;
