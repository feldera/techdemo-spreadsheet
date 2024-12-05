#![warn(clippy::all, rust_2018_idioms)]
mod app;
mod cell_cache;
mod debouncer;
mod http;
pub use app::SpreadsheetApp;
