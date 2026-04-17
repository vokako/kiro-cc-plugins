//! Kiro CC Plugins core library.
//!
//! Port of the Python `kiro_cc_plugin_converter` package to Rust.

pub mod models;
pub mod source;
pub mod scanner;
pub mod converter;
pub mod registry;

pub use models::{Scope, Source, InstalledPlugin, ComponentRecord, Error, Result};
