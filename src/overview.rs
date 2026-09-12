//! Types for the budget-aware `tethys overview` command.
//!
//! The overview is a layered architectural summary for LLM consumption during
//! initial project orientation. Only the error-flow layer is implemented so
//! far; the module-tree, trait-map, public-API, and entry-point layers land
//! with their own queries, together with the aggregate `Overview` record and
//! the CLI command.

use serde::Serialize;

/// A function or method whose return type is structurally fallible.
#[derive(Debug, Clone, Serialize)]
pub struct FallibleFunction {
    /// Qualified name.
    pub name: String,
    /// Full signature string (for display and JSON output).
    pub signature: String,
    /// Bare return type — the type after `->`, without any wrapping.
    ///
    /// Sourced from the persisted `symbols.return_type` column so the correct
    /// return type survives a signature that contains closure parameters with
    /// their own `->` arrows.
    pub return_type: String,
    /// The error/fallible type category.
    pub fallibility: Fallibility,
}

/// How a function can fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Fallibility {
    /// Returns `Result<T, E>`.
    Result,
    /// Returns `Option<T>`.
    Option,
    /// Returns `OneOf<...>` (C# discriminated union pattern).
    OneOf,
    /// Returns `Task<T>` (C# async).
    Task,
}
