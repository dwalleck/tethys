//! Closed-recipe cache behavior through the public adapter and real managed worker.
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};
use tempfile::TempDir;
use tethys::discovery::*;

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
fn project(extra: &str) -> String {
    format!(
        "<Project><PropertyGroup><TargetFrameworkIdentifier>.NETFramework</TargetFrameworkIdentifier><TargetFrameworkVersion>v4.8</TargetFrameworkVersion><TargetFrameworkProfile>Client</TargetFrameworkProfile><AssemblyName>Collision</AssemblyName></PropertyGroup><ItemGroup><Compile Include=\"src/*.cs\"><Link>Shared.cs</Link></Compile><ProjectReference Include=\"Missing.csproj\"><ReferenceOutputAssembly>false</ReferenceOutputAssembly></ProjectReference><Reference Include=\"Example\"><HintPath>lib/Example.dll</HintPath></Reference></ItemGroup>{extra}</Project>"
    )
}
fn fixture() -> TempDir {
    let root = TempDir::new().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/First.cs"), "class First {}\n").unwrap();
    fs::write(root.path().join("App.csproj"), project("")).unwrap();
    root
}
fn discover(
    root: &Path,
    options: DiscoveryOptions,
    entries: Vec<EvaluationCacheEntry>,
) -> DiscoverySnapshot {
    MsBuildDiscovery
        .discover(
            &DiscoveryRequest::new(root, options)
                .unwrap()
                .with_cache(entries),
        )
        .unwrap()
}
fn semantic_eq(left: &DiscoverySnapshot, right: &DiscoverySnapshot) {
    assert_eq!(left.context, right.context);
    assert_eq!(left.projects, right.projects);
    assert_eq!(left.units, right.units);
    assert_eq!(left.issues, right.issues);
}
fn assert_authored(snapshot: &DiscoverySnapshot, names: &[&str]) {
    assert_eq!(snapshot.projects[0].standing, DiscoveryStanding::Confirmed);
    let unit = &snapshot.units[0];
    assert_eq!(unit.standing, DiscoveryStanding::Confirmed);
    let framework = unit.framework.as_ref().unwrap();
    assert_eq!(framework.short_name, None);
    assert_eq!(framework.identifier, ".NETFramework");
    assert_eq!(framework.version, "v4.8");
    assert_eq!(framework.profile, "Client");
    let paths: Vec<_> = unit
        .sources
        .iter()
        .map(|source| source.path.to_str().unwrap().replace('\\', "/"))
        .collect();
    assert_eq!(paths, names);
    assert_eq!(unit.project_references[0].target.as_str(), "Missing.csproj");
    assert_eq!(
        unit.project_references[0].metadata["ReferenceOutputAssembly"],
        "false"
    );
    assert_eq!(
        unit.assembly_references[0].metadata["HintPath"],
        "lib/Example.dll"
    );
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn cache_input_closure_matches_forced_and_invalidates_globs_content_context() {
    let root = fixture();
    let fresh = discover(root.path(), options(), vec![]);
    assert_authored(&fresh, &["src/First.cs"]);
    assert!(
        !fresh.cache.is_empty(),
        "qualified literal/glob recipe must be reusable: {:?}",
        fresh.cache_observations
    );
    let hit = discover(root.path(), options(), fresh.cache.clone());
    assert!(
        hit.cache_observations
            .iter()
            .all(|observation| observation.reused)
    );
    semantic_eq(&fresh, &hit);
    let forced = discover(
        root.path(),
        DiscoveryOptions {
            cache_policy: DiscoveryCachePolicy::Disabled,
            ..options()
        },
        fresh.cache.clone(),
    );
    semantic_eq(&hit, &forced);
    assert!(forced.cache.is_empty());
    fs::write(root.path().join("src/Second.cs"), "class Second {}\n").unwrap();
    let changed = discover(root.path(), options(), hit.cache);
    assert_authored(&changed, &["src/First.cs", "src/Second.cs"]);
    assert!(
        changed
            .cache_observations
            .iter()
            .all(|observation| !observation.reused)
    );
    fs::remove_file(root.path().join("src/First.cs")).unwrap();
    let deleted = discover(root.path(), options(), changed.cache);
    assert_authored(&deleted, &["src/Second.cs"]);
    fs::write(
        root.path().join("App.csproj"),
        project("<PropertyGroup><DefineConstants>CHANGED</DefineConstants></PropertyGroup>"),
    )
    .unwrap();
    let changed_project = discover(root.path(), options(), deleted.cache);
    assert_eq!(
        changed_project.units[0].properties["DefineConstants"],
        "CHANGED"
    );
    assert_eq!(
        fresh.units[0].key, changed_project.units[0].key,
        "assembly/compiler metadata is not context identity"
    );
    let context = EvaluationContext {
        configuration: Some("Release".into()),
        ..EvaluationContext::default()
    };
    let changed_context = discover(
        root.path(),
        DiscoveryOptions {
            context,
            ..options()
        },
        changed_project.cache,
    );
    assert_eq!(
        changed_context.units[0].properties["Configuration"],
        "Release"
    );
    assert_ne!(changed_context.units[0].key, fresh.units[0].key);
    assert!(
        changed_context
            .cache_observations
            .iter()
            .all(|observation| !observation.reused)
    );
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn corrupt_unknown_and_import_recipes_never_reuse_success() {
    let root = fixture();
    let fresh = discover(root.path(), options(), vec![]);
    let mut serialized = serde_json::to_value(&fresh.cache).unwrap();
    serialized[0]["payload"] = serde_json::Value::String("not a receipt".into());
    let corrupt = discover(
        root.path(),
        options(),
        serde_json::from_value(serialized).unwrap(),
    );
    assert_authored(&corrupt, &["src/First.cs"]);
    assert!(
        corrupt
            .cache_observations
            .iter()
            .all(|observation| !observation.reused)
    );
    let mut serialized = serde_json::to_value(&fresh.cache).unwrap();
    let mut payload: serde_json::Value =
        serde_json::from_str(serialized[0]["payload"].as_str().unwrap()).unwrap();
    payload["recipe"] = serde_json::json!(999);
    serialized[0]["payload"] = serde_json::Value::String(serde_json::to_string(&payload).unwrap());
    let unknown = discover(
        root.path(),
        options(),
        serde_json::from_value(serialized).unwrap(),
    );
    assert!(
        unknown
            .cache_observations
            .iter()
            .all(|observation| !observation.reused)
    );
    fs::write(root.path().join("Imported.props"), "<Project><PropertyGroup><DefineConstants>IMPORTED</DefineConstants></PropertyGroup></Project>").unwrap();
    fs::write(
        root.path().join("App.csproj"),
        project("<Import Project=\"Imported.props\" />"),
    )
    .unwrap();
    let imported = discover(root.path(), options(), fresh.cache);
    assert_eq!(imported.units[0].properties["DefineConstants"], "IMPORTED");
    assert!(imported.cache.is_empty());
    fs::write(root.path().join("Imported.props"), "<Project><PropertyGroup><DefineConstants>REPLACED</DefineConstants></PropertyGroup></Project>").unwrap();
    let replaced = discover(root.path(), options(), imported.cache);
    assert_eq!(replaced.units[0].properties["DefineConstants"], "REPLACED");
    fs::write(root.path().join("App.csproj"), project("<PropertyGroup><DefineConstants>$([System.String]::Copy('FUNCTION'))</DefineConstants></PropertyGroup>")).unwrap();
    let unknown_function = discover(root.path(), options(), replaced.cache);
    assert_eq!(
        unknown_function.units[0].properties["DefineConstants"],
        "FUNCTION"
    );
    assert!(unknown_function.cache.is_empty());
    assert!(
        unknown_function.cache_observations[0]
            .bypass_reasons
            .iter()
            .any(|reason| reason == "unqualified_expression_dependencies")
    );
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn restore_and_missing_host_cannot_republish_cached_success() {
    let root = fixture();
    let fresh = discover(root.path(), options(), vec![]);
    fs::write(root.path().join("packages.config"), "<packages><package id=\"Tethys.Missing\" version=\"1.0.0\" targetFramework=\"net48\" /></packages>").unwrap();
    let restore = discover(root.path(), options(), fresh.cache.clone());
    assert!(
        matches!(&restore.projects[0].standing, DiscoveryStanding::Indeterminate(failure) if failure.reason == DiscoveryFailureReason::RestoreRequired)
    );
    assert!(restore.cache.is_empty());
    fs::remove_file(root.path().join("packages.config")).unwrap();
    let unavailable = discover(
        root.path(),
        DiscoveryOptions {
            msbuild_path: Some(root.path().join("missing-host")),
            ..options()
        },
        fresh.cache,
    );
    assert!(
        matches!(&unavailable.projects[0].standing, DiscoveryStanding::Indeterminate(failure) if failure.reason == DiscoveryFailureReason::ToolchainUnavailable)
    );
    assert!(unavailable.cache.is_empty());
    assert!(unavailable.units.is_empty());
}

#[test]
#[ignore = "requires installed net8 targeting pack and packaged real worker"]
fn disabled_restore_noop_keeps_units_without_cache_publication() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("App.csproj"),
        "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>",
    )
    .unwrap();
    fs::write(root.path().join("App.cs"), "class App {}\n").unwrap();
    fs::write(
        root.path().join("NuGet.Config"),
        "<configuration><packageSources><clear /></packageSources></configuration>",
    )
    .unwrap();

    let restored = discover(
        root.path(),
        DiscoveryOptions {
            allow_restore: true,
            ..options()
        },
        vec![],
    );
    assert_eq!(restored.projects[0].standing, DiscoveryStanding::Confirmed);
    assert!(!restored.units.is_empty());
    assert!(
        !restored.cache.is_empty(),
        "restore-backed run must publish evidence"
    );

    let disabled = discover(
        root.path(),
        DiscoveryOptions {
            allow_restore: true,
            cache_policy: DiscoveryCachePolicy::Disabled,
            ..options()
        },
        restored.cache,
    );
    assert_eq!(disabled.projects, restored.projects);
    assert_eq!(disabled.units, restored.units);
    assert!(disabled.cache.is_empty());
}

#[test]
#[ignore = "requires installed net8 targeting pack and packaged real worker"]
fn restore_receipt_property_names_ignore_case_but_values_do_not() {
    let root = TempDir::new().unwrap();
    let project = root.path().join("App.csproj");
    fs::write(&project, "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>").unwrap();
    fs::write(root.path().join("App.cs"), "class App {}\n").unwrap();
    fs::write(
        root.path().join("NuGet.Config"),
        "<configuration><packageSources><clear /></packageSources></configuration>",
    )
    .unwrap();
    fs::File::options()
        .write(true)
        .open(&project)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + Duration::from_secs(60))
        .unwrap();
    let mut selected = options();
    selected.allow_restore = true;
    selected
        .context
        .global_properties
        .insert("DefineConstants".to_owned(), "STABLE".to_owned());
    let restored = discover(root.path(), selected, vec![]);
    assert_eq!(restored.projects[0].standing, DiscoveryStanding::Confirmed);
    let mut recased = options();
    recased.cache_policy = DiscoveryCachePolicy::Disabled;
    recased
        .context
        .global_properties
        .insert("defineconstants".to_owned(), "STABLE".to_owned());
    let reused = discover(root.path(), recased.clone(), restored.cache.clone());
    assert_eq!(reused.projects[0].standing, DiscoveryStanding::Confirmed);
    assert_eq!(reused.units[0].key, restored.units[0].key);
    assert!(reused.cache.is_empty());
    recased
        .context
        .global_properties
        .insert("defineconstants".to_owned(), "DIFFERENT".to_owned());
    let changed = discover(root.path(), recased, restored.cache);
    assert!(matches!(
        changed.projects[0].standing,
        DiscoveryStanding::Indeterminate(DiscoveryFailure {
            reason: DiscoveryFailureReason::RestoreRequired,
            ..
        })
    ));
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn eligible_literal_with_preepoch_source_serializes_and_invalidates_content() {
    let root = fixture();
    let source = root.path().join("src/First.cs");
    let pre_epoch = UNIX_EPOCH.checked_sub(Duration::new(1, 1)).unwrap();
    fs::File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_modified(pre_epoch)
        .unwrap();

    let fresh = discover(root.path(), options(), vec![]);
    assert_authored(&fresh, &["src/First.cs"]);
    assert!(
        !fresh.cache.is_empty(),
        "eligible literal must publish evidence: {:?}",
        fresh.cache_observations
    );

    let hit = discover(root.path(), options(), fresh.cache.clone());
    assert_authored(&hit, &["src/First.cs"]);
    assert!(
        hit.cache_observations
            .iter()
            .all(|observation| observation.reused)
    );
    semantic_eq(&fresh, &hit);

    fs::write(&source, "class Other {}\n").unwrap();
    fs::File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_modified(pre_epoch)
        .unwrap();
    let changed = discover(root.path(), options(), hit.cache);
    assert_authored(&changed, &["src/First.cs"]);
    assert!(
        changed
            .cache_observations
            .iter()
            .all(|observation| !observation.reused),
        "same-size content mutation with a preserved pre-epoch mtime must invalidate"
    );
}

fn harness_free(command: &mut std::process::Command) -> &mut std::process::Command {
    // Remove test-tool loader search paths from harness children only; the
    // product must not silently exempt caller-supplied runtime settings.
    #[cfg(unix)]
    command
        .env_remove("LD_PRELOAD")
        .env_remove("LD_AUDIT")
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("DYLD_INSERT_LIBRARIES")
        .env_remove("DYLD_LIBRARY_PATH")
        .env_remove("DYLD_FALLBACK_LIBRARY_PATH")
        .env_remove("DYLD_FRAMEWORK_PATH")
        .env_remove("DYLD_FALLBACK_FRAMEWORK_PATH")
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH");
    command
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn shared_physical_memberships_and_case_insensitive_global_precedence() {
    let root = fixture();
    fs::write(root.path().join("Other.csproj"), project("")).unwrap();
    let mut context = EvaluationContext {
        configuration: Some("Debug".into()),
        ..EvaluationContext::default()
    };
    context
        .global_properties
        .insert("configuration".into(), "Release".into());
    let snapshot = discover(
        root.path(),
        DiscoveryOptions {
            context: context.clone(),
            ..options()
        },
        vec![],
    );
    assert_eq!(snapshot.units.len(), 2);
    assert_ne!(snapshot.units[0].key, snapshot.units[1].key);
    assert_eq!(
        snapshot.units[0].sources[0].path,
        snapshot.units[1].sources[0].path
    );
    assert_eq!(snapshot.units[0].properties["Configuration"], "Release");
    context.global_properties.clear();
    context
        .global_properties
        .insert("Configuration".into(), "Release".into());
    let recased = discover(
        root.path(),
        DiscoveryOptions {
            context,
            ..options()
        },
        snapshot.cache,
    );
    assert_eq!(snapshot.units[0].key, recased.units[0].key);
    assert!(
        recased
            .cache_observations
            .iter()
            .all(|observation| observation.reused)
    );
}

#[test]
#[ignore = "requires packaged real worker and explicitly selected installed SDK"]
fn native_metadata_names_are_case_insensitive_without_rewriting_spelling() {
    let root = fixture();
    fs::write(
        root.path().join("App.csproj"),
        project("")
            .replace("<Link>", "<lInK>")
            .replace("</Link>", "</lInK>"),
    )
    .unwrap();
    let snapshot = discover(root.path(), options(), vec![]);
    assert!(matches!(
        snapshot.projects[0].standing,
        DiscoveryStanding::Confirmed
    ));
    let source = &snapshot.units[0].sources[0];
    assert_eq!(source.link.as_deref(), Some("Shared.cs"));
    assert_eq!(source.metadata["lInK"], "Shared.cs");
}

#[test]
#[ignore = "requires real SDK worker; native host tracing independently records evaluator launches"]
fn eligible_hit_launches_zero_evaluators() {
    let trace_root = TempDir::new().unwrap();
    let log = trace_root.path().join("host.trace");
    let output = harness_free(&mut std::process::Command::new(
        std::env::current_exe().unwrap(),
    ))
    .args(["--ignored", "--exact", "cache_launch_child"])
    .env("TETHYS_CACHE_LAUNCH_CHILD", "1")
    .env("COREHOST_TRACE", "1")
    .env("COREHOST_TRACE_VERBOSITY", "4")
    .env("COREHOST_TRACEFILE", &log)
    .env("DOTNET_HOST_TRACE", "1")
    .env("DOTNET_HOST_TRACE_VERBOSITY", "4")
    .env("DOTNET_HOST_TRACEFILE", &log)
    .output()
    .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "subprocess-only cache-launch fence"]
fn cache_launch_child() {
    if std::env::var_os("TETHYS_CACHE_LAUNCH_CHILD").is_none() {
        return;
    }
    let log = PathBuf::from(std::env::var_os("COREHOST_TRACEFILE").unwrap());
    let root = fixture();
    let selected = options();
    let sdk = selected.msbuild_path.as_ref().unwrap();
    fs::write(
        root.path().join("global.json"),
        serde_json::json!({"sdk": {"version": sdk.file_name().unwrap().to_str().unwrap(), "rollForward": "disable"}}).to_string(),
    ).unwrap();
    // trace.cpp opens in append mode; remove the prior invocation's file rather
    // than comparing lengths or cumulative counts. The environment stays fixed.
    let reset_trace = || match fs::remove_file(&log) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("cannot reset native trace: {error}"),
    };
    let read_trace = || match fs::read_to_string(&log) {
        Ok(trace) => trace,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => panic!("cannot read native trace: {error}"),
    };
    // hostpolicy.cpp emits this immediately before execute_assembly, unlike
    // argument/dependency mentions of the DLL or ordinary dotnet --version probes.
    let worker_launches = |trace: &str| {
        trace
            .lines()
            .filter(|line| {
                line.starts_with("Launch host: ")
                    && line.contains("Tethys.MSBuild.Evaluate.dll, argc: ")
            })
            .count()
    };
    reset_trace();
    let untrusted = discover(
        root.path(),
        DiscoveryOptions {
            trust_msbuild: false,
            ..selected.clone()
        },
        vec![],
    );
    assert!(untrusted.units.is_empty());
    assert!(
        !log.exists(),
        "untrusted discovery must leave no native process evidence"
    );
    reset_trace();
    let fresh = discover(root.path(), selected.clone(), vec![]);
    assert_authored(&fresh, &["src/First.cs"]);
    assert!(
        worker_launches(&read_trace()) > 0,
        "fresh control must launch the real worker"
    );
    reset_trace();
    let hit = discover(root.path(), selected.clone(), fresh.cache.clone());
    let hit_trace = read_trace();
    assert_eq!(
        worker_launches(&hit_trace),
        0,
        "eligible hit must avoid evaluator launch"
    );
    assert!(
        !hit_trace.contains("Tethys.MSBuild.Evaluate.dll"),
        "eligible hit must not even attempt worker startup"
    );
    semantic_eq(&fresh, &hit);
    reset_trace();
    let forced = discover(
        root.path(),
        DiscoveryOptions {
            cache_policy: DiscoveryCachePolicy::Disabled,
            ..selected
        },
        hit.cache.clone(),
    );
    assert!(
        worker_launches(&read_trace()) > 0,
        "forced control must launch the real worker"
    );
    semantic_eq(&hit, &forced);
}

#[test]
#[ignore = "requires real worker; process-local environment avoids unsafe global test mutation"]
fn environment_change_invalidates_eligible_receipt() {
    let root = fixture();
    let state = TempDir::new().unwrap();
    let state_path = state.path().join("receipt.json");
    fs::write(&state_path, "{\"cache\":[]}").unwrap();
    let run = |value: &str| {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        harness_free(&mut command);
        let output = command
            .args(["--ignored", "--exact", "environment_child"])
            .env("TETHYS_ENV_ROOT", root.path())
            .env("TETHYS_ENV_STATE", &state_path)
            .env("DefineConstants", value)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&fs::read(&state_path).unwrap()).unwrap()
    };
    let first = run("ENV_ONE");
    assert_eq!(
        first["units"][0]["properties"]["DefineConstants"],
        "ENV_ONE"
    );
    let hit = run("ENV_ONE");
    assert_eq!(hit["observations"][0]["reused"], true);
    assert_eq!(first["units"], hit["units"]);
    let changed = run("ENV_TWO");
    assert_eq!(changed["observations"][0]["reused"], false);
    assert_eq!(
        changed["units"][0]["properties"]["DefineConstants"],
        "ENV_TWO"
    );
}

#[test]
#[ignore = "subprocess-only environment fence"]
fn environment_child() {
    let Some(root) = std::env::var_os("TETHYS_ENV_ROOT") else {
        return;
    };
    let state_path = PathBuf::from(std::env::var_os("TETHYS_ENV_STATE").unwrap());
    let state: serde_json::Value = serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
    let snapshot = discover(
        Path::new(&root),
        options(),
        serde_json::from_value(state["cache"].clone()).unwrap(),
    );
    fs::write(state_path, serde_json::to_vec(&serde_json::json!({
        "cache": snapshot.cache, "units": snapshot.units, "observations": snapshot.cache_observations
    })).unwrap()).unwrap();
}
