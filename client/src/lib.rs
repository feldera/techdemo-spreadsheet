#![warn(clippy::all, rust_2018_idioms)]

mod app;
mod cell_cache;
mod http;
mod debouncer;
pub use app::SpreadsheetApp;
