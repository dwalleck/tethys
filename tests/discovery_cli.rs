//! S2 packaged-worker seam only; CLI discovery behavior belongs to later slices.

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
