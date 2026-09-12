//! Layer 5 of the overview (`query_error_flow`) must select fallible public
//! functions from the **parsed** index, not from a hand-built fixture.
//!
//! The distinguishing cases are the ones a signature substring-match gets
//! wrong: a function that merely *takes* a closure returning `Result`, a
//! private function that does return `Result`, and non-callable symbols.

use std::collections::BTreeMap;
use std::fs;

use tethys::{Fallibility, IndexOptions, Tethys};

const RUST_SOURCE: &str = r"
pub fn fallible() -> Result<(), String> { Ok(()) }
pub fn qualified() -> std::io::Result<()> { Ok(()) }
pub fn optional() -> Option<u32> { None }
pub fn infallible(x: u32) -> u32 { x }
pub fn takes_closure<F: Fn() -> Result<(), String>>(f: F) -> u32 { let _ = f; 42 }
/// The signature records parameter types, so `Result<` appears in `signature`
/// while the return type is plainly `u32`. Selecting on the signature instead
/// of `return_type` would wrongly report this as fallible.
pub fn takes_result_param(r: Result<u32, String>) -> u32 { let _ = r; 0 }
fn private_result() -> Result<(), String> { Ok(()) }
pub struct NotAFunction;
";

const CSHARP_SOURCE: &str = r"
public class Widget {
    public Task<OneOf<Success, Error>> Fetch() { return null; }
    public int Count() { return 0; }
}
";

fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temporary workspace");
    fs::create_dir(dir.path().join("src")).expect("source directory");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"error_flow_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("manifest");
    fs::write(dir.path().join("src/lib.rs"), RUST_SOURCE).expect("rust source");
    fs::write(dir.path().join("src/Widget.cs"), CSHARP_SOURCE).expect("csharp source");
    dir
}

fn by_name(tethys: &Tethys) -> BTreeMap<String, (String, Fallibility)> {
    tethys
        .query_error_flow()
        .expect("error flow query")
        .into_iter()
        .map(|entry| (entry.name, (entry.return_type, entry.fallibility)))
        .collect()
}

#[test]
fn error_flow_selects_fallible_public_callables_from_the_parsed_index() {
    for (mode, options) in [
        ("batch", IndexOptions::default()),
        ("streaming", IndexOptions::with_streaming()),
    ] {
        let dir = workspace();
        let mut tethys = Tethys::new(dir.path()).expect("open index");
        tethys.index_with_options(options).expect("index workspace");

        let found = by_name(&tethys);

        for (name, expected_return, expected_fallibility) in [
            ("fallible", "Result<(), String>", Fallibility::Result),
            ("qualified", "std::io::Result<()>", Fallibility::Result),
            ("optional", "Option<u32>", Fallibility::Option),
            // C# members are reported by qualified name (`Type::member`).
            (
                "Widget::Fetch",
                "Task<OneOf<Success, Error>>",
                Fallibility::OneOf,
            ),
        ] {
            let (return_type, fallibility) = found
                .get(name)
                .unwrap_or_else(|| panic!("{name} should be reported as fallible ({mode})"));
            assert_eq!(return_type, expected_return, "{name} return type ({mode})");
            assert_eq!(
                *fallibility, expected_fallibility,
                "{name} fallibility ({mode})"
            );
        }

        // The false positives a signature substring-match would produce.
        for absent in [
            "infallible",
            "takes_closure",
            "takes_result_param",
            "private_result",
            "Widget::Count",
        ] {
            assert!(
                !found.contains_key(absent),
                "{absent} must not be reported as fallible ({mode}); found {:?}",
                found.get(absent)
            );
        }

        // Non-callable symbols can never be fallible.
        assert!(
            !found.contains_key("NotAFunction"),
            "a struct must not be reported as fallible ({mode})"
        );
    }
}

/// The reported return type must be the bare type, not the whole signature —
/// the signature of `takes_closure` contains its own `->` for the closure
/// parameter, which is exactly what makes substring matching unreliable.
#[test]
fn reported_return_type_is_bare_not_the_signature() {
    let dir = workspace();
    let mut tethys = Tethys::new(dir.path()).expect("open index");
    tethys.index().expect("index workspace");

    let entries = tethys.query_error_flow().expect("error flow query");
    let closure = entries.iter().find(|entry| entry.name == "takes_closure");
    assert!(
        closure.is_none(),
        "takes_closure returns u32, so it is not fallible"
    );

    let fallible = entries
        .iter()
        .find(|entry| entry.name == "fallible")
        .expect("fallible function");
    assert_eq!(fallible.return_type, "Result<(), String>");
    assert!(
        fallible.signature.contains("fn fallible()"),
        "signature is retained for display: {}",
        fallible.signature
    );
}
