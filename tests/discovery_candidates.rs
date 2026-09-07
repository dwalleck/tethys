//! Authored, non-executing candidate cases through the public discovery seam.
use std::{
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;
use tethys::discovery::{
    DiscoveryFailureReason, DiscoveryOptions, DiscoveryRequest, DiscoverySnapshot,
    DiscoveryStanding, discover_workspace,
};

fn put(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn discover(root: &Path) -> DiscoverySnapshot {
    discover_workspace(&DiscoveryRequest::new(root, DiscoveryOptions::default()).unwrap()).unwrap()
}

fn solution(entries: &[&str]) -> String {
    let mut text = "Microsoft Visual Studio Solution File, Format Version 12.00\n".to_owned();
    for entry in entries {
        writeln!(text, "Project(\"{{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}}\") = \"display\", \"{entry}\", \"{{00000000-0000-0000-0000-000000000001}}\"\nEndProject").unwrap();
    }
    text
}

#[test]
fn filters_resolve_from_solution_and_do_not_exclude_standalone_projects() {
    let root = TempDir::new().unwrap();
    for path in [
        "Projects/λ space/A.csproj",
        "Projects/B.csproj",
        "Standalone.csproj",
        "obj/Explicit.csproj",
        "obj/Hidden.csproj",
        ".git/Private.csproj",
        ".rivets/Private.csproj",
    ] {
        put(
            root.path(),
            path,
            "<Project><Target Name=\"Never\"><Error Text=\"Must not execute\" /></Target></Project>",
        );
    }
    put(
        root.path(),
        "Solutions/Main.sln",
        &solution(&[
            "..\\Projects\\λ space\\A.csproj",
            "../Projects/B.csproj",
            "../Projects/λ space/./A.csproj",
            "../obj/Explicit.csproj",
        ]),
    );
    put(
        root.path(),
        "Filters/Nested/Only.slnf",
        r#"{"solution":{"path":"../../Solutions/Main.sln","projects":["../Projects/λ space/A.csproj"]}}"#,
    );
    put(
        root.path(),
        "Xml.slnx",
        "<Solution><Folder Name=\"/Group/\"><Project Path=\"Projects/λ space/A.csproj\" /></Folder></Solution>",
    );
    let snapshot = discover(root.path());
    assert!(snapshot.issues.is_empty(), "{:?}", snapshot.issues);
    assert!(snapshot.units.is_empty());
    let records: Vec<_> = snapshot
        .projects
        .iter()
        .map(|project| (project.key.as_str(), project.containers.clone()))
        .collect();
    assert_eq!(
        records,
        vec![
            (
                "Projects/B.csproj",
                vec![PathBuf::from("Solutions/Main.sln")]
            ),
            (
                "Projects/λ space/A.csproj",
                vec![
                    PathBuf::from("Filters/Nested/Only.slnf"),
                    PathBuf::from("Solutions/Main.sln"),
                    PathBuf::from("Xml.slnx")
                ]
            ),
            ("Standalone.csproj", vec![]),
            (
                "obj/Explicit.csproj",
                vec![PathBuf::from("Solutions/Main.sln")]
            ),
        ]
    );
    assert!(snapshot.projects.iter().all(|project| matches!(&project.standing, DiscoveryStanding::Indeterminate(failure) if failure.reason == DiscoveryFailureReason::TrustRequired)));
}

#[test]
fn empty_workspace_is_distinct_from_malformed_enumeration() {
    let root = TempDir::new().unwrap();
    let empty = discover(root.path());
    assert!(empty.projects.is_empty() && empty.issues.is_empty());
    put(
        root.path(),
        "Broken.sln",
        "Microsoft Visual Studio Solution File, Format Version 12.00\nProject(\"oops\") = no quotes\n",
    );
    put(
        root.path(),
        "Broken.slnx",
        "<Solution><Project /></Solution>",
    );
    put(
        root.path(),
        "Broken.slnf",
        r#"{"solution":{"path":"Missing.sln","projects":[]}}"#,
    );
    let broken = discover(root.path());
    assert!(broken.projects.is_empty());
    assert_eq!(
        broken
            .issues
            .iter()
            .map(|issue| (&issue.path, issue.failure.reason))
            .collect::<Vec<_>>(),
        vec![
            (
                &PathBuf::from("Broken.sln"),
                DiscoveryFailureReason::MalformedInput
            ),
            (
                &PathBuf::from("Broken.slnf"),
                DiscoveryFailureReason::MalformedInput
            ),
            (
                &PathBuf::from("Broken.slnx"),
                DiscoveryFailureReason::MalformedInput
            ),
        ]
    );
}

#[test]
fn missing_directory_empty_and_outside_declarations_are_typed_not_dropped() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("workspace");
    fs::create_dir(&root).unwrap();
    put(
        parent.path(),
        "workspace-sibling/Outside.csproj",
        "<Project />",
    );
    put(&root, "Inside.csproj", "<Project />");
    fs::create_dir(root.join("Directory.csproj")).unwrap();
    put(
        &root,
        "Paths.sln",
        &solution(&[
            "Inside.csproj",
            "Missing.csproj",
            "Directory.csproj",
            "",
            "../workspace-sibling/Outside.csproj",
            "../workspace-sibling/NotHere.csproj",
        ]),
    );
    let snapshot = discover(&root);
    assert_eq!(
        snapshot
            .projects
            .iter()
            .map(|project| project.key.as_str())
            .collect::<Vec<_>>(),
        vec!["Inside.csproj"]
    );
    assert_eq!(
        snapshot
            .issues
            .iter()
            .map(|issue| issue.failure.reason)
            .collect::<Vec<_>>(),
        vec![
            DiscoveryFailureReason::MalformedInput,
            DiscoveryFailureReason::MalformedInput,
            DiscoveryFailureReason::MalformedInput,
            DiscoveryFailureReason::OutsideWorkspaceInput,
            DiscoveryFailureReason::OutsideWorkspaceInput
        ]
    );
}

#[test]
fn filter_requires_exact_solution_membership_and_valid_json() {
    let root = TempDir::new().unwrap();
    put(root.path(), "A.csproj", "<Project />");
    put(root.path(), "B.csproj", "<Project />");
    put(root.path(), "Main.sln", &solution(&["A.csproj"]));
    put(
        root.path(),
        "Wrong.slnf",
        r#"{"solution":{"path":"Main.sln","projects":["B.csproj"]}}"#,
    );
    put(
        root.path(),
        "Invalid.slnf",
        r#"{"solution":{"path":"Main.sln","projects":null}}"#,
    );
    let snapshot = discover(root.path());
    assert_eq!(snapshot.issues.len(), 2);
    assert!(
        snapshot
            .issues
            .iter()
            .all(|issue| issue.failure.reason == DiscoveryFailureReason::MalformedInput)
    );
    assert_eq!(
        snapshot.projects[0].containers,
        vec![PathBuf::from("Main.sln")]
    );
    assert!(snapshot.projects[1].containers.is_empty());
}

#[test]
fn xml_escapes_decode_but_external_entities_and_broken_roots_are_rejected() {
    let root = TempDir::new().unwrap();
    put(root.path(), "A & B.csproj", "<Project />");
    put(
        root.path(),
        "Good.slnx",
        "<Solution><Project Path=\"A &amp; B.csproj\" /></Solution>",
    );
    put(
        root.path(),
        "Entity.slnx",
        "<!DOCTYPE Solution [<!ENTITY leak SYSTEM 'file:///etc/passwd'>]><Solution><Project Path=\"&leak;\" /></Solution>",
    );
    put(
        root.path(),
        "Broken.slnx",
        "<Solution><Project Path=\"A &amp; B.csproj\" /></Wrong>",
    );
    put(root.path(), "Multiple.slnx", "<Solution/><Solution/>");
    let snapshot = discover(root.path());
    assert_eq!(snapshot.projects[0].key.as_str(), "A & B.csproj");
    assert_eq!(
        snapshot.projects[0].containers,
        vec![PathBuf::from("Good.slnx")]
    );
    assert_eq!(snapshot.issues.len(), 3);
    assert!(
        snapshot
            .issues
            .iter()
            .all(|issue| issue.failure.reason == DiscoveryFailureReason::MalformedInput)
    );
}

#[test]
fn xml_attribute_whitespace_preserves_distinct_paths_and_character_references() {
    let root = TempDir::new().unwrap();
    put(root.path(), "A B.csproj", "<Project />");
    // Tabs are valid Unix file names but invalid Windows file names.
    #[cfg(unix)]
    put(root.path(), "A\tB.csproj", "<Project />");
    put(
        root.path(),
        "Distinct.slnx",
        "<Solution><Project Path=\"A B.csproj\"/><Project Path=\"A\tB.csproj\"/></Solution>",
    );
    put(
        root.path(),
        "Reference.slnx",
        "<Solution><Project Path=\"A&#x9;B.csproj\"/></Solution>",
    );
    let snapshot = discover(root.path());
    let projects: Vec<_> = snapshot
        .projects
        .iter()
        .map(|project| (project.key.as_str(), project.containers.clone()))
        .collect();
    let mut expected = vec![("A B.csproj", vec![PathBuf::from("Distinct.slnx")])];
    if cfg!(unix) {
        expected.insert(
            0,
            (
                "A\tB.csproj",
                vec![
                    PathBuf::from("Distinct.slnx"),
                    PathBuf::from("Reference.slnx"),
                ],
            ),
        );
    }
    assert_eq!(projects, expected);
    #[cfg(windows)]
    assert_eq!(
        snapshot
            .issues
            .iter()
            .map(|issue| (&issue.path, issue.failure.reason))
            .collect::<Vec<_>>(),
        vec![
            (
                &PathBuf::from("Distinct.slnx"),
                DiscoveryFailureReason::MalformedInput
            ),
            (
                &PathBuf::from("Reference.slnx"),
                DiscoveryFailureReason::MalformedInput
            ),
        ],
        "invalid tab paths must not silently bind to the existing space-named project"
    );
}

#[cfg(unix)]
#[test]
fn symlink_aliases_deduplicate_and_escapes_and_loops_do_not_get_followed() {
    use std::os::unix::fs::symlink;
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("workspace");
    fs::create_dir(&root).unwrap();
    put(&root, "Real/Inside.csproj", "<Project />");
    put(
        parent.path(),
        "workspace-other/Outside.csproj",
        "<Project />",
    );
    symlink(root.join("Real/Inside.csproj"), root.join("Alias.csproj")).unwrap();
    symlink(&root, root.join("Real/Loop")).unwrap();
    symlink(parent.path().join("workspace-other"), root.join("Escape")).unwrap();
    symlink(
        parent.path().join("workspace-other/Outside.csproj"),
        root.join("Outside.csproj"),
    )
    .unwrap();
    symlink(root.join("Real/Missing.csproj"), root.join("Broken.csproj")).unwrap();
    put(
        &root,
        "Links.slnx",
        "<Solution><Project Path=\"Alias.csproj\"/><Project Path=\"Escape/Outside.csproj\"/><Project Path=\"Escape/Missing.csproj\"/></Solution>",
    );
    let snapshot = discover(&root);
    assert_eq!(
        snapshot
            .projects
            .iter()
            .map(|project| project.key.as_str())
            .collect::<Vec<_>>(),
        vec!["Real/Inside.csproj"]
    );
    assert_eq!(
        snapshot.projects[0].containers,
        vec![PathBuf::from("Links.slnx")]
    );
    assert_eq!(snapshot.issues.len(), 4);
    assert_eq!(
        snapshot
            .issues
            .iter()
            .filter(|issue| issue.failure.reason == DiscoveryFailureReason::OutsideWorkspaceInput)
            .count(),
        3
    );
    for (path, reason) in [
        (
            "Outside.csproj",
            DiscoveryFailureReason::OutsideWorkspaceInput,
        ),
        ("Broken.csproj", DiscoveryFailureReason::MalformedInput),
    ] {
        assert!(
            snapshot
                .issues
                .iter()
                .any(|issue| { issue.path == Path::new(path) && issue.failure.reason == reason })
        );
    }
}

#[test]
fn directory_exclusions_follow_platform_case_semantics() {
    let root = TempDir::new().unwrap();
    for directory in ["OBJ", "bIn", "TaRgEt", ".GIT", ".RIVETS"] {
        put(
            root.path(),
            &format!("{directory}/Hidden.csproj"),
            "<Project />",
        );
    }
    put(root.path(), "OBJ/Allowed.csproj", "<Project />");
    put(root.path(), "src/Visible.csproj", "<Project />");
    put(
        root.path(),
        "Explicit.slnx",
        "<Solution><Project Path=\"OBJ/Allowed.csproj\" /></Solution>",
    );
    let snapshot = discover(root.path());
    assert!(snapshot.issues.is_empty());
    let keys = snapshot
        .projects
        .iter()
        .map(|project| project.key.as_str())
        .collect::<Vec<_>>();
    if root.path().join("obj").is_dir() {
        assert_eq!(keys, vec!["OBJ/Allowed.csproj", "src/Visible.csproj"]);
    } else {
        assert_eq!(
            keys,
            vec![
                ".GIT/Hidden.csproj",
                ".RIVETS/Hidden.csproj",
                "OBJ/Allowed.csproj",
                "OBJ/Hidden.csproj",
                "TaRgEt/Hidden.csproj",
                "bIn/Hidden.csproj",
                "src/Visible.csproj"
            ]
        );
    }
    #[cfg(unix)]
    if !root.path().join("obj").exists() {
        for (alias, actual) in [
            ("obj", "OBJ"),
            ("bin", "bIn"),
            ("target", "TaRgEt"),
            (".git", ".GIT"),
            (".rivets", ".RIVETS"),
        ] {
            std::os::unix::fs::symlink(actual, root.path().join(alias)).unwrap();
        }
        let aliased = discover(root.path());
        assert!(aliased.issues.is_empty());
        assert_eq!(
            aliased
                .projects
                .iter()
                .map(|project| project.key.as_str())
                .collect::<Vec<_>>(),
            vec!["OBJ/Allowed.csproj", "src/Visible.csproj"]
        );
    }
}

#[cfg(unix)]
#[test]
fn aliases_cannot_make_generated_or_internal_projects_automatic() {
    let root = TempDir::new().unwrap();
    for (physical, alias) in [
        ("obj", "object-files"),
        ("bin", "binaries"),
        ("target", "cargo-output"),
        (".git", "git-mirror"),
        (".rivets", "tracker-mirror"),
        ("src/obj/deep", "nested-generated"),
    ] {
        put(
            root.path(),
            &format!("{physical}/Hidden.csproj"),
            "<Project />",
        );
        std::os::unix::fs::symlink(physical, root.path().join(alias)).unwrap();
        std::os::unix::fs::symlink(
            format!("{physical}/Hidden.csproj"),
            root.path().join(format!("{alias}.csproj")),
        )
        .unwrap();
    }
    put(root.path(), "obj/Allowed.csproj", "<Project />");
    put(root.path(), "src/Visible.csproj", "<Project />");
    put(
        root.path(),
        "Explicit.slnx",
        "<Solution><Project Path=\"obj/Allowed.csproj\" /></Solution>",
    );
    let snapshot = discover(root.path());
    assert!(snapshot.issues.is_empty());
    assert_eq!(
        snapshot
            .projects
            .iter()
            .map(|project| project.key.as_str())
            .collect::<Vec<_>>(),
        vec!["obj/Allowed.csproj", "src/Visible.csproj"]
    );
}
