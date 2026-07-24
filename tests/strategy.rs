//! Integration fences for resolution provenance (tethys-9z7i slice 2,
//! ADR-0003): every write path stamps `refs.strategy`; NULL ⇔ unresolved.
//!
//! Expected labels were written in `.tethys-9z7i/plan-slice2.md` BEFORE
//! implementation; the independent oracles are raw SQL shapes (spatial
//! joins, direction-split NULL counts) and `RUST_LOG` trace events, per the
//! probe (`.tethys-9z7i/findings.md`).

mod common;

use common::{open_db, workspace_with_files};

/// Count refs matching a where-clause. Raw SQL — no analysis code.
fn count_refs(tethys: &tethys::Tethys, where_clause: &str) -> i64 {
    let conn = open_db(tethys);
    conn.query_row(
        &format!("SELECT COUNT(*) FROM refs WHERE {where_clause}"),
        [],
        |r| r.get(0),
    )
    .expect("count query")
}

/// B3 (design C2): every insert-time bind stamps `same_file`, including
/// the macro-map path (kills a stamp wired only into the general map);
/// the label count equals the same-file spatial JOIN count — the label
/// and the join are independent mechanisms (the probe's 1564==1564
/// reconciliation at fixture scale). A truly unresolved ref stays NULL.
#[test]
fn pass1_stamps_same_file_and_matches_spatial_join() {
    let (_dir, mut tethys) = workspace_with_files(&[(
        "src/lib.rs",
        "macro_rules! shout {\n    () => {};\n}\n\
         pub fn local() {}\n\
         pub fn caller() {\n    local();\n    shout!();\n    zz::nothing();\n}\n",
    )]);
    tethys.index().expect("index failed");

    let same_file_labeled = count_refs(&tethys, "strategy = 'same_file'");
    assert!(
        same_file_labeled >= 2,
        "the fn call AND the macro invocation both stamp same_file; got {same_file_labeled}"
    );

    let conn = open_db(&tethys);
    let spatial: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM refs r
             JOIN symbols s ON s.id = r.symbol_id
             WHERE r.file_id = s.file_id",
            [],
            |r| r.get(0),
        )
        .expect("spatial join");
    assert_eq!(
        same_file_labeled, spatial,
        "same_file label count must equal the same-file spatial join count"
    );

    let unresolved_with_strategy =
        count_refs(&tethys, "symbol_id IS NULL AND strategy IS NOT NULL");
    assert_eq!(
        unresolved_with_strategy, 0,
        "unresolved rows never carry a strategy"
    );
}

/// B4 (design C1): after a full index — with a CIRCULAR cross-file import
/// pair forcing multi-round Pass 2 — `strategy IS NULL ⇔ symbol_id IS
/// NULL`, asserted as two direction-split counts so a failure names its
/// direction. Kills: any arm (or the LSP path) forgetting the strategy
/// bind; anything stamping unresolved rows.
#[test]
fn strategy_null_iff_unresolved() {
    let (_dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "pub mod a;\npub mod b;\n"),
        (
            "src/a.rs",
            "use crate::b::bee;\npub fn aye() {\n    bee();\n}\n",
        ),
        (
            "src/b.rs",
            "use crate::a::aye;\npub fn bee() {\n    aye();\n}\npub fn lone() {\n    zz::nope();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");

    assert_eq!(
        count_refs(&tethys, "symbol_id IS NOT NULL AND strategy IS NULL"),
        0,
        "every resolved ref carries a strategy"
    );
    assert_eq!(
        count_refs(&tethys, "symbol_id IS NULL AND strategy IS NOT NULL"),
        0,
        "no unresolved ref carries a strategy"
    );
    assert!(
        count_refs(&tethys, "symbol_id IS NULL") >= 1,
        "the zz::nope() ref stays genuinely unresolved"
    );
    assert!(
        count_refs(&tethys, "symbol_id IS NOT NULL") >= 2,
        "the circular imports actually resolve — direction 1 must not \
         pass vacuously on an all-unresolved fixture"
    );
}

/// B4 (design C4): the per-file memo fans ONE resolution out to every
/// duplicate ref — all three calls to the imported fn carry the same
/// `explicit_import` label. Kills a memo that caches the symbol without the
/// strategy (duplicates would land NULL or defaulted).
#[test]
fn memo_fans_strategy_to_duplicates() {
    let (_dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "pub mod m;\npub mod u;\n"),
        ("src/m.rs", "pub fn worker() {}\n"),
        (
            "src/u.rs",
            "use crate::m::worker;\npub fn go() {\n    worker();\n    worker();\n    worker();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");

    let labeled = count_refs(
        &tethys,
        "strategy = 'explicit_import' AND symbol_id IN
         (SELECT id FROM symbols WHERE name = 'worker')",
    );
    assert_eq!(
        labeled, 3,
        "all three duplicate calls inherit the memoized strategy"
    );
}

/// B4 (design C9): reexport-kind refs (v1w8) are ordinary refs to the
/// provenance machinery — when resolved they carry a real arm label.
/// Kills kind-filtered stamping.
#[test]
fn reexport_refs_carry_strategy() {
    let (_dir, mut tethys) = workspace_with_files(&[
        ("src/lib.rs", "mod inner;\npub use inner::item;\n"),
        ("src/inner.rs", "pub fn item() {}\n"),
    ]);
    tethys.index().expect("index failed");

    let resolved_reexports = count_refs(&tethys, "kind = 'reexport' AND symbol_id IS NOT NULL");
    assert_eq!(resolved_reexports, 1, "the pub use ref resolves");
    let labeled = count_refs(
        &tethys,
        "kind = 'reexport' AND symbol_id IS NOT NULL AND strategy IS NOT NULL",
    );
    assert_eq!(labeled, 1, "and carries a strategy label");
}

/// B4 drift fence (caught by the oracle step, not any fixture): the
/// outdated-schema guard must not brick its own remedy. On a pre-column
/// DB, a query command fails WITH the guidance, and `index --rebuild`
/// clears the files and succeeds — the CLI deletes the db before opening.
#[test]
fn rebuild_recovers_from_outdated_schema() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"t\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .expect("manifest");
    std::fs::create_dir_all(dir.path().join("src")).expect("src dir");
    std::fs::write(dir.path().join("src/lib.rs"), "pub fn f() {}\n").expect("lib");

    // Hand-build a pre-provenance db (the tethys-xvlw shape).
    let db_dir = dir.path().join(".rivets").join("index");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    {
        let conn = rusqlite::Connection::open(db_dir.join("tethys.db")).expect("old db");
        conn.execute_batch(
            "CREATE TABLE refs (
                 id INTEGER PRIMARY KEY, symbol_id INTEGER,
                 file_id INTEGER NOT NULL, kind TEXT NOT NULL,
                 line INTEGER NOT NULL, column INTEGER NOT NULL,
                 end_line INTEGER, end_column INTEGER,
                 in_symbol_id INTEGER, reference_name TEXT);",
        )
        .expect("old refs table");
    }

    let bin = env!("CARGO_BIN_EXE_tethys");
    // Query command: clear failure with the remedy in the message.
    let out = std::process::Command::new(bin)
        .args(["stats", "-w"])
        .arg(dir.path())
        .output()
        .expect("run stats");
    assert!(!out.status.success(), "query on outdated schema must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--rebuild"),
        "error must name the remedy; stderr: {stderr}"
    );

    // The remedy itself must work.
    let out = std::process::Command::new(bin)
        .args(["index", "--rebuild", "-w"])
        .arg(dir.path())
        .output()
        .expect("run rebuild");
    assert!(
        out.status.success(),
        "--rebuild must recover from an outdated schema; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Per-name strategy lookup for a resolved ref bound to symbol `name`.
fn strategy_of(tethys: &tethys::Tethys, symbol_name: &str) -> Vec<String> {
    let conn = open_db(tethys);
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT COALESCE(r.strategy, '(null)') FROM refs r
             JOIN symbols s ON s.id = r.symbol_id
             WHERE s.name = ?1 ORDER BY 1",
        )
        .expect("prepare");
    let rows = stmt
        .query_map([symbol_name], |r| r.get::<_, String>(0))
        .expect("query");
    rows.collect::<Result<_, _>>().expect("collect")
}

/// B5 (design C3): every Pass-2 arm stamps its own label — one mixed
/// multi-crate + C# workspace fires all seven. Each shape is chosen so
/// exactly one arm can claim it (arm order documented per case). Kills:
/// swapped labels; the fallback's three sub-paths collapsing into one.
#[test]
fn every_arm_stamps_its_label() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/main-crate\", \"crates/aux-crate\"]\n",
        ),
        (
            "crates/main-crate/Cargo.toml",
            "[package]\nname = \"main-crate\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        (
            "crates/main-crate/src/lib.rs",
            "pub mod m;\npub mod g;\npub mod helper;\npub mod x;\npub mod user;\npub fn sfn() {}\n",
        ),
        ("crates/main-crate/src/m.rs", "pub fn efn() {}\n"),
        ("crates/main-crate/src/g.rs", "pub fn gfn() {}\n"),
        ("crates/main-crate/src/helper.rs", "pub fn do_thing() {}\n"),
        (
            "crates/main-crate/src/x.rs",
            "pub struct Holder;\nimpl Holder {\n    pub fn alpha() {}\n}\n",
        ),
        (
            // One consumer file, no import for the fallback shapes:
            // - efn: explicit import        -> explicit_import
            // - gfn: glob import            -> glob_import
            // - Holder::alpha qualified text matches stored qualified_name
            //   (module-stripped)           -> qualified_exact
            // - sfn: bare, same crate, no import -> same_crate (scoped
            //   search runs BEFORE the unscoped one)
            // - helper::do_thing: relative module-qualified, stored
            //   qualified_name is just 'do_thing' so the exact match
            //   misses                      -> qualified_module_fallback
            "crates/main-crate/src/user.rs",
            "use crate::m::efn;\nuse crate::g::*;\n\npub fn go() {\n    efn();\n    gfn();\n    \
             Holder::alpha();\n    sfn();\n    helper::do_thing();\n    afn();\n}\n",
        ),
        (
            // afn lives in ANOTHER crate, bare + unimported + unique:
            // the crate-scoped search misses -> unique_workspace.
            "crates/aux-crate/Cargo.toml",
            "[package]\nname = \"aux-crate\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        ("crates/aux-crate/src/lib.rs", "pub fn afn() {}\n"),
        (
            // C# union arm: static-member using + bare call is only
            // claimable by GlobPolicy::UniqueAcrossAll -> import_union.
            "Lib.cs",
            "using System;\n\nnamespace Lib\n{\n    public static class Legacy\n    {\n        \
             public static void UnionTarget() { }\n    }\n}\n",
        ),
        (
            "Use.cs",
            "using static Lib.Legacy;\n\nnamespace App\n{\n    public class User\n    {\n        \
             public void Go()\n        {\n            UnionTarget();\n        }\n    }\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");

    let cases = [
        ("efn", "explicit_import"),
        ("gfn", "glob_import"),
        ("alpha", "qualified_exact"),
        ("sfn", "same_crate"),
        ("do_thing", "qualified_module_fallback"),
        ("afn", "unique_workspace"),
        ("UnionTarget", "import_union"),
    ];
    for (name, expected) in cases {
        assert_eq!(
            strategy_of(&tethys, name),
            [expected],
            "arm label for {name}"
        );
    }
}

/// B5 (design C5): macro refs bypass the memo without cross-contamination.
/// `solo()` (fn, same file) binds Pass 1 `same_file`; `solo!()` shares the
/// reference name but must NOT inherit the fn's binding or strategy — with
/// a same-named macro in another file the kind-gated lookup is ambiguous,
/// so the macro ref stays unresolved with strategy NULL. The clean case
/// (`lone_macro!` unique cross-file) resolves through the bypass with a
/// real arm label.
#[test]
fn macro_bypass_stamps_without_contamination() {
    let (_dir, mut tethys) = workspace_with_files(&[
        (
            "src/lib.rs",
            "pub mod a;\npub mod b;\n\
             #[macro_export]\nmacro_rules! solo {\n    () => {};\n}\n\
             #[macro_export]\nmacro_rules! lone_macro {\n    () => {};\n}\n",
        ),
        (
            "src/b.rs",
            "pub fn solo() {}\npub fn go() {\n    solo();\n    solo!();\n    lone_macro!();\n}\n",
        ),
        ("src/a.rs", "pub fn unrelated() {}\n"),
    ]);
    tethys.index().expect("index failed");

    let conn = open_db(&tethys);
    let (fn_strategy, macro_solo_strategy, lone_strategy): (String, String, String) = conn
        .query_row(
            "SELECT
               (SELECT COALESCE(strategy,'(null)') FROM refs WHERE kind='call'
                  AND symbol_id IN (SELECT id FROM symbols WHERE name='solo')),
               (SELECT COALESCE(strategy,'(null)') FROM refs WHERE kind='macro'
                  AND reference_name = 'solo'),
               (SELECT COALESCE(strategy,'(null)') FROM refs WHERE kind='macro'
                  AND symbol_id IN (SELECT id FROM symbols WHERE name='lone_macro'))",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("three-way lookup");
    assert_eq!(fn_strategy, "same_file", "the fn call binds at insert");
    assert_eq!(
        macro_solo_strategy, "(null)",
        "ambiguous macro ref must not inherit the fn's memo/strategy"
    );
    assert_ne!(lone_strategy, "(null)", "unique macro resolves via Pass 2");
}

/// Slice-3 P2 (design C3/C4/C5): --exclude-speculative semantics on the
/// callers surface. Chain: `a_fn` -[explicit import]-> `b_fn` -[bare
/// cross-crate call, `unique_workspace`]-> `leaf`; `d_fn` has MIXED
/// support to `ml` (bare imported call = `explicit_import`, plus
/// relative-qualified `inner::ml()` = `qualified_module_fallback`).
/// Expected: excluding drops ONLY the all-speculative edge
/// (`b_fn`->`leaf`) — `d_fn`'s mixed edge survives — and the drop is
/// transitive (`a_fn` must not surface through the severed edge).
fn exclusion_fixture() -> (tempfile::TempDir, tethys::Tethys) {
    let (dir, mut tethys) = workspace_with_files(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/base-c\", \"crates/mid-b\", \"crates/top-a\"]\n",
        ),
        (
            "crates/base-c/Cargo.toml",
            "[package]\nname = \"base-c\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        ("crates/base-c/src/lib.rs", "pub fn leaf() {}\n"),
        (
            "crates/mid-b/Cargo.toml",
            "[package]\nname = \"mid-b\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        (
            "crates/mid-b/src/lib.rs",
            "pub mod inner;\nuse crate::inner::ml;\n\
             pub fn b_fn() {\n    leaf();\n}\n\
             pub fn d_fn() {\n    ml();\n    inner::ml();\n}\n",
        ),
        ("crates/mid-b/src/inner.rs", "pub fn ml() {}\n"),
        (
            "crates/top-a/Cargo.toml",
            "[package]\nname = \"top-a\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        ),
        (
            "crates/top-a/src/lib.rs",
            "use mid_b::b_fn;\npub fn a_fn() {\n    b_fn();\n}\n",
        ),
    ]);
    tethys.index().expect("index failed");
    (dir, tethys)
}

#[test]
fn exclude_speculative_drops_only_unsupported() {
    let (_dir, tethys) = exclusion_fixture();

    let callers_of = |name: &str, exclude: bool| -> Vec<String> {
        let call_edges = if exclude {
            tethys::CallEdgeSelection::ExcludeSpeculative
        } else {
            tethys::CallEdgeSelection::All
        };
        let mut v: Vec<String> = tethys
            .get_callers(name, tethys::CallerMode::Indexed { call_edges })
            .expect("callers")
            .into_iter()
            .map(|caller| caller.symbol.qualified_name)
            .collect();
        v.sort_unstable();
        v
    };
    assert_eq!(
        callers_of("leaf", false),
        ["b_fn"],
        "speculative edge visible by default"
    );
    assert!(
        callers_of("leaf", true).is_empty(),
        "the only support is unique_workspace — edge dropped"
    );
    assert_eq!(
        callers_of("ml", true),
        ["d_fn"],
        "an edge with ANY trustworthy support survives exclusion"
    );
}

#[test]
fn exclusion_is_transitive() {
    let (_dir, tethys) = exclusion_fixture();

    let full = tethys
        .get_symbol_impact("leaf", None, false)
        .expect("impact");
    let mut all: Vec<String> = full
        .direct_dependents
        .iter()
        .chain(full.transitive_dependents.iter())
        .flat_map(|d| d.symbols_used.clone())
        .collect();
    all.sort_unstable();
    assert_eq!(all, ["a_fn", "b_fn"], "unfiltered chain reaches a_fn");

    let filtered = tethys
        .get_symbol_impact("leaf", None, true)
        .expect("impact excl");
    assert!(
        filtered.direct_dependents.is_empty() && filtered.transitive_dependents.is_empty(),
        "severing the speculative edge must also remove everything beyond \
         it; got direct {:?} transitive {:?}",
        filtered.direct_dependents,
        filtered.transitive_dependents
    );
}

/// Slice-3 P3 (design C6): the CLI flag reaches the analysis — same
/// fixture through the real binary, flag off vs on.
#[test]
fn cli_callers_exclude_speculative() {
    let (dir, _tethys) = exclusion_fixture();
    let run = |extra: &[&str]| -> String {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
            .args(["callers", "leaf", "-w"])
            .arg(dir.path())
            .args(extra)
            .output()
            .expect("run callers");
        assert!(out.status.success(), "exit ok");
        String::from_utf8(out.stdout).expect("utf8")
    };
    assert!(
        run(&[]).contains("b_fn"),
        "default output shows the speculative caller"
    );
    assert!(
        run(&["--exclude-speculative"]).contains("No callers found"),
        "flag drops the all-speculative edge"
    );
    assert!(
        run(&["--transitive", "--exclude-speculative"]).contains("No callers found"),
        "flag composes with --transitive through get_symbol_impact"
    );
}

#[test]
fn cli_callers_rejects_unsupported_lsp_combinations() {
    for conflicting_flag in ["--transitive", "--exclude-speculative"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
            .args(["callers", "leaf", "--lsp", conflicting_flag])
            .output()
            .expect("run callers");

        assert!(
            !output.status.success(),
            "--lsp with {conflicting_flag} must fail"
        );
        let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
        assert!(
            stderr.contains("cannot be used with") && stderr.contains(conflicting_flag),
            "explicit conflict error for {conflicting_flag}, got: {stderr}"
        );
    }
}
