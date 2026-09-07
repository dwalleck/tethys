//! Runtime extensions remain executable but cannot close an evaluation cache recipe.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use tethys::discovery::*;

fn sdk() -> PathBuf {
    PathBuf::from(std::env::var_os("TETHYS_SDK_MSBUILD_PATH").expect("select an installed SDK"))
        .canonicalize()
        .unwrap()
}

fn dotnet(sdk: &Path) -> PathBuf {
    sdk.parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(if cfg!(windows) {
            "dotnet.exe"
        } else {
            "dotnet"
        })
        .canonicalize()
        .unwrap()
}

fn worker_distribution() -> PathBuf {
    std::env::var_os("TETHYS_WORKER_DISTRIBUTION").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/worker-dist/msbuild-evaluate"),
        PathBuf::from,
    )
}

fn fixture() -> TempDir {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("App.csproj"), "<Project><PropertyGroup><TargetFrameworkIdentifier>.NETFramework</TargetFrameworkIdentifier><TargetFrameworkVersion>v4.8</TargetFrameworkVersion></PropertyGroup><ItemGroup><Compile Include=\"App.cs\" /></ItemGroup></Project>").unwrap();
    fs::write(root.path().join("App.cs"), "class App {}\n").unwrap();
    fs::write(
        root.path().join("global.json"),
        serde_json::json!({"sdk": {"version": sdk().file_name().unwrap().to_str().unwrap(), "rollForward": "disable"}}).to_string(),
    ).unwrap();
    root
}

fn child(root: &Path, state: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--ignored", "--exact", "runtime_extension_child"])
        .env("TETHYS_RUNTIME_ROOT", root)
        .env("TETHYS_RUNTIME_STATE", state);
    command
}

fn run(command: &mut Command, state: &Path) -> serde_json::Value {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&fs::read(state).unwrap()).unwrap()
}

fn state(directory: &Path, name: &str) -> PathBuf {
    let path = directory.join(name);
    fs::write(&path, "{\"cache\":[]}").unwrap();
    path
}

fn assert_defines(snapshot: &serde_json::Value, expected: &str) {
    assert_eq!(
        snapshot["units"][0]["properties"]["DefineConstants"],
        expected
    );
}

// Build only this explicitly prepared fixture, using the selected SDK's compiler
// and installed targeting pack. No project evaluation/restore or downloaded packages.
fn compile_startup_hook(directory: &Path) -> PathBuf {
    let sdk = sdk();
    let root = sdk.parent().unwrap().parent().unwrap();
    let packs = root.join("packs/Microsoft.NETCore.App.Ref");
    let config: serde_json::Value = serde_json::from_slice(
        &fs::read(worker_distribution().join("sdk/Tethys.MSBuild.Evaluate.runtimeconfig.json"))
            .unwrap(),
    )
    .unwrap();
    let tfm = config["runtimeOptions"]["tfm"]
        .as_str()
        .expect("worker target framework");
    let mut versions: Vec<_> = fs::read_dir(&packs)
        .unwrap()
        .map(|entry| entry.unwrap().path().join("ref").join(tfm))
        .filter(|path| path.is_dir())
        .collect();
    versions.sort();
    let references = versions
        .last()
        .expect("installed targeting pack matching the worker target framework");
    let source = directory.join("StartupHook.cs");
    let assembly = directory.join("StartupHook.dll");
    fs::write(&source, "public class StartupHook { public static void Initialize() { System.Environment.SetEnvironmentVariable(\"DefineConstants\", System.IO.File.ReadAllText(System.Environment.GetEnvironmentVariable(\"TETHYS_RUNTIME_EXTERNAL\")).Trim()); } }\n").unwrap();
    let mut compiler = Command::new(dotnet(&sdk));
    compiler
        .arg(sdk.join("Roslyn/bincore/csc.dll"))
        .args(["-nologo", "-target:library", "-nostdlib+"])
        .arg(format!("-out:{}", assembly.display()))
        .arg(&source);
    for entry in fs::read_dir(references).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "dll") {
            compiler.arg(format!("-reference:{}", path.display()));
        }
    }
    let output = compiler.output().unwrap();
    assert!(
        output.status.success(),
        "fixture compiler: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assembly
}

#[test]
#[ignore = "requires packaged real worker, selected installed SDK and targeting pack"]
fn startup_hook_external_reads_never_reuse_literal_metadata() {
    let root = fixture();
    let external = TempDir::new().unwrap();
    let native_state = state(external.path(), "native.json");
    let mut native = child(root.path(), &native_state);
    let first = run(&mut native, &native_state);
    let hit = run(&mut native, &native_state);
    assert_eq!(hit["observations"][0]["reused"], true);
    assert_eq!(first["units"], hit["units"]);

    let hook = compile_startup_hook(external.path());
    let value = external.path().join("value");
    let hook_state = state(external.path(), "hook.json");
    let mut hooked = child(root.path(), &hook_state);
    hooked
        .env("DOTNET_STARTUP_HOOKS", hook)
        .env("TETHYS_RUNTIME_EXTERNAL", &value);
    fs::write(&value, "HOOK_ONE").unwrap();
    assert_defines(&run(&mut hooked, &hook_state), "HOOK_ONE");
    fs::write(&value, "HOOK_TWO").unwrap();
    let refreshed = run(&mut hooked, &hook_state);
    assert_defines(&refreshed, "HOOK_TWO");
    assert_eq!(refreshed["cache"], serde_json::json!([]));
}

#[cfg(unix)]
#[test]
#[ignore = "requires packaged real worker and selected installed SDK"]
fn forwarding_muxer_external_reads_never_reuse_literal_metadata() {
    use std::os::unix::fs::PermissionsExt;
    let root = fixture();
    let external = TempDir::new().unwrap();
    let launcher = external.path().join("dotnet");
    fs::write(&launcher, "#!/bin/sh\ncase \"$1\" in --*) ;; *) DefineConstants=$(cat \"$TETHYS_RUNTIME_EXTERNAL\") || exit 1; export DefineConstants ;; esac\nexec \"$TETHYS_RUNTIME_NATIVE\" \"$@\"\n").unwrap();
    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755)).unwrap();
    let value = external.path().join("value");
    let wrapper_state = state(external.path(), "wrapper.json");
    let mut wrapped = child(root.path(), &wrapper_state);
    wrapped
        .env("DOTNET", &launcher)
        .env("TETHYS_RUNTIME_IMPLICIT", "1")
        .env("TETHYS_RUNTIME_NATIVE", dotnet(&sdk()))
        .env("TETHYS_RUNTIME_EXTERNAL", &value);
    fs::write(&value, "WRAPPER_ONE").unwrap();
    assert_defines(&run(&mut wrapped, &wrapper_state), "WRAPPER_ONE");
    fs::write(&value, "WRAPPER_TWO").unwrap();
    let refreshed = run(&mut wrapped, &wrapper_state);
    assert_defines(&refreshed, "WRAPPER_TWO");
    assert_eq!(refreshed["cache"], serde_json::json!([]));
}

#[test]
#[ignore = "subprocess-only runtime extension regression"]
fn runtime_extension_child() {
    let Some(root) = std::env::var_os("TETHYS_RUNTIME_ROOT") else {
        return;
    };
    let state = PathBuf::from(std::env::var_os("TETHYS_RUNTIME_STATE").unwrap());
    let previous: serde_json::Value = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    let options = DiscoveryOptions {
        trust_msbuild: true,
        msbuild_path: if std::env::var_os("TETHYS_RUNTIME_IMPLICIT").is_some() {
            None
        } else {
            Some(sdk())
        },
        companion_directory: Some(worker_distribution()),
        ..DiscoveryOptions::default()
    };
    let snapshot = MsBuildDiscovery
        .discover(
            &DiscoveryRequest::new(Path::new(&root), options)
                .unwrap()
                .with_cache(serde_json::from_value(previous["cache"].clone()).unwrap()),
        )
        .unwrap();
    assert_eq!(snapshot.projects.len(), 1, "{snapshot:?}");
    assert_eq!(
        snapshot.projects[0].standing,
        DiscoveryStanding::Confirmed,
        "{snapshot:?}"
    );
    assert_eq!(snapshot.units.len(), 1, "{snapshot:?}");
    assert_eq!(
        snapshot.units[0].standing,
        DiscoveryStanding::Confirmed,
        "{snapshot:?}"
    );
    assert_eq!(
        snapshot.units[0].framework.as_ref().unwrap().identifier,
        ".NETFramework"
    );
    assert_eq!(snapshot.units[0].sources[0].path, Path::new("App.cs"));
    fs::write(state, serde_json::to_vec(&serde_json::json!({"cache": snapshot.cache, "units": snapshot.units, "observations": snapshot.cache_observations})).unwrap()).unwrap();
}
