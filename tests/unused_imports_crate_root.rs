//! Import-side fences for bare-`crate` resolution (tethys-3i35 slice 4,
//! design claim C7 — implements tethys-xzdr's acceptance criteria).
//!
//! Pre-fix, `resolve_import_segments(["crate"])` returned the `src/`
//! directory, so `classify_confidence` could never look the name up and
//! downgraded every unused `use crate::X;` to `MaybeTrait` (hidden by
//! default). Post-fix the lookup lands in the crate-root file.

mod common;

use common::workspace_with_files;
use tethys::UnusedImportConfidence;

/// C7 (tethys-xzdr AC): an unused crate-root import of a NON-trait symbol
/// is Definite; an unused crate-root import of a TRAIT stays `MaybeTrait`
/// (invisible trait-method use is still possible); a USED crate-root
/// import produces no finding at all.
///
/// Ground truth: `cargo check` on this shape warns `unused_imports` for
/// `Foo` and `Greet` but not `Bar` (compiler = the oracle; recorded during
/// the tethys-3i35 design falsifiers).
///
/// Buggy impl this kills: a fix scoped to `qualified_splits` only (import
/// side keeps the directory, `Foo` stays `MaybeTrait`); a confidence upgrade
/// that misfires on used imports (`Bar` would appear as a finding).
#[test]
fn unused_crate_root_import_is_definite() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod inner;\n\npub struct Foo;\npub struct Bar;\n\
             pub trait Greet {\n    fn hi(&self) {}\n}\n",
        ),
        (
            "src/inner.rs",
            "use crate::Foo;\nuse crate::Bar;\nuse crate::Greet;\n\n\
             pub fn takes(_b: &Bar) {}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    let findings = tethys.find_unused_imports().expect("analysis failed");

    let foo = findings
        .iter()
        .find(|f| f.name == "Foo")
        .expect("unused `use crate::Foo;` must be reported");
    assert_eq!(
        foo.confidence,
        UnusedImportConfidence::Definite,
        "a non-trait crate-root symbol must classify Definite (xzdr AC), \
         not the pre-fix MaybeTrait downgrade"
    );
    assert_eq!(foo.source_module, "crate");

    let greet = findings
        .iter()
        .find(|f| f.name == "Greet")
        .expect("unused `use crate::Greet;` must be reported");
    assert_eq!(
        greet.confidence,
        UnusedImportConfidence::MaybeTrait,
        "a trait resolved at the crate root must stay MaybeTrait \
         (invisible trait-method use)"
    );

    assert!(
        !findings.iter().any(|f| f.name == "Bar"),
        "Bar is used in a type position — the confidence upgrade must not \
         turn used imports into findings; got {findings:?}"
    );
}

/// C7 second half: the bare-crate import now RESOLVES during dependency
/// computation, so the fixture reports zero unresolved dependencies (the
/// probe's pre-fix repro reported exactly one — the `use crate::helper;`).
#[test]
fn crate_root_import_is_not_an_unresolved_dependency() {
    let (_dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "pub mod inner;\n\npub fn helper() {}\n"),
        (
            "src/inner.rs",
            "use crate::helper;\n\npub fn go() {\n    helper();\n}\n",
        ),
    ]);
    let stats = tethys.index().expect("index failed");
    assert_eq!(
        stats.unresolved_dependencies,
        vec![],
        "the bare-crate import must resolve during dependency computation"
    );
}
