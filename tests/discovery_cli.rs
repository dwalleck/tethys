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
