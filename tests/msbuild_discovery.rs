//! Independent native-worker and atomic discovery-publication fences.

mod common;

#[test]
#[ignore = "requires explicitly packaged companion and installed MSBuild; CI runs the authoritative runner"]
fn sdk_classic_metadata() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = common::qualification_python();
    let host = std::env::var("TETHYS_QUALIFICATION_HOST").unwrap_or_else(|_| "sdk".into());
    let output = std::process::Command::new(python)
        .current_dir(root)
        .arg(root.join(".tethys-82a6/oracles/worker_qualification.py"))
        .args(["--host", &host])
        .output()
        .expect("launch real MSBuild qualification (Python 3.11+ required)");
    assert!(
        output.status.success(),
        "independent per-framework metadata/sentinel qualification failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use tethys::discovery::{
    DiscoveryFailureReason, DiscoveryOptions, DiscoverySnapshot, DiscoveryStanding,
};
use tethys::{IndexOptions, Tethys};

fn native_options() -> IndexOptions {
    IndexOptions::default().with_discovery(DiscoveryOptions {
        trust_msbuild: true,
        msbuild_path: Some(PathBuf::from(
            std::env::var_os("TETHYS_SDK_MSBUILD_PATH")
                .expect("select installed SDK via TETHYS_SDK_MSBUILD_PATH"),
        )),
        companion_directory: Some(std::env::var_os("TETHYS_WORKER_DISTRIBUTION").map_or_else(
            || {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("target/worker-dist/msbuild-evaluate")
            },
            PathBuf::from,
        )),
        ..DiscoveryOptions::default()
    })
}

fn project(root: &Path, name: &str, body: &str) {
    fs::write(
        root.join(name),
        format!(
            "<Project><PropertyGroup><TargetFrameworkIdentifier>.NETFramework</TargetFrameworkIdentifier><TargetFrameworkVersion>v4.8</TargetFrameworkVersion><AssemblyName>Collision</AssemblyName></PropertyGroup>{body}</Project>"
        ),
    )
    .unwrap();
}

fn observer(root: &Path) -> Connection {
    Connection::open(root.join(".rivets/index/tethys.db")).unwrap()
}

fn strings(connection: &Connection, sql: &str) -> Vec<String> {
    connection
        .prepare(sql)
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

fn membership(connection: &Connection) -> Vec<(String, String, Option<String>)> {
    connection
        .prepare(
            "SELECT p.project_key, m.path, f.path
             FROM file_participation m
             JOIN evaluation_units u ON u.unit_key = m.unit_key
             JOIN projects p ON p.project_key = u.project_key
             LEFT JOIN files f ON f.id = m.file_id
             ORDER BY p.project_key, m.path",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

fn assert_reopened(root: &Path, expected: &DiscoverySnapshot) {
    let reopened = Tethys::new(root).expect("open persisted discovery without native options");
    assert_eq!(reopened.discovery_snapshot(), expected);
}

fn failure(standing: &DiscoveryStanding) -> DiscoveryFailureReason {
    match standing {
        DiscoveryStanding::Indeterminate(failure) => failure.reason,
        DiscoveryStanding::Confirmed => panic!("expected bounded failure"),
    }
}

fn assert_initial_shared_membership(initial: &DiscoverySnapshot, sql: &Connection) {
    let mut authored = initial
        .units
        .iter()
        .map(|unit| {
            (
                unit.project.as_str(),
                unit.properties["DefineConstants"].as_str(),
                unit.properties["AssemblyName"].as_str(),
            )
        })
        .collect::<Vec<_>>();
    authored.sort_unstable();
    assert_eq!(
        authored,
        [
            ("A.csproj", "FIRST", "Collision"),
            ("B.csproj", "SECOND", "Collision")
        ]
    );
    assert_ne!(initial.units[0].key, initial.units[1].key);
    assert_eq!(
        strings(sql, "SELECT path FROM files ORDER BY path"),
        ["Second.cs", "Shared.cs"],
        "shared syntax is stored once; a distinct physical file is the positive control"
    );
    assert_eq!(
        membership(sql),
        [
            (
                "A.csproj".into(),
                "Second.cs".into(),
                Some("Second.cs".into())
            ),
            (
                "A.csproj".into(),
                "Shared.cs".into(),
                Some("Shared.cs".into())
            ),
            (
                "B.csproj".into(),
                "Shared.cs".into(),
                Some("Shared.cs".into())
            ),
        ]
    );
    let links: Vec<(String, Option<String>)> = sql
        .prepare("SELECT u.project_key, m.link FROM file_participation m JOIN evaluation_units u ON u.unit_key=m.unit_key WHERE m.path='Shared.cs' ORDER BY u.project_key")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        links,
        [
            ("A.csproj".into(), Some("Linked/Shared.cs".into())),
            ("B.csproj".into(), None)
        ]
    );
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn membership_replacement() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Shared.cs"), "public class Shared {}\n").unwrap();
    fs::write(root.path().join("Second.cs"), "public class Second {}\n").unwrap();
    project(
        root.path(),
        "A.csproj",
        "<PropertyGroup><DefineConstants>FIRST</DefineConstants></PropertyGroup><ItemGroup><Compile Include=\"Shared.cs\"><Link>Linked/Shared.cs</Link></Compile><Compile Include=\"Second.cs\" /></ItemGroup>",
    );
    project(
        root.path(),
        "B.csproj",
        "<PropertyGroup><DefineConstants>SECOND</DefineConstants></PropertyGroup><ItemGroup><Compile Include=\"Shared.cs\" /></ItemGroup>",
    );
    let mut index = Tethys::new(root.path()).unwrap();
    let initial = index.index_with_options(native_options()).unwrap();
    assert!(initial.discovery.is_complete(), "{:?}", initial.discovery);
    let sql = observer(root.path());
    assert_initial_shared_membership(&initial.discovery, &sql);
    assert_reopened(root.path(), &initial.discovery);

    // Only project inputs change: neither source's bytes nor timestamp is touched.
    let source_bytes = fs::read(root.path().join("Shared.cs")).unwrap();
    let source_modified = fs::metadata(root.path().join("Shared.cs"))
        .unwrap()
        .modified()
        .unwrap();
    project(
        root.path(),
        "A.csproj",
        "<PropertyGroup><DefineConstants>FIRST</DefineConstants></PropertyGroup><ItemGroup><Compile Include=\"Second.cs\" /></ItemGroup>",
    );
    fs::rename(
        root.path().join("B.csproj"),
        root.path().join("Moved.csproj"),
    )
    .unwrap();
    let replacement = index.index_with_options(native_options()).unwrap();
    assert!(
        replacement.discovery.is_complete(),
        "{:?}",
        replacement.discovery
    );
    assert_eq!(
        fs::read(root.path().join("Shared.cs")).unwrap(),
        source_bytes
    );
    assert_eq!(
        fs::metadata(root.path().join("Shared.cs"))
            .unwrap()
            .modified()
            .unwrap(),
        source_modified
    );
    assert_eq!(
        strings(
            &sql,
            "SELECT project_key FROM evaluation_units ORDER BY project_key"
        ),
        ["A.csproj", "Moved.csproj"],
        "removed project/unit identities must not survive replacement"
    );
    assert_eq!(
        membership(&sql),
        [
            (
                "A.csproj".into(),
                "Second.cs".into(),
                Some("Second.cs".into())
            ),
            (
                "Moved.csproj".into(),
                "Shared.cs".into(),
                Some("Shared.cs".into())
            ),
        ]
    );
    assert_reopened(root.path(), &replacement.discovery);

    // Failed syntax removes its physical row, not evaluated Compile participation.
    fs::write(root.path().join("Shared.cs"), [0xff, 0xfe]).unwrap();
    let invalid_source = index.index_with_options(native_options()).unwrap();
    assert!(invalid_source.discovery.is_complete());
    assert_eq!(
        strings(&sql, "SELECT path FROM files ORDER BY path"),
        ["Second.cs"]
    );
    assert_eq!(
        membership(&sql),
        [
            (
                "A.csproj".into(),
                "Second.cs".into(),
                Some("Second.cs".into())
            ),
            ("Moved.csproj".into(), "Shared.cs".into(), None),
        ]
    );
    assert_reopened(root.path(), &invalid_source.discovery);
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn bounded_failures_replace_confirmed_units() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Shared.cs"), "public class Shared {}\n").unwrap();
    let body = "<PropertyGroup><TargetFrameworks>good;broken</TargetFrameworks></PropertyGroup><ItemGroup><Compile Include=\"Shared.cs\" /></ItemGroup>";
    project(root.path(), "Multi.csproj", body);
    let mut index = Tethys::new(root.path()).unwrap();
    let initial = index.index_with_options(native_options()).unwrap();
    assert!(initial.discovery.is_complete(), "{:?}", initial.discovery);
    let sql = observer(root.path());
    assert_eq!(
        strings(
            &sql,
            "SELECT target_framework FROM evaluation_units ORDER BY target_framework"
        ),
        ["broken", "good"]
    );

    project(
        root.path(),
        "Multi.csproj",
        &format!(
            "{body}<Import Project=\"absent.targets\" Condition=\"'$(TargetFramework)' == 'broken'\" />"
        ),
    );
    let partial = index.index_with_options(native_options()).unwrap();
    assert!(!partial.discovery.is_complete());
    assert_eq!(partial.discovery.projects[0].key.as_str(), "Multi.csproj");
    assert_eq!(
        failure(&partial.discovery.projects[0].standing),
        DiscoveryFailureReason::PartialTargetFrameworks
    );
    let failed = partial
        .discovery
        .units
        .iter()
        .find(|unit| unit.target_framework.as_deref() == Some("broken"))
        .unwrap();
    assert_eq!(
        failure(&failed.standing),
        DiscoveryFailureReason::EvaluationFailed
    );
    let good = partial
        .discovery
        .units
        .iter()
        .find(|unit| unit.target_framework.as_deref() == Some("good"))
        .unwrap();
    assert_eq!(good.standing, DiscoveryStanding::Confirmed);
    assert_eq!(
        strings(
            &sql,
            "SELECT u.target_framework || ':' || m.path FROM evaluation_units u JOIN file_participation m ON m.unit_key=u.unit_key ORDER BY 1"
        ),
        ["good:Shared.cs"],
        "previously confirmed failed-inner membership must be removed"
    );
    let standing: String = sql
        .query_row(
            "SELECT standing_json FROM evaluation_units WHERE target_framework='broken'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        failure(&serde_json::from_str::<DiscoveryStanding>(&standing).unwrap()),
        DiscoveryFailureReason::EvaluationFailed
    );
    assert_reopened(root.path(), &partial.discovery);

    project(
        root.path(),
        "Multi.csproj",
        "<Import Project=\"absent.targets\" />",
    );
    let outer = index.index_with_options(native_options()).unwrap();
    assert!(!outer.discovery.is_complete());
    assert_eq!(outer.discovery.projects[0].key.as_str(), "Multi.csproj");
    assert_eq!(
        failure(&outer.discovery.projects[0].standing),
        DiscoveryFailureReason::EvaluationFailed
    );
    assert!(outer.discovery.units.is_empty());
    assert_eq!(
        strings(&sql, "SELECT unit_key FROM evaluation_units"),
        Vec::<String>::new()
    );
    assert!(membership(&sql).is_empty());
    assert_eq!(strings(&sql, "SELECT path FROM files"), ["Shared.cs"]);
    let standing: String = sql
        .query_row(
            "SELECT standing_json FROM projects WHERE project_key='Multi.csproj'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        failure(&serde_json::from_str::<DiscoveryStanding>(&standing).unwrap()),
        DiscoveryFailureReason::EvaluationFailed
    );
    assert_reopened(root.path(), &outer.discovery);
}

#[test]
fn untrusted_publication_retains_source_only_facts() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Shared.cs"), "public class Shared {}\n").unwrap();
    project(
        root.path(),
        "Untrusted.csproj",
        "<ItemGroup><Compile Include=\"Shared.cs\" /></ItemGroup>",
    );
    let mut index = Tethys::new(root.path()).unwrap();
    let stats = index.index().unwrap();
    assert!(!stats.discovery.is_complete());
    assert_eq!(stats.discovery.projects[0].key.as_str(), "Untrusted.csproj");
    assert_eq!(
        failure(&stats.discovery.projects[0].standing),
        DiscoveryFailureReason::TrustRequired
    );
    assert!(stats.discovery.units.is_empty());
    let sql = observer(root.path());
    assert_eq!(strings(&sql, "SELECT path FROM files"), ["Shared.cs"]);
    assert_eq!(
        strings(&sql, "SELECT name FROM symbols WHERE name='Shared'"),
        ["Shared"]
    );
    assert!(membership(&sql).is_empty());
    let standing: String = sql
        .query_row(
            "SELECT standing_json FROM projects WHERE project_key='Untrusted.csproj'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        failure(&serde_json::from_str::<DiscoveryStanding>(&standing).unwrap()),
        DiscoveryFailureReason::TrustRequired
    );
    assert_reopened(root.path(), &stats.discovery);
}

#[test]
#[ignore = "requires explicitly packaged companion and installed MSBuild"]
fn evaluated_only_sources_share_publication_and_freshness_identity() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("obj")).unwrap();
    let source = root.path().join("obj/Authored.cs");
    fs::write(&source, "public class Authored {}").unwrap();
    project(
        root.path(),
        "Authored.csproj",
        "<ItemGroup><Compile Include=\"obj/Authored.cs\" /></ItemGroup>",
    );
    let mut index = Tethys::new(root.path()).unwrap();
    index.index_with_options(native_options()).unwrap();
    let sql = observer(root.path());
    assert_eq!(strings(&sql, "SELECT path FROM files"), ["obj/Authored.cs"]);
    assert!(!index.needs_update().unwrap());
    let reopened = Tethys::new(root.path()).unwrap();
    let current = reopened.get_stale_files().unwrap();
    assert!(current.modified.is_empty() && current.added.is_empty() && current.deleted.is_empty());
    fs::write(&source, "public class AuthoredChanged {}").unwrap();
    assert_eq!(
        reopened.get_stale_files().unwrap().modified,
        [PathBuf::from("obj/Authored.cs")]
    );
    fs::remove_file(root.path().join("Authored.csproj")).unwrap();
    index.index().unwrap();
    assert!(index.discovery_snapshot().units.is_empty());
    assert_eq!(
        strings(&sql, "SELECT name FROM symbols"),
        ["AuthoredChanged"]
    );
    assert!(!index.needs_update().unwrap());
    fs::remove_file(&source).unwrap();
    assert_eq!(
        index.get_stale_files().unwrap().deleted,
        [PathBuf::from("obj/Authored.cs")]
    );
}
