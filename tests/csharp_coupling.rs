//! Hand-authored persisted evidence: declarations are not selected-unit edges.

use rusqlite::{Connection, params};
use tethys::{CouplingIndeterminacy, CouplingSort, MetricEvidence, Tethys};

const APP: &str = "msbuild:App/App.csproj:app-net8";
const CORE8: &str = "msbuild:Core/Core.csproj:core-net8";
const CORE9: &str = "msbuild:Core/Core.csproj:core-net9";
const ISOLATED: &str = "msbuild:Isolated/Isolated.csproj:isolated-net8";
const CONFIRMED: &str = r#"{"standing":"confirmed"}"#;
const FAILED: &str =
    r#"{"standing":"indeterminate","failure":{"reason":"evaluation-failed","diagnostics":[]}}"#;

fn fixture() -> (tempfile::TempDir, Tethys, Connection) {
    let root = tempfile::tempdir().unwrap();
    let tethys = Tethys::new(root.path()).unwrap();
    let db = Connection::open(root.path().join(".rivets/index/tethys.db")).unwrap();
    db.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    for (ordinal, project) in [
        "App/App.csproj",
        "Core/Core.csproj",
        "Isolated/Isolated.csproj",
    ]
    .into_iter()
    .enumerate()
    {
        db.execute(
            "INSERT INTO projects VALUES (?1, ?2, '[]', ?3)",
            params![project, ordinal, CONFIRMED],
        )
        .unwrap();
    }
    for (ordinal, (key, project, tfm, name)) in [
        ("app-net8", "App/App.csproj", "net8.0", APP),
        ("core-net8", "Core/Core.csproj", "net8.0", CORE8),
        ("core-net9", "Core/Core.csproj", "net9.0", CORE9),
        (
            "isolated-net8",
            "Isolated/Isolated.csproj",
            "net8.0",
            ISOLATED,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        db.execute(
            "INSERT INTO evaluation_units VALUES (?1, ?2, ?3, ?4, 'null', ?5, '{\"AssemblyName\":\"Collision\"}', 'null', '{\"style\":\"none\",\"inputs\":[]}')",
            params![key, project, ordinal, tfm, CONFIRMED],
        ).unwrap();
        db.execute(
            "INSERT INTO arch_packages (name, path, source, evaluation_unit_key) VALUES (?1, ?2, 'msbuild', ?3)",
            params![name, std::path::Path::new(project).parent().unwrap().to_str().unwrap(), key],
        ).unwrap();
    }
    db.execute_batch(
        "INSERT INTO declared_project_references VALUES ('app-net8', 0, 'Core/Core.csproj', '../Core/Core.csproj', '{}');",
    ).unwrap();
    (root, tethys, db)
}

#[test]
fn declarations_do_not_select_either_framework_or_an_assembly_name_collision() {
    let (_root, tethys, db) = fixture();
    db.execute_batch(
        "INSERT INTO declared_project_references VALUES ('app-net8', 1, 'Core/Core.csproj', '../Core/./Core.csproj', '{}');",
    ).unwrap();
    let unattributed =
        MetricEvidence::Indeterminate(CouplingIndeterminacy::UnattributedEvaluationUnit);
    let unselected =
        MetricEvidence::Indeterminate(CouplingIndeterminacy::UnselectedProjectReference);
    let rows = tethys.get_coupling_metrics(CouplingSort::Name).unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row.package.name.as_str())
            .collect::<Vec<_>>(),
        [APP, CORE8, CORE9, ISOLATED]
    );
    // No evaluation unit carries file attribution, so every zero here is the
    // shape of the graph, never a measurement; the reason is the observable.
    // The declaration targets Core, a selected project, so it reports neither an
    // unselected-reference reason nor a count.
    for (name, ca, ce) in [
        (APP, unattributed, unattributed),
        // Core is a declared target: its incoming count may be understated.
        (CORE8, unselected, unattributed),
        (CORE9, unselected, unattributed),
        (ISOLATED, unattributed, unattributed),
    ] {
        let detail = tethys.get_package_coupling(name).unwrap().unwrap();
        assert_eq!(detail.metrics.afferent, ca, "{name} incoming");
        assert_eq!(detail.metrics.efferent, ce, "{name} outgoing");
        // Instability is unavailable with either axis; the incoming reason leads.
        assert_eq!(
            detail.metrics.instability(),
            MetricEvidence::Indeterminate(match ca {
                MetricEvidence::Indeterminate(reason) => reason,
                MetricEvidence::Known(_) => panic!("{name}: fixture expects unavailable evidence"),
            }),
            "{name} instability"
        );
        assert!(
            detail.incoming.is_empty(),
            "declarations cannot manufacture incoming neighbors"
        );
        assert!(
            detail.outgoing.is_empty(),
            "declarations cannot manufacture outgoing neighbors"
        );
    }
    assert!(tethys.get_package_coupling("Collision").unwrap().is_none());
    assert!(
        tethys
            .get_package_coupling("Core/Core.csproj")
            .unwrap()
            .is_none()
    );
    let app = tethys.get_package_coupling(APP).unwrap().unwrap();
    let declarations = &app
        .metrics
        .evaluation_unit
        .as_ref()
        .unwrap()
        .declared_references;
    assert_eq!(
        declarations
            .iter()
            .map(|reference| (reference.target.as_str(), reference.include.as_str()))
            .collect::<Vec<_>>(),
        [
            ("Core/Core.csproj", "../Core/Core.csproj"),
            ("Core/Core.csproj", "../Core/./Core.csproj"),
        ]
    );
}

#[test]
fn noncontributing_reference_never_reports_an_unselected_edge() {
    let (_root, tethys, db) = fixture();
    // A reference to a project with no unit is genuinely unselected, so this
    // fixture would report an unselected edge if the reference contributed.
    db.execute_batch(
        "INSERT INTO declared_project_references VALUES ('app-net8', 1, 'Missing/Missing.csproj', '../Missing/Missing.csproj', '{\"ReferenceOutputAssembly\":\"false\"}');",
    )
    .unwrap();
    let detail = tethys.get_package_coupling(APP).unwrap().unwrap();
    assert_eq!(
        detail.metrics.efferent,
        MetricEvidence::Indeterminate(CouplingIndeterminacy::UnattributedEvaluationUnit),
        "a reference that produces no output must not report an unselected edge"
    );
    let declarations = &detail
        .metrics
        .evaluation_unit
        .as_ref()
        .unwrap()
        .declared_references;
    assert_eq!(declarations.len(), 2);
    assert_eq!(declarations[1].metadata["ReferenceOutputAssembly"], "false");

    // Control: the same target, contributing, does report the reason.
    db.execute_batch(
        "UPDATE declared_project_references SET metadata_json = '{}' WHERE ordinal = 1;",
    )
    .unwrap();
    let detail = tethys.get_package_coupling(APP).unwrap().unwrap();
    assert_eq!(
        detail.metrics.efferent,
        MetricEvidence::Indeterminate(CouplingIndeterminacy::UnselectedProjectReference),
        "a contributing reference to an unselected project withholds the outgoing count"
    );
}

#[test]
fn incomplete_project_issue_or_failed_unit_withholds_both_csharp_axes() {
    for evidence in ["project", "issue", "unit"] {
        let (_root, tethys, db) = fixture();
        db.execute_batch("INSERT INTO arch_packages (name,path,source) VALUES ('rust-isolated','rust','manifest');").unwrap();
        match evidence {
            "project" => {
                db.execute(
                    "INSERT INTO projects VALUES ('Missing.csproj', 3, '[]', ?1)",
                    [FAILED],
                )
                .unwrap();
            }
            "issue" => {
                db.execute_batch("INSERT INTO discovery_issues VALUES (0, 'Broken.sln', '{\"reason\":\"malformed-input\",\"diagnostics\":[]}');").unwrap();
            }
            "unit" => {
                db.execute(
                    "UPDATE evaluation_units SET standing_json = ?1 WHERE unit_key = 'core-net9'",
                    [FAILED],
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let incomplete: MetricEvidence<u32> =
            MetricEvidence::Indeterminate(CouplingIndeterminacy::IncompleteDiscovery);
        let incomplete_ratio: MetricEvidence<f64> =
            MetricEvidence::Indeterminate(CouplingIndeterminacy::IncompleteDiscovery);
        // An incomplete inventory can hide a project or an input, so neither
        // axis is published: one withheld axis beside a measured one would read
        // as a measurement.
        let isolated = tethys.get_package_coupling(ISOLATED).unwrap().unwrap();
        assert_eq!(isolated.metrics.afferent, incomplete, "{evidence}");
        assert_eq!(isolated.metrics.efferent, incomplete, "{evidence}");
        let app = tethys.get_package_coupling(APP).unwrap().unwrap();
        assert_eq!(app.metrics.afferent, incomplete);
        assert_eq!(app.metrics.efferent, incomplete);
        assert_eq!(app.metrics.instability(), incomplete_ratio);
        if evidence == "unit" {
            let failed = tethys.get_package_coupling(CORE9).unwrap().unwrap();
            assert_eq!(failed.metrics.afferent, incomplete);
            assert_eq!(failed.metrics.efferent, incomplete);
            assert!(failed.incoming.is_empty());
        }
        let rust = tethys
            .get_package_coupling("rust-isolated")
            .unwrap()
            .unwrap();
        assert_eq!(rust.metrics.afferent, MetricEvidence::Known(0));
        assert_eq!(rust.metrics.efferent, MetricEvidence::Known(0));
    }
}

/// One measured edge, so unknown-ranked rows are distinguishable from measured ones.
fn insert_measured_edge(db: &Connection) {
    db.execute_batch(
        "INSERT INTO arch_packages (name, path, source) VALUES ('rust-hub', 'hub', 'manifest');
         INSERT INTO arch_packages (name, path, source) VALUES ('rust-leaf', 'leaf', 'manifest');
         INSERT INTO arch_package_deps (source_pkg, target_pkg, dep_count)
             SELECT hub.id, leaf.id, 2 FROM arch_packages hub, arch_packages leaf
             WHERE hub.name = 'rust-hub' AND leaf.name = 'rust-leaf';",
    )
    .unwrap();
}

#[test]
fn a_declared_reference_does_not_replace_a_real_edge() {
    let (_root, tethys, db) = fixture();
    insert_measured_edge(&db);
    // Positive control: the neighbor query demonstrably returns rows, so the
    // emptiness assertions below are falsifiable rather than merely unfalsified.
    let leaf = tethys.get_package_coupling("rust-leaf").unwrap().unwrap();
    assert_eq!(leaf.metrics.afferent, MetricEvidence::Known(1));
    assert_eq!(
        leaf.incoming
            .iter()
            .map(|edge| (edge.package.name.as_str(), edge.dep_count))
            .collect::<Vec<_>>(),
        [("rust-hub", 2)]
    );
    let app = tethys.get_package_coupling(APP).unwrap().unwrap();
    assert!(
        app.outgoing.is_empty(),
        "a declaration is not an edge even when the edge query works"
    );
    assert!(app.incoming.is_empty());
}

#[test]
fn numeric_sorts_place_measured_counts_before_unknown_ones() {
    let (_root, tethys, db) = fixture();
    insert_measured_edge(&db);
    let names = |sort| {
        tethys
            .get_coupling_metrics(sort)
            .unwrap()
            .iter()
            .map(|row| row.package.name.clone())
            .collect::<Vec<_>>()
    };
    // rust-leaf is the only package with a measured incoming count; rust-hub the
    // only one with a measured outgoing count. Both outrank the unattributed
    // units, which tie and fall back to the name break.
    assert_eq!(
        names(CouplingSort::Afferent).first().map(String::as_str),
        Some("rust-leaf")
    );
    assert_eq!(
        names(CouplingSort::Efferent).first().map(String::as_str),
        Some("rust-hub")
    );
    let instability = names(CouplingSort::Instability);
    assert_eq!(instability.first().map(String::as_str), Some("rust-hub"));
    assert_eq!(instability.get(1).map(String::as_str), Some("rust-leaf"));
}
