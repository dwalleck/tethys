//! S2 worker-only fences. No native discovery/index integration is implied.

#[test]
#[ignore = "requires explicitly packaged companion and installed MSBuild; CI runs the authoritative runner"]
fn sdk_classic_metadata() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = std::env::var_os("PYTHON").unwrap_or_else(|| "python3".into());
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
