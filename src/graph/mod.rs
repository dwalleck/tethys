//! Graph-specific query result types.
//!
//! Graph analyses are exposed through [`crate::Tethys`]. Their SQLite-backed
//! query implementations are concrete operations on `crate::db::Index`.

mod types;

pub use types::{FileDepInfo, FileImpact, FilePath, SymbolImpact, SymbolImpactCaller};
