//! Golden-content regression fence for the one-transaction-per-file write
//! path (idxperf design claims C1/C2/C3/C7, plan slice 4).
//!
//! The idxperf loop's empirical falsifiers (frozen-tree canonical dump
//! equality vs a pre-change binary) are one-shot measurements; this test is
//! their permanent CI form. It indexes a small mixed-language fixture and
//! asserts the EXACT canonical row set — ids replaced by natural keys,
//! volatile columns excluded — for the batch arm, and the documented
//! batch↔streaming divergences for the streaming arms.
//!
//! The in-test canonical dump deliberately duplicates
//! `.idxperf/probe-dump.py`'s row format (cross-checked against it when this
//! fence was authored): the fence must not depend on python in CI.

use std::path::Path;

use rusqlite::Connection;
use tempfile::TempDir;
use tethys::{IndexOptions, Tethys};

const CARGO_TOML: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

/// Cross-file call + pub use re-export + top-level type ref (the d4d87f1
/// accumulating shape) + a fn name duplicated across files.
const LIB_RS: &str = "pub mod util;\n\
pub use util::shared_fn;\n\
pub type Alias = ExternalThing;\n\
pub fn caller() {\n    util::shared_fn();\n}\n";

const UTIL_RS: &str = "pub fn shared_fn() {}\n\
pub fn caller() {}\n";

const LIB_CS: &str = "namespace App.Cs\n{\n    public static class Helper\n    {\n        public static void Assist() { }\n    }\n}\n";

const USE_CS: &str = "using App.Cs;\n\nnamespace App.Use\n{\n    public class U\n    {\n        public void Go()\n        {\n            Helper.Assist();\n        }\n    }\n}\n";

fn write_fixture(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mkdir src");
    std::fs::create_dir_all(root.join("cs")).expect("mkdir cs");
    std::fs::write(root.join("Cargo.toml"), CARGO_TOML).expect("Cargo.toml");
    std::fs::write(root.join("src/lib.rs"), LIB_RS).expect("lib.rs");
    std::fs::write(root.join("src/util.rs"), UTIL_RS).expect("util.rs");
    std::fs::write(root.join("cs/Lib.cs"), LIB_CS).expect("Lib.cs");
    std::fs::write(root.join("cs/Use.cs"), USE_CS).expect("Use.cs");
}

/// Canonical dump of every table, ids replaced by natural keys, volatile
/// columns (`indexed_at`, `mtime_ns`) excluded, sorted, duplicates preserved.
/// Row formats mirror `.idxperf/probe-dump.py`.
#[expect(
    clippy::too_many_lines,
    reason = "one query block per table; splitting hides the dump's completeness"
)]
fn canonical_rows(db_path: &Path) -> Vec<String> {
    let conn = Connection::open(db_path).expect("open db");
    let mut out = Vec::new();

    let mut files: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, path, language, size_bytes FROM files")
            .expect("prep files");
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .expect("files");
        for row in rows {
            let (id, path, lang, size) = row.expect("file row");
            out.push(format!("file|{path}|{lang}|{size}"));
            files.insert(id, path);
        }
    }

    let mut syms: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, file_id, line, column, name FROM symbols")
            .expect("prep symkeys");
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .expect("symkeys");
        for row in rows {
            let (id, fid, line, col, name) = row.expect("sym key row");
            let fpath = files.get(&fid).cloned().unwrap_or_else(|| "?".into());
            syms.insert(id, format!("{fpath}:{line}:{col}:{name}"));
        }
    }
    let symkey = |id: Option<i64>, syms: &std::collections::HashMap<i64, String>| -> String {
        id.map_or(String::new(), |i| {
            syms.get(&i)
                .cloned()
                .unwrap_or_else(|| format!("DANGLING:{i}"))
        })
    };

    {
        let mut stmt = conn
            .prepare(
                "SELECT file_id, line, column, name, module_path, qualified_name, kind,
                        end_line, end_column, signature, visibility, parent_symbol_id, is_test
                 FROM symbols",
            )
            .expect("prep symbols");
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<i64>>(7)?,
                    r.get::<_, Option<i64>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                    r.get::<_, String>(10)?,
                    r.get::<_, Option<i64>>(11)?,
                    r.get::<_, i64>(12)?,
                ))
            })
            .expect("symbols");
        for row in rows {
            let (fid, line, col, name, mp, qn, kind, el, ec, sig, vis, parent, test) =
                row.expect("sym row");
            let fpath = files.get(&fid).cloned().unwrap_or_else(|| "?".into());
            let opt = |v: Option<i64>| v.map_or(String::from("None"), |x| x.to_string());
            out.push(format!(
                "sym|{fpath}|{line}|{col}|{name}|{mp}|{qn}|{kind}|{}|{}|{}|{vis}|{}|{test}",
                opt(el),
                opt(ec),
                sig.unwrap_or_default(),
                symkey(parent, &syms),
            ));
        }
    }

    {
        let mut stmt = conn
            .prepare(
                "SELECT file_id, line, column, kind, reference_name, symbol_id, in_symbol_id
                 FROM refs",
            )
            .expect("prep refs");
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                    r.get::<_, Option<i64>>(6)?,
                ))
            })
            .expect("refs");
        for row in rows {
            let (fid, line, col, kind, rname, sid, in_sid) = row.expect("ref row");
            let fpath = files.get(&fid).cloned().unwrap_or_else(|| "?".into());
            out.push(format!(
                "ref|{fpath}|{line}|{col}|{kind}|{}|{}|{}",
                rname.unwrap_or_default(),
                symkey(sid, &syms),
                symkey(in_sid, &syms),
            ));
        }
    }

    {
        let mut stmt = conn
            .prepare("SELECT file_id, symbol_name, source_module, alias FROM imports")
            .expect("prep imports");
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .expect("imports");
        for row in rows {
            let (fid, sname, smod, alias) = row.expect("imp row");
            let fpath = files.get(&fid).cloned().unwrap_or_else(|| "?".into());
            out.push(format!(
                "imp|{fpath}|{sname}|{smod}|{}",
                alias.unwrap_or_default()
            ));
        }
    }

    {
        let mut stmt = conn
            .prepare("SELECT from_file_id, to_file_id, ref_count FROM file_deps")
            .expect("prep deps");
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .expect("deps");
        for row in rows {
            let (f, t, n) = row.expect("dep row");
            out.push(format!(
                "dep|{}|{}|{n}",
                files.get(&f).cloned().unwrap_or_else(|| "?".into()),
                files.get(&t).cloned().unwrap_or_else(|| "?".into()),
            ));
        }
    }

    {
        let mut stmt = conn
            .prepare("SELECT caller_symbol_id, callee_symbol_id, call_count FROM call_edges")
            .expect("prep edges");
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .expect("edges");
        for row in rows {
            let (c, e, n) = row.expect("edge row");
            out.push(format!(
                "edge|{}|{}|{n}",
                symkey(Some(c), &syms),
                symkey(Some(e), &syms),
            ));
        }
    }

    {
        let mut stmt = conn
            .prepare("SELECT symbol_id, name, args, line FROM attributes")
            .expect("prep attrs");
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .expect("attrs");
        for row in rows {
            let (sid, name, args, line) = row.expect("attr row");
            out.push(format!(
                "attr|{}|{name}|{}|{line}",
                symkey(Some(sid), &syms),
                args.unwrap_or_default(),
            ));
        }
    }

    {
        let mut pkgs: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
        let mut stmt = conn
            .prepare("SELECT id, name, path, source FROM arch_packages")
            .expect("prep pkgs");
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .expect("pkgs");
        for row in rows {
            let (id, name, path, source) = row.expect("pkg row");
            out.push(format!("pkg|{name}|{path}|{source}"));
            pkgs.insert(id, name);
        }
        let mut stmt = conn
            .prepare("SELECT file_id, package_id FROM arch_file_packages")
            .expect("prep fpkg");
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            .expect("fpkg");
        for row in rows {
            let (fid, pid) = row.expect("fpkg row");
            out.push(format!(
                "fpkg|{}|{}",
                files.get(&fid).cloned().unwrap_or_else(|| "?".into()),
                pkgs.get(&pid).cloned().unwrap_or_else(|| "?".into()),
            ));
        }
        let mut stmt = conn
            .prepare("SELECT source_pkg, target_pkg, dep_count FROM arch_package_deps")
            .expect("prep pdep");
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .expect("pdep");
        for row in rows {
            let (s, t, n) = row.expect("pdep row");
            out.push(format!(
                "pdep|{}|{}|{n}",
                pkgs.get(&s).cloned().unwrap_or_else(|| "?".into()),
                pkgs.get(&t).cloned().unwrap_or_else(|| "?".into()),
            ));
        }
    }

    out.sort();
    out
}

/// The exact expected canonical content for the BATCH arm. Authored from a
/// hand-predicted skeleton (resolution targets, edges, dep counts), then
/// reconciled against the actual index and cross-checked against
/// `.idxperf/probe-dump.py` on the same fixture. Notable content this
/// fences: the cross-file call resolves via the qualified-module fallback;
/// the C# `Helper.Assist` call resolves via qualified-name lookup; the
/// top-level `ExternalThing` ref stays unresolved with NULL `in_symbol_id`
/// (the d4d87f1 accumulating shape); `pub use` produces an import row but
/// no reference; file deps come from call edges (the resolver declines the
/// bare single-segment `util` import path, so no L2 dep row exists to merge).
const EXPECTED_BATCH: &[&str] = &[
    "dep|cs/Use.cs|cs/Lib.cs|1",
    "dep|src/lib.rs|src/util.rs|1",
    "edge|cs/Use.cs:7:9:Go|cs/Lib.cs:5:9:Assist|1",
    "edge|src/lib.rs:4:1:caller|src/util.rs:1:1:shared_fn|1",
    "file|cs/Lib.cs|csharp|104",
    "file|cs/Use.cs|csharp|142",
    "file|src/lib.rs|rust|114",
    "file|src/util.rs|rust|41",
    "fpkg|cs/Lib.cs|app",
    "fpkg|cs/Use.cs|app",
    "fpkg|src/lib.rs|app",
    "fpkg|src/util.rs|app",
    "imp|cs/Use.cs|*|App.Cs|",
    "imp|src/lib.rs|shared_fn|util|",
    "pkg|app||manifest",
    "ref|cs/Use.cs|9|13|call||cs/Lib.cs:5:9:Assist|cs/Use.cs:7:9:Go",
    "ref|src/lib.rs|3|18|type|ExternalThing||",
    "ref|src/lib.rs|5|5|call||src/util.rs:1:1:shared_fn|src/lib.rs:4:1:caller",
    "sym|cs/Lib.cs|1|1|App.Cs||App.Cs|module|7|2||public||0",
    "sym|cs/Lib.cs|3|5|Helper||Helper|class|6|6||public||0",
    "sym|cs/Lib.cs|5|9|Assist||Helper::Assist|function|5|40|void Assist()|public||0",
    "sym|cs/Use.cs|3|1|App.Use||App.Use|module|12|2||public||0",
    "sym|cs/Use.cs|5|5|U||U|class|11|6||public||0",
    "sym|cs/Use.cs|7|9|Go||U::Go|method|10|10|void Go()|public||0",
    "sym|src/lib.rs|1|1|util|crate|util|module|1|14||public||0",
    "sym|src/lib.rs|3|1|Alias|crate|Alias|type_alias|3|32||public||0",
    "sym|src/lib.rs|4|1|caller|crate|caller|function|6|2|fn caller()|public||0",
    "sym|src/util.rs|1|1|shared_fn|crate::util|shared_fn|function|1|22|fn shared_fn()|public||0",
    "sym|src/util.rs|2|1|caller|crate::util|caller|function|2|19|fn caller()|public||0",
];

/// The documented batch→streaming divergence: streaming mode does not
/// populate `module_path` (parallel parse threads lack the crate list, and
/// streaming has no post-pass for it). Everything else — refs, resolution
/// targets, edges, deps, imports — is identical. If streaming `module_path`
/// support ever lands, this transform (and this fence) must be updated
/// consciously.
fn expected_streaming() -> Vec<String> {
    let mut rows: Vec<String> = EXPECTED_BATCH
        .iter()
        .map(|row| {
            if let Some(rest) = row.strip_prefix("sym|") {
                let mut fields: Vec<&str> = rest.split('|').collect();
                // sym|path|line|col|name|module_path|qualified|kind|...
                fields[4] = "";
                format!("sym|{}", fields.join("|"))
            } else {
                (*row).to_string()
            }
        })
        .collect();
    rows.sort();
    rows
}

fn index_batch(root: &Path) -> Vec<String> {
    let mut tethys = Tethys::new(root).expect("Tethys::new");
    tethys.rebuild().expect("rebuild (batch)");
    canonical_rows(&root.join(".rivets/index/tethys.db"))
}

fn index_streaming(root: &Path, options: IndexOptions) -> Vec<String> {
    let mut tethys = Tethys::new(root).expect("Tethys::new");
    tethys
        .rebuild_with_options(options)
        .expect("rebuild (streaming)");
    canonical_rows(&root.join(".rivets/index/tethys.db"))
}

/// Claims C1/C3: batch-mode canonical content is exactly the expected set,
/// Rust and C# arms together.
#[test]
fn batch_content_matches_golden_rows() {
    let dir = TempDir::new().expect("tempdir");
    write_fixture(dir.path());
    let rows = index_batch(dir.path());
    assert_eq!(
        rows,
        EXPECTED_BATCH
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        "batch canonical content drifted from the golden set"
    );
}

/// Claims C2/C7: streaming-mode canonical content equals batch content
/// modulo the documented `module_path` divergence — at the default batch
/// size AND at `batch_size` 1 (boundary shape: every file its own batch).
#[test]
fn streaming_content_matches_golden_rows_at_both_batch_sizes() {
    let dir = TempDir::new().expect("tempdir");
    write_fixture(dir.path());

    let expected = expected_streaming();
    let at_default = index_streaming(dir.path(), IndexOptions::with_streaming());
    assert_eq!(
        at_default, expected,
        "streaming canonical content drifted from the golden set"
    );

    let at_one = index_streaming(dir.path(), IndexOptions::with_streaming_batch_size(1));
    assert_eq!(
        at_one, at_default,
        "batch_size=1 must produce identical content to the default batch size"
    );
}

/// Claim C9: re-indexing WITHOUT a rebuild over an unchanged tree yields
/// content identical to a fresh rebuild, across multiple runs. The fixture
/// contains the exact accumulating shape from the d4d87f1 bug (a top-level
/// unresolved type ref with NULL `in_symbol_id`, which the symbols-delete
/// cascade never reaches) — re-breaking the refs DELETE in
/// `index_parsed_file_atomic` makes this fail with duplicated ref rows.
#[test]
fn reindex_without_rebuild_equals_fresh_rebuild() {
    let dir = TempDir::new().expect("tempdir");
    write_fixture(dir.path());
    let db = dir.path().join(".rivets/index/tethys.db");

    let mut tethys = Tethys::new(dir.path()).expect("Tethys::new");
    tethys.rebuild().expect("fresh rebuild");
    let fresh = canonical_rows(&db);

    tethys.index().expect("second index (no rebuild)");
    let second = canonical_rows(&db);
    assert_eq!(second, fresh, "first re-index must not change content");

    tethys.index().expect("third index (no rebuild)");
    let third = canonical_rows(&db);
    assert_eq!(third, fresh, "content must stay stable across N re-indexes");
}
