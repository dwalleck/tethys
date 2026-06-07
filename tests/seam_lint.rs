//! Source-lint fences for the `ModuleResolver` seam (separator-fix claims
//! C4, C5, C10).
//!
//! These greps are the permanent CI form of the design's one-shot
//! falsifiers: a regression re-introducing language-specific module
//! semantics into the neutral drivers, or DB access into the resolver
//! implementations, fails here. TDD-inversion verified at introduction:
//! the slice-3-era resolve.rs (git d5cb3d3) contains 9 matches for the C4
//! pattern and would fail `resolve_rs_contains_no_rust_module_semantics`.

const RESOLVE_RS: &str = include_str!("../src/resolve.rs");
const INDEXING_RS: &str = include_str!("../src/indexing.rs");
const BATCH_WRITER_RS: &str = include_str!("../src/batch_writer.rs");
const MODULE_RESOLVER_RS: &str = include_str!("../src/languages/module_resolver.rs");

/// C4: the Pass-2 driver is language-neutral. Rust module semantics —
/// direct `resolve_module_path` calls, `CrateInfo` handling, and the
/// `"crate"`/`"self"`/`"super"` path-keyword string literals — live only
/// in the `ModuleResolver` implementations.
#[test]
fn resolve_rs_contains_no_rust_module_semantics() {
    for needle in [
        "resolve_module_path",
        "CrateInfo",
        "\"crate\"",
        "\"self\"",
        "\"super\"",
    ] {
        assert!(
            !RESOLVE_RS.contains(needle),
            "src/resolve.rs contains '{needle}' — Rust module semantics \
             belong in the ModuleResolver impls, not the neutral driver"
        );
    }
}

/// C5: dependency computation in indexing.rs resolves imports through the
/// seam, never through `resolve_module_path` directly.
#[test]
fn indexing_rs_contains_no_direct_module_path_resolution() {
    assert!(
        !INDEXING_RS.contains("resolve_module_path"),
        "src/indexing.rs calls resolve_module_path directly — import \
         resolution must go through the file's ModuleResolver"
    );
}

/// C10: resolver implementations are DB-free — candidate enumeration and
/// index lookup stay separable (the driver owns all DB access). Matches
/// the import form and the connection-handle type; `crate::db` as a
/// module-path EXAMPLE in docs and test strings is fine.
#[test]
fn module_resolver_impls_are_db_free() {
    for needle in ["use crate::db", "&Index"] {
        assert!(
            !MODULE_RESOLVER_RS.contains(needle),
            "module_resolver.rs contains '{needle}' — resolver impls must \
             stay DB-free (filesystem probing only)"
        );
    }
}

/// Import-format joins go through `ModuleResolver::join_import` — a raw
/// `.join(".")` in driver code is a seam bypass (the bug class PR-review
/// finding I1 caught at the pre-fix indexing.rs namespace join). Canonical
/// `::` joins (qualified names, module paths) are a different concept
/// (spec decision #5) and remain legitimate, so this lint polices the
/// dotted form only.
#[test]
fn import_joins_go_through_the_seam() {
    for (name, src) in [
        ("src/resolve.rs", RESOLVE_RS),
        ("src/indexing.rs", INDEXING_RS),
        ("src/batch_writer.rs", BATCH_WRITER_RS),
    ] {
        assert!(
            !src.contains(".join(\".\")"),
            "{name} joins segments on '.' directly — import-format joins \
             must go through ModuleResolver::join_import"
        );
    }
}
