/// The [`TeraWeb`](app::TeraWeb) application state and its builder.
pub mod app;

/// The file watcher that drives live reload.
#[cfg(feature = "devel")]
pub mod reload;