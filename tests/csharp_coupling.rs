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
    let unknown = MetricEvidence::Indeterminate(CouplingIndeterminacy::UnselectedProjectReference);
    let rows = tethys.get_coupling_metrics(CouplingSort::Name).unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row.package.name.as_str())
            .collect::<Vec<_>>(),
        [APP, CORE8, CORE9, ISOLATED]
    );
    for (name, ca, ce) in [
        (APP, MetricEvidence::Known(0), unknown),
        (CORE8, unknown, MetricEvidence::Known(0)),
        (CORE9, unknown, MetricEvidence::Known(0)),
        (ISOLATED, MetricEvidence::Known(0), MetricEvidence::Known(0)),
    ] {
        let detail = tethys.get_package_coupling(name).unwrap().unwrap();
        assert_eq!(detail.metrics.afferent, ca, "{name} incoming");
        assert_eq!(detail.metrics.efferent, ce, "{name} outgoing");
        assert_eq!(
            detail.metrics.instability(),
            if name == ISOLATED {
                MetricEvidence::Known(0.0)
            } else {
                MetricEvidence::Indeterminate(CouplingIndeterminacy::UnselectedProjectReference)
            }
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
fn noncontributing_reference_is_retained_without_poisoning_either_count() {
    let (_root, tethys, db) = fixture();
    db.execute_batch("UPDATE declared_project_references SET metadata_json = '{\"ReferenceOutputAssembly\":\"false\"}';").unwrap();
    for row in tethys.get_coupling_metrics(CouplingSort::Name).unwrap() {
        assert_eq!(row.afferent, MetricEvidence::Known(0));
        assert_eq!(row.efferent, MetricEvidence::Known(0));
        assert_eq!(row.instability(), MetricEvidence::Known(0.0));
    }
    let detail = tethys.get_package_coupling(APP).unwrap().unwrap();
    assert!(detail.outgoing.is_empty());
    let declarations = &detail
        .metrics
        .evaluation_unit
        .as_ref()
        .unwrap()
        .declared_references;
    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0].metadata["ReferenceOutputAssembly"], "false");
}

#[test]
fn incomplete_project_issue_and_failed_unit_each_withhold_csharp_incoming_only() {
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
        let incomplete = MetricEvidence::Indeterminate(CouplingIndeterminacy::IncompleteDiscovery);
        let isolated = tethys.get_package_coupling(ISOLATED).unwrap().unwrap();
        assert_eq!(isolated.metrics.afferent, incomplete, "{evidence}");
        assert_eq!(isolated.metrics.efferent, MetricEvidence::Known(0));
        let app = tethys.get_package_coupling(APP).unwrap().unwrap();
        assert_eq!(app.metrics.afferent, incomplete);
        assert_eq!(
            app.metrics.efferent,
            MetricEvidence::Indeterminate(CouplingIndeterminacy::UnselectedProjectReference)
        );
        assert_eq!(
            app.metrics.instability(),
            MetricEvidence::Indeterminate(CouplingIndeterminacy::IncompleteDiscovery)
        );
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

#[test]
fn numeric_sorts_place_unknown_after_known_zero_with_name_ties() {
    let (_root, tethys, _db) = fixture();
    for (sort, expected) in [
        (CouplingSort::Afferent, [APP, ISOLATED, CORE8, CORE9]),
        (CouplingSort::Efferent, [CORE8, CORE9, ISOLATED, APP]),
        (CouplingSort::Instability, [ISOLATED, APP, CORE8, CORE9]),
    ] {
        assert_eq!(
            tethys
                .get_coupling_metrics(sort)
                .unwrap()
                .iter()
                .map(|row| row.package.name.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }
}
