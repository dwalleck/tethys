//! Packaged-worker and published CLI discovery contracts.

mod common;

#[test]
#[ignore = "requires explicitly packaged companion and installed MSBuild; release CI runs the same oracle"]
fn installed_companion() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = common::qualification_python();
    let host = std::env::var("TETHYS_QUALIFICATION_HOST").unwrap_or_else(|_| "sdk".into());
    let output = std::process::Command::new(python)
        .current_dir(root)
        .arg(root.join(".tethys-82a6/oracles/worker_qualification.py"))
        .args(["--host", &host, "--installed-only"])
        .output()
        .expect("launch installed-companion qualification (Python 3.11+ required)");
    assert!(
        output.status.success(),
        "clean-distribution/host/protocol qualification failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn untrusted_index_publishes_source_only_with_nonzero_status() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("App.csproj"), "<Project />").unwrap();
    std::fs::write(root.path().join("Source.cs"), "class Source {}").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
        .arg("-w")
        .arg(root.path())
        .arg("index")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "incomplete discovery must be observable"
    );
    let db = rusqlite::Connection::open(root.path().join(".rivets/index/tethys.db")).unwrap();
    let symbols: Vec<String> = db
        .prepare("SELECT name FROM symbols ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(symbols, ["Source"]);
    let tethys = tethys::Tethys::new(root.path()).unwrap();
    let snapshot = tethys.discovery_snapshot().expect("discovery publication");
    assert_eq!(snapshot.projects.len(), 1);
    assert_eq!(snapshot.projects[0].key.as_str(), "App.csproj");
    assert!(matches!(
        &snapshot.projects[0].standing,
        tethys::discovery::DiscoveryStanding::Indeterminate(failure)
            if failure.reason == tethys::discovery::DiscoveryFailureReason::TrustRequired
    ));
    assert!(!snapshot.is_complete());
    assert!(String::from_utf8_lossy(&output.stderr).contains("TrustRequired"));
}

#[test]
fn invalid_discovery_arguments_do_not_publish_an_index() {
    for args in [
        vec!["index", "--property", "Configuration"],
        vec!["index", "--property", "=Release"],
        vec!["index", "--property", " =Release"],
        vec!["index", "--import-profile", "lenient"],
        vec!["search", "Source", "--trust-msbuild"],
        vec!["search", "Source", "--allow-restore"],
    ] {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
            .arg("-w")
            .arg(root.path())
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            !root.path().join(".rivets/index/tethys.db").exists(),
            "{args:?}"
        );
    }
}

#[test]
fn restore_permission_does_not_grant_evaluation_and_queries_use_published_sources() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("App.csproj"), "<Project />").unwrap();
    std::fs::write(root.path().join("Source.cs"), "class Source {}").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
        .arg("-w")
        .arg(root.path())
        .args(["index", "--allow-restore"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let before = tethys::Tethys::new(root.path()).unwrap();
    assert!(matches!(
        &before.discovery_snapshot().expect("discovery publication").projects[0].standing,
        tethys::discovery::DiscoveryStanding::Indeterminate(failure)
            if failure.reason == tethys::discovery::DiscoveryFailureReason::TrustRequired
    ));
    std::fs::remove_file(root.path().join("App.csproj")).unwrap();
    std::fs::remove_file(root.path().join("Source.cs")).unwrap();
    let query = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
        .arg("-w")
        .arg(root.path())
        .args(["search", "Source"])
        .output()
        .unwrap();
    assert!(
        query.status.success(),
        "{}",
        String::from_utf8_lossy(&query.stderr)
    );
    assert!(String::from_utf8_lossy(&query.stdout).contains("Source"));
    let after = tethys::Tethys::new(root.path()).unwrap();
    assert_eq!(
        before.discovery_snapshot().expect("discovery publication"),
        after.discovery_snapshot().expect("discovery publication")
    );
}

#[test]
fn missing_selected_host_publishes_toolchain_failure_even_on_rebuild() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("App.csproj"), "<Project />").unwrap();
    std::fs::write(root.path().join("Source.cs"), "class Source {}").unwrap();
    for rebuild in [false, true] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"));
        command
            .arg("-w")
            .arg(root.path())
            .args(["index", "--trust-msbuild", "--msbuild-path"])
            .arg(root.path().join("missing-host"));
        if rebuild {
            command.arg("--rebuild");
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        let index = tethys::Tethys::new(root.path()).unwrap();
        assert!(matches!(
            &index.discovery_snapshot().expect("discovery publication").projects[0].standing,
            tethys::discovery::DiscoveryStanding::Indeterminate(failure)
                if failure.reason == tethys::discovery::DiscoveryFailureReason::ToolchainUnavailable
        ));
        assert!(String::from_utf8_lossy(&output.stderr).contains("ToolchainUnavailable"));
        let db = rusqlite::Connection::open(root.path().join(".rivets/index/tethys.db")).unwrap();
        let name: String = db
            .query_row("SELECT name FROM symbols", [], |row| row.get(0))
            .unwrap();
        assert_eq!(name, "Source");
    }
}

// Match discovery_cache's native hostpolicy marker: dependency probing and
// `dotnet --version` are not execution of the evaluator assembly.
fn evaluator_launches(log: &std::path::Path) -> usize {
    let trace = match std::fs::read_to_string(log) {
        Ok(trace) => trace,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => panic!("read native host trace: {error}"),
    };
    trace
        .lines()
        .filter(|line| {
            line.starts_with("Launch host: ")
                && line.contains("Tethys.MSBuild.Evaluate.dll, argc: ")
        })
        .count()
}

fn copy_distribution(source: &std::path::Path, destination: &std::path::Path) {
    std::fs::create_dir_all(destination).unwrap();
    for entry in std::fs::read_dir(source).expect("packaged companion directory") {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_distribution(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

struct NativeQueryFixture {
    _installation: tempfile::TempDir,
    root: tempfile::TempDir,
    sdk: std::path::PathBuf,
    executable: std::path::PathBuf,
    log: std::path::PathBuf,
    project: &'static str,
}

fn native_query_fixture() -> NativeQueryFixture {
    use std::path::PathBuf;

    let sdk = PathBuf::from(
        std::env::var_os("TETHYS_SDK_MSBUILD_PATH")
            .expect("select installed SDK via TETHYS_SDK_MSBUILD_PATH"),
    )
    .canonicalize()
    .unwrap();
    let distribution = std::env::var_os("TETHYS_WORKER_DISTRIBUTION").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/worker-dist/msbuild-evaluate"),
        PathBuf::from,
    );
    let installation = tempfile::tempdir().unwrap();
    copy_distribution(&distribution, &installation.path().join("msbuild-evaluate"));
    let executable = installation
        .path()
        .join(format!("tethys{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_tethys"), &executable).unwrap();
    let root = tempfile::tempdir().unwrap();
    let log = installation.path().join("host.trace");
    std::fs::write(
        root.path().join("global.json"),
        serde_json::json!({"sdk": {
            "version": sdk.file_name().unwrap().to_str().unwrap(),
            "rollForward": "disable"
        }})
        .to_string(),
    )
    .unwrap();
    let project = "<Project><PropertyGroup><TargetFrameworkIdentifier>.NETFramework</TargetFrameworkIdentifier><TargetFrameworkVersion>v4.8</TargetFrameworkVersion><TargetFrameworkProfile>Client</TargetFrameworkProfile><AssemblyName>Before</AssemblyName></PropertyGroup><ItemGroup><Compile Include=\"Source.cs\" /></ItemGroup></Project>";
    std::fs::write(root.path().join("App.csproj"), project).unwrap();
    std::fs::write(
        root.path().join("Source.cs"),
        "class Base {}\nclass Source : Base {\n    [System.Obsolete] public static void Leaf() {}\n    public static void Run() { Leaf(); }\n}\n",
    )
    .unwrap();
    NativeQueryFixture {
        _installation: installation,
        root,
        sdk,
        executable,
        log,
        project,
    }
}

impl NativeQueryFixture {
    fn execute(&self, args: &[&str]) -> std::process::Output {
        std::process::Command::new(&self.executable)
            .arg("-w")
            .arg(self.root.path())
            .args(args)
            .env("COREHOST_TRACE", "1")
            .env("COREHOST_TRACE_VERBOSITY", "4")
            .env("COREHOST_TRACEFILE", &self.log)
            .env("DOTNET_HOST_TRACE", "1")
            .env("DOTNET_HOST_TRACE_VERBOSITY", "4")
            .env("DOTNET_HOST_TRACEFILE", &self.log)
            .env("NO_COLOR", "1")
            // Logs have wall-clock timestamps; disable diagnostics rather than
            // normalizing either stream of the observable command response.
            .env("RUST_LOG", "off")
            .output()
            .unwrap()
    }

    fn run(&self, args: &[&str]) -> Vec<u8> {
        let output = self.execute(args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }
}

fn persisted_query_commands(selector: &str) -> Vec<Vec<&str>> {
    // Every query command in Commands is exercised against this publication,
    // including coupling's four human/JSON table/detail variants.
    // Do not request --lsp: that is a separately opted-in external service,
    // not the evaluation-free persisted-query boundary being qualified here.
    vec![
        vec!["search", "Source"],
        vec!["callers", "Source::Leaf"],
        vec!["coupling", "--json"],
        vec!["coupling", "--package", selector, "--json"],
        vec!["coupling"],
        vec!["coupling", "--package", selector],
        vec!["impact", "Source.cs"],
        vec!["cycles"],
        vec!["stats"],
        vec!["reachable", "Source::Run"],
        vec!["affected-tests", "Source.cs", "--names-only"],
        vec!["panic-points", "--json"],
        vec!["deprecated-callers", "--json"],
        vec!["visibility-tightening", "--json"],
        vec!["unused-imports", "--json"],
        vec!["untested-code", "--json"],
        vec!["dead-code", "--json"],
        vec!["hierarchy", "Source", "--json"],
    ]
}

fn published_query_response(
    fixture: &NativeQueryFixture,
    args: &[&str],
) -> (Option<i32>, Vec<u8>, Vec<u8>) {
    let output = fixture.execute(args);
    if args[0] == "affected-tests" {
        assert!(
            matches!(output.status.code(), Some(0 | 2)),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    } else {
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    (output.status.code(), output.stdout, output.stderr)
}

#[test]
#[ignore = "requires packaged real worker and TETHYS_SDK_MSBUILD_PATH; native host tracing proves query isolation"]
fn coupling_queries_read_published_units_without_launching_evaluation() {
    let fixture = native_query_fixture();
    let NativeQueryFixture {
        root,
        sdk,
        log,
        project,
        ..
    } = &fixture;
    let run = |args: &[&str]| fixture.run(args);
    let execute = |args: &[&str]| fixture.execute(args);
    let index_args = [
        "index",
        "--trust-msbuild",
        "--msbuild-path",
        sdk.to_str().unwrap(),
    ];
    run(&index_args);
    assert!(
        evaluator_launches(log) > 0,
        "trusted indexing positive control must execute the packaged worker"
    );
    let table = run(&["coupling", "--json"]);
    let parsed: serde_json::Value = serde_json::from_slice(&table).unwrap();
    assert_eq!(parsed["count"], 1);
    let row = &parsed["packages"][0];
    assert_eq!(row["source"], "msbuild");
    // A published evaluation unit carries no file attribution, so its zero is
    // the shape of the graph, not a measurement: the query reports unknown and
    // names why, rather than publishing a confident 0.
    assert_eq!(row["afferent"], serde_json::Value::Null);
    assert_eq!(row["efferent"], serde_json::Value::Null);
    assert_eq!(row["instability"], serde_json::Value::Null);
    assert_eq!(row["evaluation_unit"]["assembly_name"], "Before");
    assert_eq!(
        row["metric_evidence"],
        serde_json::json!({
            "afferent": {"standing": "indeterminate", "reason": "unattributed_evaluation_unit"},
            "efferent": {"standing": "indeterminate", "reason": "unattributed_evaluation_unit"},
            "instability": {"standing": "indeterminate", "reason": "unattributed_evaluation_unit"}
        })
    );
    let selector = row["name"].as_str().unwrap();
    let detail_args = ["coupling", "--package", selector, "--json"];
    let detail = run(&detail_args);
    let parsed_detail: serde_json::Value = serde_json::from_slice(&detail).unwrap();
    assert_eq!(parsed_detail["outgoing"], serde_json::json!([]));
    assert_eq!(parsed_detail["incoming"], serde_json::json!([]));
    let queries = persisted_query_commands(selector);
    let published: Vec<_> = queries
        .iter()
        .map(|args| published_query_response(&fixture, args))
        .collect();

    // Change an evaluation input, not just a source mtime. A forbidden evaluation
    // cannot hide behind an eligible cache hit and must observe a different name.
    std::fs::write(
        root.path().join("App.csproj"),
        project.replace("Before", "After"),
    )
    .unwrap();
    std::fs::remove_file(log).unwrap();
    for (args, expected) in queries.iter().zip(&published) {
        let output = execute(args);
        assert_eq!(
            (output.status.code(), output.stdout, output.stderr),
            *expected,
            "{args:?} must read the unchanged publication"
        );
    }
    assert_eq!(
        evaluator_launches(log),
        0,
        "published queries must not launch the evaluator"
    );

    // Same changed input, binary, host and trace configuration: authorized
    // indexing must miss the cache and publish the new evaluation instead.
    run(&index_args);
    assert!(
        evaluator_launches(log) > 0,
        "changed-input control must launch the same evaluator"
    );
    let changed: serde_json::Value = serde_json::from_slice(&run(&["coupling", "--json"])).unwrap();
    assert_eq!(
        changed["packages"][0]["evaluation_unit"]["assembly_name"],
        "After"
    );
}

#[cfg(unix)]
#[test]
fn unreadable_directory_in_a_project_less_workspace_does_not_fail_the_run() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src")).unwrap();
    std::fs::write(root.path().join("src/lib.rs"), "pub fn kept() {}\n").unwrap();
    let blocked = root.path().join("src/blocked");
    std::fs::create_dir_all(&blocked).unwrap();
    std::fs::write(blocked.join("hidden.rs"), "pub fn hidden() {}\n").unwrap();
    let permissions = std::fs::metadata(&blocked).unwrap().permissions();
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let blocked_readable = std::fs::read_dir(&blocked).is_err();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tethys"))
        .arg("-w")
        .arg(root.path())
        .arg("index")
        .output()
        .unwrap();
    std::fs::set_permissions(&blocked, permissions).unwrap();
    if !blocked_readable {
        return; // Elevated users cannot exercise a directory-permission boundary.
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "a workspace with nothing to evaluate must publish without failing:\n{stderr}"
    );
    assert!(
        stderr.contains("blocked"),
        "the unreadable directory must still be reported: {stderr}"
    );
    let reopened = tethys::Tethys::new(root.path()).expect("reopen published revision");
    assert!(
        reopened
            .get_file(std::path::Path::new("src/lib.rs"))
            .unwrap()
            .is_some(),
        "readable source must still be published"
    );
}
