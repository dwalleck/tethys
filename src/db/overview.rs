//! SQL queries that populate the [`crate::overview`] domain types.
//!
//! Each query method returns one overview layer. Only the error-flow layer is
//! implemented so far; see the module doc in [`crate::overview`].

use super::Index;
use crate::error::Result;
use crate::overview::{Fallibility, FallibleFunction};

impl Index {
    /// Layer 5: public functions/methods whose return type is structurally
    /// fallible (`Result`, `Option`, `OneOf`, or `Task`).
    ///
    /// Filters on the persisted `symbols.return_type` column rather than
    /// substring-matching the full signature. The signature records parameter
    /// types, so a function whose *parameter* is a `Result` (or any type naming
    /// a fallibility constructor) would otherwise be reported as fallible
    /// itself — see `takes_result_param` in `tests/error_flow.rs`.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be read.
    pub fn query_error_flow(&self) -> Result<Vec<FallibleFunction>> {
        let conn = self.connection()?;

        // Match fallibility shapes at the start of `return_type`, OR after a
        // `::` (Rust path qualifier), OR after a `.` (C# namespace qualifier).
        // Anchoring on `::` and `.` is what distinguishes a qualified type
        // constructor from a nested `Result<...>` inside a closure return
        // (`Box<dyn FnOnce() -> Result<_, _>>`) — the latter has a space before
        // `Result`, not a `::` or `.`, so the anchored patterns skip it.
        //
        // Known gap (tethys-wr7g): Rust type aliases (`type Foo = Result<...>`)
        // and `use`-renames (`use std::io::Result as IoResult`) still miss
        // silently, because this reconstructs type semantics from a string
        // column without the imports and aliases in scope.
        let mut stmt = conn.prepare(
            "SELECT s.qualified_name, s.signature, s.return_type
             FROM symbols s
             WHERE s.visibility = 'public'
               AND s.kind IN ('function', 'method')
               AND s.return_type IS NOT NULL
               AND (
                   -- Bare or angle-bracketed forms at the start of the string
                   s.return_type = 'Result' OR s.return_type LIKE 'Result<%'
                   OR s.return_type = 'Option' OR s.return_type LIKE 'Option<%'
                   OR s.return_type LIKE 'OneOf<%'
                   OR s.return_type LIKE 'Task<%'
                   -- Rust qualified paths: `std::io::Result<%>`, `anyhow::Result<%>`
                   OR s.return_type LIKE '%::Result' OR s.return_type LIKE '%::Result<%'
                   OR s.return_type LIKE '%::Option' OR s.return_type LIKE '%::Option<%'
                   OR s.return_type LIKE '%::OneOf<%'
                   OR s.return_type LIKE '%::Task<%'
                   -- C# qualified namespaces: `System.Threading.Tasks.Task<%>`
                   OR s.return_type LIKE '%.Result' OR s.return_type LIKE '%.Result<%'
                   OR s.return_type LIKE '%.Option' OR s.return_type LIKE '%.Option<%'
                   OR s.return_type LIKE '%.OneOf<%'
                   OR s.return_type LIKE '%.Task<%'
               )
             ORDER BY s.module_path, s.qualified_name",
        )?;

        let rows = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let signature: String = row.get(1)?;
            let return_type: String = row.get(2)?;
            Ok((name, signature, return_type))
        })?;

        let mut functions = Vec::new();
        for row in rows {
            let (name, signature, return_type) = row?;
            let fallibility = classify_fallibility(&return_type);
            functions.push(FallibleFunction {
                name,
                signature,
                return_type,
                fallibility,
            });
        }

        Ok(functions)
    }
}

/// Classify a bare return type string into a [`Fallibility`] category by its
/// outermost generic constructor.
///
/// Input is a bare return type like `Result<i32, Error>` or `Task<OneOf<A, B>>`
/// — no leading `-> ` and no surrounding function signature, matching what the
/// `symbols.return_type` column stores. Walks the string to find the first `<`,
/// then compares the prefix against the known fallibility shapes.
///
/// For the C# convention of wrapping fallible results in async — e.g.
/// `Task<OneOf<Success, Error>>` — the outer `Task` is stripped and the inner
/// type is classified instead. A plain `Task<int>` stays `Task`.
///
/// Logs an `error!` when the outermost type is unrecognized, because
/// [`Index::query_error_flow`]'s SQL filter should only admit known shapes.
fn classify_fallibility(return_type: &str) -> Fallibility {
    classify_inner(return_type).unwrap_or_else(|| {
        tracing::error!(
            return_type = %return_type,
            "classify_fallibility received return type that passed SQL filter \
             but matched no known fallibility shape"
        );
        Fallibility::Task
    })
}

/// Recursive helper that returns `None` for unknown outer types, so the
/// `Task<X>` recursion can treat a plain `X` as "not fallible" without logging
/// a false-positive anomaly for every `Task<int>`.
///
/// Strips any leading path qualifier from the outermost constructor before
/// matching, so `std::io::Result<()>` classifies as `Result` and
/// `System.Threading.Tasks.Task<int>` classifies as `Task`. Classifying types
/// by name rather than resolved identity is the design debt tracked by
/// tethys-wr7g.
fn classify_inner(return_type: &str) -> Option<Fallibility> {
    let trimmed = return_type.trim();
    let outer_end = trimmed.find('<').unwrap_or(trimmed.len());
    let outer_full = trimmed[..outer_end].trim();

    // Take the last segment after `::` (Rust) or `.` (C#). For a bare name
    // like `Result`, both `rsplit`s return the original string unchanged.
    let outer = outer_full.rsplit("::").next().unwrap_or(outer_full);
    let outer = outer.rsplit('.').next().unwrap_or(outer);

    match outer {
        "Result" => Some(Fallibility::Result),
        "Option" => Some(Fallibility::Option),
        "OneOf" => Some(Fallibility::OneOf),
        "Task" => {
            // When Task wraps a fallible shape, classify by the inner type.
            // A plain `Task<int>` (or a `Task` with no generic) stays `Task`.
            let inner = extract_inner_generic(trimmed)
                .and_then(classify_inner)
                .unwrap_or(Fallibility::Task);
            Some(inner)
        }
        _ => None,
    }
}

/// Given `Foo<A, B>`, return `A, B` (the generic argument list), or `None` when
/// there is no `<` or the angle brackets are unbalanced.
fn extract_inner_generic(s: &str) -> Option<&str> {
    let start = s.find('<')?;
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        match b {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start + 1..i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::bare_result("Result<()>", Fallibility::Result)]
    #[case::bare_option("Option<u32>", Fallibility::Option)]
    #[case::one_of("OneOf<Success, Error>", Fallibility::OneOf)]
    #[case::plain_task("Task<int>", Fallibility::Task)]
    #[case::csharp_task_of_oneof("Task<OneOf<Success, Error>>", Fallibility::OneOf)]
    #[case::rust_path_qualified("std::io::Result<()>", Fallibility::Result)]
    #[case::rust_path_qualified_option("core::option::Option<u32>", Fallibility::Option)]
    #[case::csharp_namespace_qualified(
        "System.Threading.Tasks.Task<OneOf<A, B>>",
        Fallibility::OneOf
    )]
    #[case::nested_result_of_option("Result<Option<u32>, Error>", Fallibility::Result)]
    #[case::nested_option_of_result("Option<Result<u32, Error>>", Fallibility::Option)]
    #[case::qualified_without_generics("std::io::Result", Fallibility::Result)]
    fn classifies_fallibility_by_outermost_constructor(
        #[case] return_type: &str,
        #[case] expected: Fallibility,
    ) {
        assert_eq!(classify_fallibility(return_type), expected);
    }

    /// Unknown outer types have no fallibility, which is what lets the `Task<X>`
    /// recursion treat `Task<int>`-style wrappers as plain `Task` rather than
    /// logging an anomaly.
    #[rstest]
    #[case::plain_type("u32")]
    #[case::unknown_wrapper("FooResult<i32>")]
    #[case::rust_alias("MyResult")]
    #[case::use_rename("IoResult<()>")]
    fn unknown_outer_types_are_not_fallible(#[case] return_type: &str) {
        assert_eq!(classify_inner(return_type), None);
    }
}
