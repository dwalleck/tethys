//! Public-seam failure fences. Successful metadata always comes from the real companion.
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tethys::discovery::*;

fn write(root: &Path, name: &str, text: &str) {
    fs::write(root.join(name), text).unwrap();
}
fn literal(items: &str) -> String {
    format!(
        "<Project><PropertyGroup><TargetFrameworkIdentifier>.NETFramework</TargetFrameworkIdentifier><TargetFrameworkVersion>v4.8</TargetFrameworkVersion></PropertyGroup>{items}</Project>"
    )
}
fn options() -> DiscoveryOptions {
    DiscoveryOptions {
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
    }
}
fn discover(root: &Path, options: DiscoveryOptions) -> DiscoverySnapshot {
    MsBuildDiscovery
        .discover(&DiscoveryRequest::new(root, options).unwrap())
        .unwrap()
}
fn reason(standing: &DiscoveryStanding) -> DiscoveryFailureReason {
    match standing {
        DiscoveryStanding::Indeterminate(failure) => failure.reason,
        DiscoveryStanding::Confirmed => panic!("expected explicit incomplete coverage"),
    }
}

#[test]
fn trust_gate_precedes_host_and_restore() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "App.csproj",
        "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>",
    );
    let snapshot = discover(
        root.path(),
        DiscoveryOptions {
            allow_restore: true,
            msbuild_path: Some(root.path().join("missing")),
            ..DiscoveryOptions::default()
        },
    );
    assert_eq!(snapshot.projects.len(), 1);
    assert_eq!(
        reason(&snapshot.projects[0].standing),
        DiscoveryFailureReason::TrustRequired
    );
    assert!(snapshot.units.is_empty());
    assert_eq!(
        snapshot.grants,
        DiscoveryGrants {
            trust_msbuild: false,
            allow_restore: true,
        }
    );
    assert!(!snapshot.is_complete());
    assert!(snapshot.inputs.is_empty());
    assert!(!root.path().join("obj").exists());
}

#[test]
fn malformed_input_and_missing_toolchain_are_not_empty_success() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "Broken.csproj",
        "<Project><PropertyGroup></Project>",
    );
    let snapshot = discover(
        root.path(),
        DiscoveryOptions {
            trust_msbuild: true,
            ..DiscoveryOptions::default()
        },
    );
    assert_eq!(
        reason(&snapshot.projects[0].standing),
        DiscoveryFailureReason::MalformedInput
    );
    write(root.path(), "Broken.csproj", &literal(""));
    let snapshot = discover(
        root.path(),
        DiscoveryOptions {
            trust_msbuild: true,
            msbuild_path: Some(root.path().join("missing")),
            ..DiscoveryOptions::default()
        },
    );
    assert_eq!(
        reason(&snapshot.projects[0].standing),
        DiscoveryFailureReason::ToolchainUnavailable
    );
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn reason_matrix_native_failures_preserve_successful_siblings() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "Good.csproj",
        &literal("<ItemGroup><Compile Include=\"Good.cs\" /></ItemGroup>"),
    );
    write(root.path(), "Good.cs", "class Good {}\n");
    write(
        root.path(),
        "Sdk.csproj",
        "<Project Sdk=\"Tethys.Does.Not.Exist/0.0.0\" />",
    );
    write(
        root.path(),
        "Import.csproj",
        &literal("<Import Project=\"absent.targets\" />"),
    );
    write(
        root.path(),
        "Restore.csproj",
        "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>",
    );
    let snapshot = discover(root.path(), options());
    for (name, expected) in [
        ("Sdk.csproj", DiscoveryFailureReason::SdkUnresolved),
        ("Import.csproj", DiscoveryFailureReason::EvaluationFailed),
        ("Restore.csproj", DiscoveryFailureReason::RestoreRequired),
    ] {
        let project = snapshot
            .projects
            .iter()
            .find(|project| project.key.as_str() == name)
            .unwrap();
        assert_eq!(
            reason(&project.standing),
            expected,
            "{name}: {:?}",
            project.standing
        );
    }
    let good = snapshot
        .units
        .iter()
        .find(|unit| unit.project.as_str() == "Good.csproj")
        .unwrap();
    assert_eq!(good.standing, DiscoveryStanding::Confirmed);
    assert_eq!(good.sources[0].path, Path::new("Good.cs"));
    assert_eq!(good.framework.as_ref().unwrap().version, "v4.8");
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn explicit_host_is_not_rejected_by_a_conflicting_global_json() {
    let root = TempDir::new().unwrap();
    let selected = options();
    let sdk = selected.msbuild_path.as_ref().unwrap();
    let selected_version = sdk.file_name().unwrap().to_str().unwrap();
    // An explicit installation selects its own muxer; only implicit discovery consults
    // global.json policy. Pin a different installed SDK so the old conflict branch fired.
    let muxer = sdk.parent().and_then(Path::parent).unwrap().join("dotnet");
    let listed = std::process::Command::new(muxer)
        .arg("--list-sdks")
        .output()
        .unwrap();
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let inventory: Vec<String> = String::from_utf8(listed.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| line.split_once(" [").map(|(version, _)| version.to_owned()))
        .collect();
    let pin = inventory
        .iter()
        .find(|version| version.as_str() != selected_version)
        .cloned()
        .unwrap_or_else(|| "0.0.1".to_owned());
    write(
        root.path(),
        "global.json",
        &serde_json::json!({"sdk": {"version": pin, "rollForward": "disable"}}).to_string(),
    );
    write(
        root.path(),
        "App.csproj",
        &literal("<ItemGroup><Compile Include=\"App.cs\" /></ItemGroup>"),
    );
    write(root.path(), "App.cs", "class App {}\n");
    let snapshot = discover(root.path(), selected);
    assert_eq!(
        snapshot.projects[0].standing,
        DiscoveryStanding::Confirmed,
        "{:?}",
        snapshot.projects
    );
    assert_eq!(snapshot.units[0].sources[0].path, Path::new("App.cs"));
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn partial_framework_failure_retains_selector_and_success() {
    let root = TempDir::new().unwrap();
    write(root.path(), "Shared.cs", "class Shared {}\n");
    write(
        root.path(),
        "Multi.csproj",
        &literal(
            "<PropertyGroup><TargetFrameworks>good;broken</TargetFrameworks></PropertyGroup><ItemGroup><Compile Include=\"Shared.cs\"><Link>Linked.cs</Link></Compile></ItemGroup><Import Project=\"absent.targets\" Condition=\"'$(TargetFramework)' == 'broken'\" />",
        ),
    );
    let mut snapshot = discover(root.path(), options());
    assert_eq!(
        reason(&snapshot.projects[0].standing),
        DiscoveryFailureReason::PartialTargetFrameworks
    );
    assert_eq!(snapshot.units.len(), 2);
    let good = snapshot
        .units
        .iter()
        .find(|unit| unit.target_framework.as_deref() == Some("good"))
        .unwrap();
    assert_eq!(good.standing, DiscoveryStanding::Confirmed);
    assert_eq!(good.sources[0].link.as_deref(), Some("Linked.cs"));
    let broken = snapshot
        .units
        .iter()
        .find(|unit| unit.target_framework.as_deref() == Some("broken"))
        .unwrap();
    assert!(broken.framework.is_none());
    assert_eq!(
        reason(&broken.standing),
        DiscoveryFailureReason::EvaluationFailed
    );
    assert_ne!(good.key, broken.key);
    assert_eq!(
        snapshot
            .inputs
            .iter()
            .filter(|scope| scope.project.as_str() == "Multi.csproj")
            .count(),
        2,
        "outer and successful inner captures must remain separate"
    );
    assert!(!snapshot.is_complete());
    snapshot.projects[0].standing = DiscoveryStanding::Confirmed;
    assert!(
        !snapshot.is_complete(),
        "a failed unit cannot be hidden by a malformed confirmed project aggregate"
    );
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn containment_rejects_link_escape_but_keeps_inside_twin() {
    let root = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    write(outside.path(), "Escape.cs", "class Escape {}\n");
    write(root.path(), "Inside.cs", "class Inside {}\n");
    write(
        root.path(),
        "Good.csproj",
        &literal("<ItemGroup><Compile Include=\"Inside.cs\" /></ItemGroup>"),
    );
    let escaped = outside
        .path()
        .join("Escape.cs")
        .to_string_lossy()
        .replace('&', "&amp;")
        .replace('"', "&quot;");
    write(
        root.path(),
        "Bad.csproj",
        &literal(&format!(
            "<ItemGroup><Compile Include=\"{escaped}\" /></ItemGroup>"
        )),
    );
    let snapshot = discover(root.path(), options());
    assert_eq!(
        reason(
            &snapshot
                .projects
                .iter()
                .find(|p| p.key.as_str() == "Bad.csproj")
                .unwrap()
                .standing
        ),
        DiscoveryFailureReason::OutsideWorkspaceInput
    );
    assert_eq!(
        snapshot
            .units
            .iter()
            .find(|u| u.project.as_str() == "Good.csproj")
            .unwrap()
            .sources[0]
            .path,
        Path::new("Inside.cs")
    );
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn packages_config_authority_and_host_support() {
    let root = TempDir::new().unwrap();
    write(root.path(), "Legacy.csproj", &literal(""));
    write(
        root.path(),
        "packages.config",
        "<packages><package id=\"Tethys.Missing.Package\" version=\"1.0.0\" targetFramework=\"net48\" /></packages>",
    );
    let snapshot = discover(root.path(), options());
    assert_eq!(
        reason(&snapshot.projects[0].standing),
        DiscoveryFailureReason::RestoreRequired
    );
    if !cfg!(windows) {
        let snapshot = discover(
            root.path(),
            DiscoveryOptions {
                allow_restore: true,
                ..options()
            },
        );
        assert_eq!(
            reason(&snapshot.projects[0].standing),
            DiscoveryFailureReason::RestoreUnsupportedOnHost
        );
    }
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn separately_authorized_offline_restore_failure_is_explicit() {
    let root = TempDir::new().unwrap();
    fs::create_dir(root.path().join("empty-feed")).unwrap();
    write(
        root.path(),
        "NuGet.Config",
        "<configuration><packageSources><clear/><add key=\"offline\" value=\"empty-feed\" /></packageSources></configuration>",
    );
    write(
        root.path(),
        "App.csproj",
        "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup><ItemGroup><PackageReference Include=\"Tethys.Definitely.Absent\" Version=\"0.0.0\" /></ItemGroup></Project>",
    );
    let snapshot = discover(
        root.path(),
        DiscoveryOptions {
            allow_restore: true,
            ..options()
        },
    );
    assert_eq!(
        reason(&snapshot.projects[0].standing),
        DiscoveryFailureReason::RestoreFailed
    );
    assert!(
        snapshot
            .units
            .iter()
            .all(|unit| unit.standing != DiscoveryStanding::Confirmed)
    );
    assert!(snapshot.cache.is_empty());
}

#[cfg(unix)]
#[test]
#[ignore = "requires installed SDK; uses adversarial actor only for deadline control"]
fn evaluator_deadline_kills_silent_actor() {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new().unwrap();
    let companion = TempDir::new().unwrap();
    write(root.path(), "App.csproj", &literal(""));
    let sdk = options().msbuild_path.unwrap();
    write(
        root.path(),
        "global.json",
        &serde_json::json!({"sdk": {"version": sdk.file_name().unwrap().to_str().unwrap(), "rollForward": "disable"}}).to_string(),
    );
    fs::create_dir(companion.path().join("sdk")).unwrap();
    // DOTNET is scoped to the child test process by the caller; use the supported
    // Framework executable actor on Windows in the host runner's platform fence.
    let actor = companion.path().join("dotnet");
    fs::write(
        &actor,
        "#!/bin/sh\ncase \"$1\" in --*) exec \"$TETHYS_REAL_DOTNET\" \"$@\" ;; esac\necho started > \"$TETHYS_ACTOR_MARKER\"\nsleep 30\n",
    )
    .unwrap();
    fs::set_permissions(&actor, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        companion.path().join("sdk/Tethys.MSBuild.Evaluate.dll"),
        "deadline actor payload",
    )
    .unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "deadline_child"])
        .env("DOTNET", &actor)
        .env(
            "TETHYS_REAL_DOTNET",
            std::env::var_os("DOTNET").unwrap_or_else(|| "dotnet".into()),
        )
        .env("TETHYS_ACTOR_MARKER", companion.path().join("started"))
        .env("TETHYS_DEADLINE_ROOT", root.path())
        .env("TETHYS_WORKER_DISTRIBUTION", companion.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(companion.path().join("started").exists());
}

#[cfg(unix)]
#[test]
#[ignore = "subprocess-only deadline fixture"]
fn deadline_child() {
    let Some(root) = std::env::var_os("TETHYS_DEADLINE_ROOT") else {
        return;
    };
    let snapshot = discover(
        Path::new(&root),
        DiscoveryOptions {
            timeout: std::time::Duration::from_secs(2),
            msbuild_path: None,
            ..options()
        },
    );
    assert_eq!(
        reason(&snapshot.projects[0].standing),
        DiscoveryFailureReason::Timeout
    );
}

#[test]
#[ignore = "requires installed net8 targeting pack and packaged real worker"]
fn authorized_restore_rechecks_metadata_before_confirmation() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "App.csproj",
        "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>",
    );
    write(root.path(), "App.cs", "class App {}\n");
    write(
        root.path(),
        "NuGet.Config",
        "<configuration><packageSources><clear/></packageSources></configuration>",
    );
    let ungranted = discover(root.path(), options());
    assert_eq!(
        reason(&ungranted.projects[0].standing),
        DiscoveryFailureReason::RestoreRequired
    );
    assert!(!root.path().join("obj/project.assets.json").exists());
    let snapshot = discover(
        root.path(),
        DiscoveryOptions {
            allow_restore: true,
            ..options()
        },
    );
    assert_eq!(
        snapshot.projects[0].standing,
        DiscoveryStanding::Confirmed,
        "{:?}",
        snapshot.projects
    );
    assert_eq!(snapshot.units[0].standing, DiscoveryStanding::Confirmed);
    assert_eq!(
        snapshot.units[0]
            .framework
            .as_ref()
            .unwrap()
            .short_name
            .as_deref(),
        Some("net8.0")
    );
    assert_eq!(snapshot.units[0].sources[0].path, Path::new("App.cs"));
    assert_eq!(
        snapshot.units[0].restore.style,
        DiscoveryRestoreStyle::PackageReference
    );
    assert!(root.path().join("obj/project.assets.json").is_file());
    // A context-only project edit can leave restore outputs byte-identical and
    // older after a legitimate no-op restore. Persist validated restore evidence.
    write(
        root.path(),
        "App.csproj",
        "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net8.0</TargetFramework><DefineConstants>AFTER_NOOP_RESTORE</DefineConstants></PropertyGroup></Project>",
    );
    fs::File::options()
        .write(true)
        .open(root.path().join("App.csproj"))
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(2))
        .unwrap();
    let updated = discover(
        root.path(),
        DiscoveryOptions {
            allow_restore: true,
            ..options()
        },
    );
    assert_eq!(
        updated.projects[0].standing,
        DiscoveryStanding::Confirmed,
        "{:?}",
        updated.projects
    );
    assert!(updated.units[0].properties["DefineConstants"].contains("AFTER_NOOP_RESTORE"));
    let request = DiscoveryRequest::new(root.path(), options())
        .unwrap()
        .with_cache(updated.cache.clone());
    let granted_once = MsBuildDiscovery.discover(&request).unwrap();
    assert_eq!(granted_once.units, updated.units);
    let forced = MsBuildDiscovery
        .discover(
            &DiscoveryRequest::new(
                root.path(),
                DiscoveryOptions {
                    cache_policy: DiscoveryCachePolicy::Disabled,
                    ..options()
                },
            )
            .unwrap()
            .with_cache(updated.cache),
        )
        .unwrap();
    assert_eq!(forced.units, updated.units);
}

#[test]
#[ignore = "requires packaged worker, net8 targeting pack, and its restored Newtonsoft.Json dependency"]
#[expect(
    clippy::too_many_lines,
    reason = "the fixture IS the test: the target-declared download, restore config, ungranted/granted/no-op transition and independent direct-host evaluation are inlined so the authority assertion is self-contained"
)]
fn authorized_restore_corroborates_target_downloads_and_rejects_changed_imports() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "App.csproj",
        r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <RestoreConfigFile>Restore.config</RestoreConfigFile>
  </PropertyGroup>
  <ItemGroup><PackageReference Include="Newtonsoft.Json" /></ItemGroup>
  <Import Project="Downloads.targets" />
</Project>"#,
    );
    write(root.path(), "App.cs", "class App {}\n");
    // Central package management needs a declared PackageVersion for VersionOverride to
    // resolve identically on every supported SDK band; without it SDK 8 ignores the
    // override and restores an older cached package, which fails NU1605.
    write(
        root.path(),
        "Directory.Packages.props",
        "<Project><ItemGroup><PackageVersion Include=\"Newtonsoft.Json\" Version=\"13.0.3\" /></ItemGroup></Project>",
    );
    write(
        root.path(),
        "Restore.config",
        "<configuration><packageSources><clear/></packageSources></configuration>",
    );
    write(
        root.path(),
        "Directory.Packages.props",
        r#"<Project>
  <PropertyGroup><ManagePackageVersionsCentrally>true</ManagePackageVersionsCentrally></PropertyGroup>
  <ItemGroup><PackageVersion Include="Newtonsoft.Json" Version="13.0.3" /></ItemGroup>
</Project>"#,
    );
    // Use the worker's already-restored package, not a feed or fabricated assets.
    // This supported NuGet extension point runs only during ordinary Restore.
    let signals = TempDir::new().unwrap();
    let restore_marker = signals.path().join("restore-ran");
    let noop_marker = signals.path().join("noop-restore-ran");
    let xml_path = |path: &Path| {
        path.to_string_lossy()
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
    };
    let restore_marker_xml = xml_path(&restore_marker);
    let noop_marker_xml = xml_path(&noop_marker);
    let download = r#"<PackageDownload Include="Newtonsoft.Json" Version="[13.0.3.0]" />"#;
    let targets = format!(
        r#"<Project>
  <Target Name="AddQualificationDownload" BeforeTargets="CollectPackageDownloads">
    <ItemGroup>{download}</ItemGroup>
    <WriteLinesToFile File="{restore_marker_xml}" Lines="restore" Overwrite="true" />
  </Target>
</Project>"#
    );
    write(root.path(), "Downloads.targets", &targets);
    let selected = options();
    let dotnet = std::env::var_os("DOTNET").unwrap_or_else(|| "dotnet".into());
    let evaluation = std::process::Command::new(dotnet)
        .arg(selected.msbuild_path.as_ref().unwrap().join("MSBuild.dll"))
        .arg(root.path().join("App.csproj"))
        .args(["-getItem:PackageDownload", "-nologo"])
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(
        evaluation.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&evaluation.stdout),
        String::from_utf8_lossy(&evaluation.stderr)
    );
    let evaluated: serde_json::Value = serde_json::from_slice(&evaluation.stdout).unwrap();
    assert_eq!(evaluated["Items"]["PackageDownload"], serde_json::json!([]));
    let ungranted = discover(root.path(), selected.clone());
    assert_eq!(
        reason(&ungranted.projects[0].standing),
        DiscoveryFailureReason::RestoreRequired
    );
    assert!(!restore_marker.exists());
    assert!(!root.path().join("obj/project.assets.json").exists());

    let restored = discover(
        root.path(),
        DiscoveryOptions {
            allow_restore: true,
            ..selected.clone()
        },
    );
    assert_eq!(
        restored.projects[0].standing,
        DiscoveryStanding::Confirmed,
        "{:?}",
        restored.projects
    );
    assert_eq!(restored.units[0].standing, DiscoveryStanding::Confirmed);
    assert_eq!(restored.units[0].sources[0].path, Path::new("App.cs"));
    assert!(restore_marker.is_file());
    let assets: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join("obj/project.assets.json")).unwrap())
            .unwrap();
    assert!(
        assets["project"]["frameworks"]["net8.0"]["downloadDependencies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|dependency| dependency["name"] == "Newtonsoft.Json"
                && dependency["version"] == "[13.0.3, 13.0.3]"),
        "ordinary native Restore must actually record the target-generated download"
    );
    fs::remove_file(&restore_marker).unwrap();

    // Native artifacts alone do not grant target-derived authority to a fresh
    // caller. Only the genuine earlier snapshot can carry that evidence.
    let no_receipt = discover(root.path(), selected.clone());
    assert_eq!(
        reason(&no_receipt.projects[0].standing),
        DiscoveryFailureReason::RestoreRequired
    );
    let reuse = || {
        MsBuildDiscovery
            .discover(
                &DiscoveryRequest::new(
                    root.path(),
                    DiscoveryOptions {
                        cache_policy: DiscoveryCachePolicy::Disabled,
                        ..selected.clone()
                    },
                )
                .unwrap()
                .with_cache(restored.cache.clone()),
            )
            .unwrap()
    };
    let current = reuse();
    assert_eq!(
        current.projects[0].standing,
        DiscoveryStanding::Confirmed,
        "{:?}",
        current.projects
    );
    assert_eq!(current.units, restored.units);
    assert!(!restore_marker.exists());

    let import = root.path().join("Downloads.targets");
    let modified = fs::metadata(&import).unwrap().modified().unwrap();
    for changed in [
        targets.replace("[13.0.3.0]", "[13.0.4.0]"),
        targets.replace(download, ""),
    ] {
        fs::write(&import, changed).unwrap();
        fs::File::options()
            .write(true)
            .open(&import)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        let stale = reuse();
        assert_eq!(
            reason(&stale.projects[0].standing),
            DiscoveryFailureReason::RestoreRequired,
            "changed or removed target downloads cannot reuse stale native outputs"
        );
        assert!(stale.units.is_empty());
        assert!(!restore_marker.exists());
    }

    // An ordinary Restore target may report success without generating anything.
    // That success cannot corroborate the old download after its removal.
    write(
        root.path(),
        "App.csproj",
        &format!(
            r#"<Project>
  <Import Project="Sdk.props" Sdk="Microsoft.NET.Sdk" />
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <RestoreConfigFile>Restore.config</RestoreConfigFile>
  </PropertyGroup>
  <ItemGroup><PackageReference Include="Newtonsoft.Json" /></ItemGroup>
  <Import Project="Downloads.targets" />
  <Import Project="Sdk.targets" Sdk="Microsoft.NET.Sdk" />
  <Target Name="Restore">
    <WriteLinesToFile File="{noop_marker_xml}" Lines="restore" Overwrite="true" />
  </Target>
</Project>"#
        ),
    );
    let stale = MsBuildDiscovery
        .discover(
            &DiscoveryRequest::new(
                root.path(),
                DiscoveryOptions {
                    allow_restore: true,
                    ..selected
                },
            )
            .unwrap()
            .with_cache(restored.cache),
        )
        .unwrap();
    assert!(noop_marker.is_file());
    assert_eq!(
        reason(&stale.projects[0].standing),
        DiscoveryFailureReason::RestoreFailed,
        "an exit-zero no-op Restore cannot legitimize stale target downloads"
    );
    assert!(stale.units.is_empty());
}

#[test]
#[ignore = "requires packaged worker, net8/netstandard2.1 targeting packs, and restored Newtonsoft.Json"]
#[expect(
    clippy::too_many_lines,
    reason = "the fixture IS the test: the multi-target project, target-assigned version, shared-assets assertion and no-op/cached/forced reuse sequence are inlined so the preserved-assets assertion is self-contained"
)]
fn authorized_restore_preserves_multitarget_assets_with_target_assigned_versions() {
    let root = TempDir::new().unwrap();
    let signals = TempDir::new().unwrap();
    let restore_marker = signals.path().join("restore-ran");
    let noop_marker = signals.path().join("noop-restore-ran");
    let restore_marker_text = restore_marker.to_string_lossy();
    let noop_marker_text = noop_marker.to_string_lossy();
    let restore_marker_xml = quick_xml::escape::escape(restore_marker_text);
    let noop_marker_xml = quick_xml::escape::escape(noop_marker_text);
    let project = r#"<Project>
  <Import Project="Sdk.props" Sdk="Microsoft.NET.Sdk" />
  <PropertyGroup>
    <TargetFrameworks> net8.0 ; netstandard2.1 ; </TargetFrameworks>
    <RestoreConfigFile>Restore.config</RestoreConfigFile>
  </PropertyGroup>
  <ItemGroup><PackageReference Include="Newtonsoft.Json" /></ItemGroup>
  <Import Project="Versions.targets" />
  <Import Project="Sdk.targets" Sdk="Microsoft.NET.Sdk" />
</Project>"#;
    let targets = format!(
        r#"<Project>
  <Target Name="AssignQualificationVersion" BeforeTargets="CollectPackageReferences">
    <ItemGroup><PackageReference Update="Newtonsoft.Json" Version="13.0.3" /></ItemGroup>
    <WriteLinesToFile File="{restore_marker_xml}" Lines="restore" Overwrite="true" />
  </Target>
</Project>"#
    );
    write(root.path(), "App.csproj", project);
    write(root.path(), "Versions.targets", &targets);
    write(root.path(), "App.cs", "class App {}\n");
    write(
        root.path(),
        "Restore.config",
        "<configuration><packageSources><clear/></packageSources></configuration>",
    );
    let selected = options();
    let ungranted = discover(root.path(), selected.clone());
    assert_eq!(
        reason(&ungranted.projects[0].standing),
        DiscoveryFailureReason::RestoreRequired
    );
    assert!(!restore_marker.exists());
    assert!(!root.path().join("obj/project.assets.json").exists());

    let restored = discover(
        root.path(),
        DiscoveryOptions {
            allow_restore: true,
            ..selected.clone()
        },
    );
    assert_eq!(
        restored.projects[0].standing,
        DiscoveryStanding::Confirmed,
        "{:?}",
        restored.projects
    );
    assert_eq!(restored.units.len(), 2, "{:?}", restored.units);
    for framework in ["net8.0", "netstandard2.1"] {
        let unit = restored
            .units
            .iter()
            .find(|unit| unit.target_framework.as_deref() == Some(framework))
            .unwrap();
        assert_eq!(unit.standing, DiscoveryStanding::Confirmed);
        assert_eq!(unit.sources[0].path, Path::new("App.cs"));
    }
    let assets_path = root.path().join("obj/project.assets.json");
    let assets: serde_json::Value =
        serde_json::from_slice(&fs::read(&assets_path).unwrap()).unwrap();
    let frameworks = assets["project"]["frameworks"].as_object().unwrap();
    assert_eq!(
        frameworks.keys().map(String::as_str).collect::<Vec<_>>(),
        ["net8.0", "netstandard2.1"],
        "internal selector restores must not overwrite the shared assets with one framework"
    );
    for framework in ["net8.0", "netstandard2.1"] {
        assert_eq!(
            frameworks[framework]["dependencies"]["Newtonsoft.Json"]["version"], "[13.0.3, )",
            "ordinary Restore must supply the otherwise unavailable version"
        );
    }
    assert!(restore_marker.is_file());
    fs::remove_file(&restore_marker).unwrap();
    let reuse = || {
        MsBuildDiscovery
            .discover(
                &DiscoveryRequest::new(
                    root.path(),
                    DiscoveryOptions {
                        cache_policy: DiscoveryCachePolicy::Disabled,
                        ..selected.clone()
                    },
                )
                .unwrap()
                .with_cache(restored.cache.clone()),
            )
            .unwrap()
    };
    let current = reuse();
    assert_eq!(current.projects[0].standing, DiscoveryStanding::Confirmed);
    assert_eq!(current.units, restored.units);
    assert!(!restore_marker.exists());

    let import = root.path().join("Versions.targets");
    let modified = fs::metadata(&import).unwrap().modified().unwrap();
    fs::write(&import, targets.replace("13.0.3", "13.0.4")).unwrap();
    fs::File::options()
        .write(true)
        .open(&import)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let stale = reuse();
    assert_eq!(
        reason(&stale.projects[0].standing),
        DiscoveryFailureReason::PartialTargetFrameworks
    );
    assert_eq!(stale.units.len(), restored.units.len());
    for unit in &stale.units {
        assert_eq!(
            reason(&unit.standing),
            DiscoveryFailureReason::RestoreRequired
        );
    }
    assert!(!restore_marker.exists());

    let noop = format!(
        r#"  <Target Name="Restore">
    <WriteLinesToFile File="{noop_marker_xml}" Lines="restore" Overwrite="true" />
  </Target>
</Project>"#
    );
    write(
        root.path(),
        "App.csproj",
        &project.replace("</Project>", &noop),
    );
    let stale = MsBuildDiscovery
        .discover(
            &DiscoveryRequest::new(
                root.path(),
                DiscoveryOptions {
                    allow_restore: true,
                    ..selected.clone()
                },
            )
            .unwrap()
            .with_cache(restored.cache),
        )
        .unwrap();
    assert!(noop_marker.is_file());
    assert_eq!(
        reason(&stale.projects[0].standing),
        DiscoveryFailureReason::PartialTargetFrameworks,
        "exit-zero Restore cannot supply an unavailable target-assigned version"
    );
    assert_eq!(stale.units.len(), 2);
    for unit in &stale.units {
        assert_eq!(
            reason(&unit.standing),
            DiscoveryFailureReason::RestoreFailed
        );
    }

    // Unlike internal selectors, an explicit caller global must constrain Restore.
    write(
        root.path(),
        "App.csproj",
        &project.replace(
            "<TargetFrameworks> net8.0 ; netstandard2.1 ; </TargetFrameworks>",
            "<TargetFramework>net8.0</TargetFramework>",
        ),
    );
    write(root.path(), "Versions.targets", &targets);
    let mut explicit = selected;
    explicit.allow_restore = true;
    explicit
        .context
        .global_properties
        .insert("TargetFramework".into(), "netstandard2.1".into());
    let constrained = discover(root.path(), explicit);
    assert_eq!(
        constrained.projects[0].standing,
        DiscoveryStanding::Confirmed,
        "{:?}",
        constrained.projects
    );
    assert_eq!(constrained.units.len(), 1, "{:?}", constrained.units);
    assert_eq!(
        constrained.units[0].target_framework.as_deref(),
        Some("netstandard2.1")
    );
    assert_eq!(constrained.units[0].standing, DiscoveryStanding::Confirmed);
    let assets: serde_json::Value =
        serde_json::from_slice(&fs::read(&assets_path).unwrap()).unwrap();
    assert_eq!(
        assets["project"]["frameworks"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["netstandard2.1"]
    );
}

#[test]
#[ignore = "requires installed net8 targeting pack and packaged real worker"]
fn later_restore_does_not_excuse_an_earlier_glob_change() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "A.csproj",
        &literal("<ItemGroup><Compile Include=\"obj/*\" /></ItemGroup>"),
    );
    write(
        root.path(),
        "Z.csproj",
        "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>",
    );
    write(
        root.path(),
        "NuGet.Config",
        "<configuration><packageSources><clear/></packageSources></configuration>",
    );
    let snapshot = discover(
        root.path(),
        DiscoveryOptions {
            allow_restore: true,
            ..options()
        },
    );
    let earlier = snapshot
        .projects
        .iter()
        .find(|project| project.key.as_str() == "A.csproj")
        .unwrap();
    assert_eq!(
        reason(&earlier.standing),
        DiscoveryFailureReason::EvaluationFailed
    );
    let later = snapshot
        .projects
        .iter()
        .find(|project| project.key.as_str() == "Z.csproj")
        .unwrap();
    assert_eq!(later.standing, DiscoveryStanding::Confirmed);
    let earlier_inputs = snapshot
        .inputs
        .iter()
        .find(|scope| scope.project == earlier.key)
        .unwrap();
    assert!(
        earlier_inputs
            .inputs
            .iter()
            .any(|input| input.canonical_path
                == fs::canonicalize(root.path().join("A.csproj")).unwrap()),
        "final invalidation must retain the earlier scope's known observations"
    );
}

#[test]
#[ignore = "requires packaged worker, net8 targeting pack, and its restored Newtonsoft.Json dependency"]
fn restored_package_metadata_names_are_case_insensitive() {
    let root = TempDir::new().unwrap();
    write(
        root.path(),
        "NuGet.Config",
        "<configuration><packageSources><clear /></packageSources></configuration>",
    );
    write(
        root.path(),
        "App.csproj",
        r#"<Project Sdk="Microsoft.NET.Sdk">
      <PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup>
      <ItemGroup><PackageReference Include="Newtonsoft.Json">
        <version>13.0.3</version><privateassets>all</privateassets>
      </PackageReference></ItemGroup>
    </Project>"#,
    );
    write(root.path(), "App.cs", "public class App {}");
    let snapshot = discover(
        root.path(),
        DiscoveryOptions {
            allow_restore: true,
            ..options()
        },
    );
    assert!(
        matches!(snapshot.projects[0].standing, DiscoveryStanding::Confirmed),
        "{:?}",
        snapshot.projects
    );
    assert!(matches!(
        snapshot.units[0].standing,
        DiscoveryStanding::Confirmed
    ));
    let request = DiscoveryRequest::new(root.path(), options())
        .unwrap()
        .with_cache(snapshot.cache);
    let current = discover_workspace(&request).unwrap();
    assert!(
        matches!(current.projects[0].standing, DiscoveryStanding::Confirmed),
        "{:?}",
        current.projects
    );
}

#[cfg(unix)]
#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn explicit_host_ignores_ambient_dotnet() {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new().unwrap();
    write(root.path(), "App.csproj", &literal(""));
    let actor = root.path().join("wrong-dotnet");
    fs::write(
        &actor,
        "#!/bin/sh\necho launched > \"$TETHYS_WRONG_DOTNET_MARKER\"\nexit 37\n",
    )
    .unwrap();
    fs::set_permissions(&actor, fs::Permissions::from_mode(0o755)).unwrap();
    let marker = root.path().join("wrong-dotnet-launched");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "explicit_host_child"])
        .env("DOTNET", actor)
        .env("TETHYS_EXPLICIT_HOST_ROOT", root.path())
        .env("TETHYS_WRONG_DOTNET_MARKER", &marker)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !marker.exists(),
        "explicit SDK selection launched the ambient muxer"
    );
}

#[cfg(unix)]
#[test]
#[ignore = "subprocess-only explicit-host fixture"]
fn explicit_host_child() {
    let Some(root) = std::env::var_os("TETHYS_EXPLICIT_HOST_ROOT") else {
        return;
    };
    let selected = PathBuf::from(std::env::var_os("TETHYS_SDK_MSBUILD_PATH").unwrap())
        .canonicalize()
        .unwrap();
    let snapshot = discover(Path::new(&root), options());
    assert_eq!(snapshot.projects[0].standing, DiscoveryStanding::Confirmed);
    assert_eq!(snapshot.units[0].host.as_ref().unwrap().path, selected);
}

#[cfg(windows)]
#[test]
#[ignore = "requires actual VS17.14, NuGet6.14 and the prepared worker's offline package fixture"]
fn windows_packages_config_restore_requires_grant_and_confirms_native_inputs() {
    let root = TempDir::new().unwrap();
    let source = std::env::var("TETHYS_NUGET_FIXTURE_SOURCE")
        .expect("select prepared offline package source");
    let source = quick_xml::escape::escape(&source);
    write(
        root.path(),
        "NuGet.Config",
        &format!(
            "<configuration><packageSources><clear /><add key=\"fixture\" value=\"{source}\" /></packageSources><config><add key=\"repositoryPath\" value=\"custom-restored-packages\" /></config></configuration>"
        ),
    );
    write(
        root.path(),
        "Legacy.csproj",
        r#"<Project ToolsVersion="Current" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
      <PropertyGroup><ProjectGuid>{3DA4EC56-508A-4B29-A05C-E42BC22B9462}</ProjectGuid>
        <TargetFrameworkIdentifier>.NETFramework</TargetFrameworkIdentifier><TargetFrameworkVersion>v4.8</TargetFrameworkVersion>
        <OutputType>Library</OutputType><AssemblyName>Legacy</AssemblyName>
      </PropertyGroup>
      <ItemGroup><Compile Include="Legacy.cs" /><None Include="packages.config" />
        <Reference Include="Newtonsoft.Json"><HintPath>custom-restored-packages/Newtonsoft.Json.13.0.3/lib/net45/Newtonsoft.Json.dll</HintPath></Reference>
      </ItemGroup>
      <Import Project="$(MSBuildToolsPath)/Microsoft.CSharp.targets" />
    </Project>"#,
    );
    write(root.path(), "Legacy.cs", "public class Legacy {}");
    write(
        root.path(),
        "Legacy.sln",
        "Microsoft Visual Studio Solution File, Format Version 12.00\nProject(\"{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}\") = \"Legacy\", \"Legacy.csproj\", \"{3DA4EC56-508A-4B29-A05C-E42BC22B9462}\"\nEndProject\nGlobal\nEndGlobal\n",
    );
    write(
        root.path(),
        "packages.config",
        "<packages><package id=\"Newtonsoft.Json\" version=\"13.0.3\" targetFramework=\"net48\" /></packages>",
    );
    let mut selected = options();
    selected.msbuild_path = Some(PathBuf::from(
        std::env::var_os("TETHYS_VS_MSBUILD_PATH").expect("select actual VS17.14"),
    ));
    let missing = discover(root.path(), selected.clone());
    assert_eq!(
        reason(&missing.projects[0].standing),
        DiscoveryFailureReason::RestoreRequired
    );
    assert!(!root.path().join("packages").exists());
    assert!(!root.path().join("custom-restored-packages").exists());
    selected.allow_restore = true;
    let restored = discover(root.path(), selected.clone());
    assert_eq!(restored.projects[0].standing, DiscoveryStanding::Confirmed);
    assert!(
        root.path()
            .join("custom-restored-packages/Newtonsoft.Json.13.0.3/lib/net45/Newtonsoft.Json.dll")
            .is_file()
    );
    assert_eq!(
        restored.units[0].host.as_ref().unwrap().kind,
        EvaluationHostKind::Framework
    );
    assert!(!root.path().join("packages").exists());
    selected.allow_restore = false;
    let request = DiscoveryRequest::new(root.path(), selected)
        .unwrap()
        .with_cache(restored.cache);
    let current = discover_workspace(&request).unwrap();
    assert_eq!(current.projects[0].standing, DiscoveryStanding::Confirmed);
}

#[cfg(windows)]
#[test]
#[ignore = "requires actual VS17.14, NuGet6.14 and the prepared worker's offline package fixture"]
fn windows_filter_solution_default_restore_and_ambiguity() {
    let root = TempDir::new().unwrap();
    let source = std::env::var("TETHYS_NUGET_FIXTURE_SOURCE")
        .expect("select prepared offline package source");
    let source = quick_xml::escape::escape(&source);
    write(
        root.path(),
        "NuGet.Config",
        &format!(
            "<configuration><packageSources><clear /><add key=\"fixture\" value=\"{source}\" /></packageSources><config><clear /></config></configuration>"
        ),
    );
    fs::create_dir_all(root.path().join("obj/solutions")).unwrap();
    fs::create_dir(root.path().join("filters")).unwrap();
    write(
        root.path(),
        "Legacy.csproj",
        r#"<Project ToolsVersion="Current" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
      <PropertyGroup><ProjectGuid>{3DA4EC56-508A-4B29-A05C-E42BC22B9462}</ProjectGuid>
        <TargetFrameworkIdentifier>.NETFramework</TargetFrameworkIdentifier><TargetFrameworkVersion>v4.8</TargetFrameworkVersion>
        <OutputType>Library</OutputType><AssemblyName>Legacy</AssemblyName>
      </PropertyGroup>
      <ItemGroup><Compile Include="Legacy.cs" /><None Include="packages.config" />
        <Reference Include="Newtonsoft.Json"><HintPath>obj/solutions/packages/Newtonsoft.Json.13.0.3/lib/net45/Newtonsoft.Json.dll</HintPath></Reference>
      </ItemGroup>
      <Import Project="$(MSBuildToolsPath)/Microsoft.CSharp.targets" />
    </Project>"#,
    );
    write(root.path(), "Legacy.cs", "public class Legacy {}");
    write(
        root.path(),
        "packages.config",
        "<packages><package id=\"Newtonsoft.Json\" version=\"13.0.3\" targetFramework=\"net48\" /></packages>",
    );
    write(
        root.path(),
        "obj/solutions/Legacy.sln",
        "Microsoft Visual Studio Solution File, Format Version 12.00\nProject(\"{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}\") = \"Legacy\", \"../../Legacy.csproj\", \"{3DA4EC56-508A-4B29-A05C-E42BC22B9462}\"\nEndProject\nGlobal\nEndGlobal\n",
    );
    write(
        root.path(),
        "filters/Legacy.slnf",
        r#"{"solution":{"path":"../obj/solutions/Legacy.sln","projects":["../../Legacy.csproj"]}}"#,
    );
    let mut selected = options();
    selected.msbuild_path = Some(PathBuf::from(
        std::env::var_os("TETHYS_VS_MSBUILD_PATH").expect("select actual VS17.14"),
    ));
    let missing = discover(root.path(), selected.clone());
    assert_eq!(
        reason(&missing.projects[0].standing),
        DiscoveryFailureReason::RestoreRequired
    );
    for path in [
        "packages",
        "filters/packages",
        "obj/packages",
        "obj/solutions/packages",
    ] {
        assert!(!root.path().join(path).exists());
    }
    // Negative control: the previous isolated-project invocation cannot infer
    // the filter's actual solution destination, using these same native tools.
    let old = std::process::Command::new("nuget.exe")
        .arg("restore")
        .arg(root.path().join("Legacy.csproj"))
        .arg("-NonInteractive")
        .arg("-MSBuildPath")
        .arg(selected.msbuild_path.as_ref().unwrap())
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(!old.status.success());
    for path in [
        "packages",
        "filters/packages",
        "obj/packages",
        "obj/solutions/packages",
    ] {
        assert!(!root.path().join(path).exists());
    }
    selected.allow_restore = true;
    let restored = discover(root.path(), selected.clone());
    assert_eq!(restored.projects[0].standing, DiscoveryStanding::Confirmed);
    assert!(
        root.path()
            .join("obj/solutions/packages/Newtonsoft.Json.13.0.3/lib/net45/Newtonsoft.Json.dll")
            .is_file()
    );
    for path in ["packages", "filters/packages", "obj/packages"] {
        assert!(!root.path().join(path).exists());
    }
    selected.allow_restore = false;
    let current = discover_workspace(
        &DiscoveryRequest::new(root.path(), selected.clone())
            .unwrap()
            .with_cache(restored.cache),
    )
    .unwrap();
    assert_eq!(current.projects[0].standing, DiscoveryStanding::Confirmed);

    // A second actual solution directory must not select either package root,
    // even though the first root already contains the required package.
    fs::create_dir(root.path().join("other")).unwrap();
    write(
        root.path(),
        "other/Legacy.sln",
        "Microsoft Visual Studio Solution File, Format Version 12.00\nProject(\"{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}\") = \"Legacy\", \"../Legacy.csproj\", \"{3DA4EC56-508A-4B29-A05C-E42BC22B9462}\"\nEndProject\nGlobal\nEndGlobal\n",
    );
    let ambiguous = discover(root.path(), selected.clone());
    assert_eq!(
        reason(&ambiguous.projects[0].standing),
        DiscoveryFailureReason::RestoreRequired
    );
    selected.allow_restore = true;
    let ambiguous = discover(root.path(), selected);
    assert_eq!(
        reason(&ambiguous.projects[0].standing),
        DiscoveryFailureReason::RestoreFailed
    );
    assert!(!root.path().join("other/packages").exists());
    assert!(!root.path().join("packages").exists());
}

#[cfg(unix)]
#[test]
#[ignore = "requires installed net8 targeting pack and packaged real worker"]
fn existing_restore_requires_same_physical_project() {
    let root = TempDir::new().unwrap();
    let physical = root.path().join("physical");
    let other = root.path().join("other");
    for directory in [&physical, &other] {
        fs::create_dir(directory).unwrap();
        write(
            directory,
            "App.csproj",
            "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>",
        );
        write(directory, "App.cs", "class App {}\n");
        write(
            directory,
            "NuGet.Config",
            "<configuration><packageSources><clear/></packageSources></configuration>",
        );
    }
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(&physical, &alias).unwrap();
    let selected = options();
    let dotnet = std::env::var_os("DOTNET").unwrap_or_else(|| "dotnet".into());
    let msbuild = selected.msbuild_path.as_ref().unwrap().join("MSBuild.dll");
    let restore = |project: &Path| {
        let output = std::process::Command::new(&dotnet)
            .arg(&msbuild)
            .arg(project)
            .args(["-target:Restore", "-nologo"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    };
    restore(&alias.join("App.csproj"));
    let same_project = discover(&physical, selected.clone());
    assert_eq!(
        same_project.projects[0].standing,
        DiscoveryStanding::Confirmed,
        "{:?}",
        same_project.projects
    );

    // A real restore for a different same-named project must not establish
    // currentness for this one, even when all referenced outputs still exist.
    restore(&other.join("App.csproj"));
    fs::copy(
        other.join("obj/project.assets.json"),
        physical.join("obj/project.assets.json"),
    )
    .unwrap();
    let wrong_project = discover(&physical, selected);
    assert_eq!(
        reason(&wrong_project.projects[0].standing),
        DiscoveryFailureReason::RestoreRequired
    );
}

#[cfg(unix)]
#[test]
#[ignore = "requires installed net8 targeting pack and packaged real worker"]
fn authorized_restore_preserves_aliased_artifact_identity() {
    let root = TempDir::new().unwrap();
    let physical = root.path().join("physical");
    fs::create_dir(&physical).unwrap();
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(&physical, &alias).unwrap();
    write(
        &physical,
        "App.csproj",
        "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>",
    );
    write(&physical, "App.cs", "class App {}\n");
    write(
        &physical,
        "NuGet.Config",
        "<configuration><packageSources><clear/></packageSources></configuration>",
    );
    let mut selected = options();
    selected.allow_restore = true;
    selected.context.global_properties.insert(
        "MSBuildProjectExtensionsPath".into(),
        format!("{}/obj/", alias.display()),
    );
    let aliased_restore = discover(&physical, selected);
    assert_eq!(
        aliased_restore.projects[0].standing,
        DiscoveryStanding::Confirmed,
        "{:?}",
        aliased_restore.projects
    );
    assert_eq!(
        aliased_restore.units[0].sources[0].path,
        Path::new("App.cs")
    );
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn malformed_nuget_config_preserves_unrelated_project() {
    let root = TempDir::new().unwrap();
    fs::create_dir(root.path().join("Bad")).unwrap();
    fs::create_dir(root.path().join("Good")).unwrap();
    write(root.path(), "Bad/Bad.csproj", &literal(""));
    write(root.path(), "Good/Good.csproj", &literal(""));
    write(
        root.path(),
        "Bad/packages.config",
        "<packages><package id=\"Example\" version=\"1.0.0\" /></packages>",
    );
    for config in [
        "<configuration><config><add key=\"repositoryPath\" key=\"duplicate\" value=\"packages\" /></config></configuration>".to_owned(),
        format!("<configuration><!--{}--></configuration>", "x".repeat(4 * 1024 * 1024)),
    ] {
        write(root.path(), "Bad/NuGet.Config", &config);
        let snapshot = discover(root.path(), options());
        let bad = snapshot.projects.iter().find(|project| project.key.as_str() == "Bad/Bad.csproj").unwrap();
        let good = snapshot.projects.iter().find(|project| project.key.as_str() == "Good/Good.csproj").unwrap();
        assert_eq!(reason(&bad.standing), DiscoveryFailureReason::MalformedInput);
        assert_eq!(good.standing, DiscoveryStanding::Confirmed);
        assert!(snapshot.units.iter().any(|unit| unit.project == good.key));
    }
}
