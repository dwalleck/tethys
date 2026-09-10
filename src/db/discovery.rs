//! Active discovery metadata, published with syntax under the revision transaction.

use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::{Connection, params};
use serde::{Serialize, de::DeserializeOwned};

use super::Index;
use crate::discovery::{
    DeclaredAssemblyReference, DeclaredProjectReference, DiscoveryInputScope, DiscoveryIssue,
    DiscoverySnapshot, EvaluationCacheEntry, EvaluationUnit, EvaluationUnitKey, ProjectDiscovery,
    ProjectKey, SourceMembership,
};
use crate::error::{Error, IndexError, Result};
use crate::types::{CrateInfo, path_wire};

fn encode(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|error| Error::Internal(format!("serialize discovery metadata: {error}")))
}

fn decode<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(|error| {
        Error::Internal(format!(
            "corrupt discovery metadata: {error}; run `tethys index --rebuild` to replace it"
        ))
    })
}

/// Decode one persisted path, reporting corruption instead of substituting a value.
fn decode_path(value: &str) -> Result<PathBuf> {
    path_wire::decode(value).map_err(Error::Internal)
}

/// Borrow `SQLite` text while retaining the column identity in conversion errors.
fn row_text<'a>(row: &'a rusqlite::Row<'_>, column: usize) -> rusqlite::Result<&'a str> {
    let value = row.get_ref(column)?;
    value.as_str().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, value.data_type(), Box::new(error))
    })
}

fn read_cache(conn: &Connection) -> Result<Vec<EvaluationCacheEntry>> {
    let mut statement =
        conn.prepare("SELECT cache_key, payload FROM evaluation_cache ORDER BY ordinal")?;
    let rows = statement.query_map([], |row| {
        Ok(EvaluationCacheEntry {
            key: row.get(0)?,
            payload: row.get(1)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

impl Index {
    /// Load opaque prior evaluation evidence without interpreting its payload.
    pub(crate) fn discovery_cache(&self) -> Result<Vec<EvaluationCacheEntry>> {
        let mut conn = self.connection()?;
        let tx = conn.savepoint()?;
        let cache = read_cache(&tx)?;
        tx.commit()?;
        Ok(cache)
    }

    /// Load only the published crate list, which every open needs.
    ///
    /// The rest of the publication stays unread until
    /// [`Self::discovery_snapshot`] is asked for it.
    pub(crate) fn discovery_crates(&self) -> Result<Option<Vec<CrateInfo>>> {
        let mut conn = self.connection()?;
        let tx = conn.savepoint()?;
        let crates = {
            let mut statement =
                tx.prepare("SELECT crates_json FROM evaluation_context WHERE singleton = 1")?;
            let mut rows = statement.query([])?;
            match rows.next()? {
                Some(row) => match decode::<Vec<CrateInfo>>(row_text(row, 0)?) {
                    Ok(crates) => Some(crates),
                    Err(error) => {
                        // Crate roots are re-derivable from the manifests, so a
                        // corrupt crate list must not block opening or indexing;
                        // reading the publication still reports it.
                        tracing::warn!(
                            %error,
                            "published crate list is unreadable; re-deriving from Cargo manifests"
                        );
                        None
                    }
                },
                None => None,
            }
        };
        tx.commit()?;
        Ok(crates)
    }

    /// Hydrate one coherent active publication, including inside an owned revision.
    pub(crate) fn discovery_snapshot(&self) -> Result<Option<DiscoverySnapshot>> {
        let mut conn = self.connection()?;
        let tx = conn.savepoint()?;
        let context = {
            let mut statement = tx.prepare(
                "SELECT crates_json, context_json, grants_json, cache_observations_json FROM evaluation_context WHERE singleton = 1",
            )?;
            let mut rows = statement.query([])?;
            rows.next()?
                .map(|row| -> Result<DiscoverySnapshot> {
                    Ok(DiscoverySnapshot {
                        crates: decode(row_text(row, 0)?)?,
                        context: decode(row_text(row, 1)?)?,
                        grants: decode(row_text(row, 2)?)?,
                        cache_observations: decode(row_text(row, 3)?)?,
                        ..DiscoverySnapshot::default()
                    })
                })
                .transpose()?
        };
        let Some(mut snapshot) = context else {
            tracing::debug!("no discovery snapshot has been published");
            tx.commit()?;
            return Ok(None);
        };
        snapshot.cache = read_cache(&tx)?;
        {
            let mut statement = tx.prepare(
                "SELECT project_key, containers_json, standing_json FROM projects ORDER BY ordinal",
            )?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                snapshot.projects.push(ProjectDiscovery {
                    key: ProjectKey(row.get(0)?),
                    containers: decode(row_text(row, 1)?)?,
                    standing: decode(row_text(row, 2)?)?,
                });
            }
        }
        read_units(&tx, &mut snapshot)?;
        {
            let mut statement = tx.prepare(
                "SELECT project_key, inputs_json FROM evaluation_inputs ORDER BY ordinal",
            )?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                snapshot.inputs.push(DiscoveryInputScope {
                    project: ProjectKey(row.get(0)?),
                    inputs: decode(row_text(row, 1)?)?,
                });
            }
        }
        {
            let mut statement =
                tx.prepare("SELECT path, failure_json FROM discovery_issues ORDER BY ordinal")?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                snapshot.issues.push(DiscoveryIssue {
                    path: decode_path(row_text(row, 0)?)?,
                    failure: decode(row_text(row, 1)?)?,
                });
            }
        }
        tx.commit()?;
        Ok(Some(snapshot))
    }

    /// Replace active metadata without removing syntax, atomically even when called alone.
    pub(crate) fn replace_discovery_snapshot(
        &self,
        snapshot: &DiscoverySnapshot,
        errors: &[IndexError],
        skipped_directories: &[(PathBuf, String)],
    ) -> Result<()> {
        let mut conn = self.connection()?;
        let tx = conn.savepoint()?;
        tx.execute_batch("DELETE FROM projects; DELETE FROM evaluation_context; DELETE FROM evaluation_cache; DELETE FROM discovery_issues; DELETE FROM source_diagnostics;")?;
        tx.execute(
            "INSERT INTO evaluation_context VALUES (1, ?1, ?2, ?3, ?4)",
            params![
                encode(&snapshot.crates)?,
                encode(&snapshot.context)?,
                encode(&snapshot.grants)?,
                encode(&snapshot.cache_observations)?,
            ],
        )?;
        {
            let mut statement = tx.prepare("INSERT INTO projects VALUES (?1, ?2, ?3, ?4)")?;
            for (ordinal, project) in snapshot.projects.iter().enumerate() {
                statement.execute(params![
                    project.key.as_str(),
                    ordinal,
                    encode(&project.containers)?,
                    encode(&project.standing)?
                ])?;
            }
        }
        write_units(&tx, &snapshot.units)?;
        {
            let mut statement = tx.prepare("INSERT INTO evaluation_inputs VALUES (?1, ?2, ?3)")?;
            for (ordinal, scope) in snapshot.inputs.iter().enumerate() {
                statement.execute(params![
                    ordinal,
                    scope.project.as_str(),
                    encode(&scope.inputs)?
                ])?;
            }
        }
        {
            let mut statement = tx.prepare("INSERT INTO evaluation_cache VALUES (?1, ?2, ?3)")?;
            for (ordinal, cache) in snapshot.cache.iter().enumerate() {
                statement.execute(params![ordinal, cache.key, cache.payload])?;
            }
        }
        {
            let mut statement = tx.prepare("INSERT INTO discovery_issues VALUES (?1, ?2, ?3)")?;
            for (ordinal, issue) in snapshot.issues.iter().enumerate() {
                statement.execute(params![
                    ordinal,
                    path_wire::encode(&issue.path),
                    encode(&issue.failure)?
                ])?;
            }
        }
        {
            let mut statement =
                tx.prepare("INSERT INTO source_diagnostics VALUES (?1, ?2, ?3, ?4)")?;
            for (ordinal, error) in errors.iter().enumerate() {
                statement.execute(params![
                    ordinal,
                    path_wire::encode(&error.path),
                    encode(error)?,
                    Option::<String>::None
                ])?;
            }
            for (ordinal, (path, reason)) in skipped_directories.iter().enumerate() {
                statement.execute(params![
                    errors.len() + ordinal,
                    path_wire::encode(path),
                    Option::<String>::None,
                    reason
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

fn read_units(conn: &Connection, snapshot: &mut DiscoverySnapshot) -> Result<()> {
    let mut unit_positions = HashMap::new();
    {
        let mut statement = conn.prepare("SELECT unit_key, project_key, target_framework, framework_json, standing_json, properties_json, host_json, restore_json FROM evaluation_units ORDER BY ordinal")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let key: String = row.get(0)?;
            unit_positions.insert(key.clone(), snapshot.units.len());
            snapshot.units.push(EvaluationUnit {
                key: EvaluationUnitKey(key),
                project: ProjectKey(row.get(1)?),
                target_framework: row.get(2)?,
                framework: decode(row_text(row, 3)?)?,
                standing: decode(row_text(row, 4)?)?,
                properties: decode(row_text(row, 5)?)?,
                host: decode(row_text(row, 6)?)?,
                restore: decode(row_text(row, 7)?)?,
                sources: Vec::new(),
                project_references: Vec::new(),
                assembly_references: Vec::new(),
            });
        }
    }
    {
        let mut statement = conn.prepare("SELECT unit_key, path, link, metadata_json FROM file_participation ORDER BY unit_key, ordinal")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            unit_mut(snapshot, &unit_positions, row_text(row, 0)?)?
                .sources
                .push(SourceMembership {
                    path: decode_path(row_text(row, 1)?)?,
                    link: row.get(2)?,
                    metadata: decode(row_text(row, 3)?)?,
                });
        }
    }
    {
        let mut statement = conn.prepare("SELECT unit_key, target_project_key, include, metadata_json FROM declared_project_references ORDER BY unit_key, ordinal")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            unit_mut(snapshot, &unit_positions, row_text(row, 0)?)?
                .project_references
                .push(DeclaredProjectReference {
                    target: ProjectKey(row.get(1)?),
                    include: row.get(2)?,
                    metadata: decode(row_text(row, 3)?)?,
                });
        }
    }
    {
        let mut statement = conn.prepare("SELECT unit_key, include, metadata_json FROM declared_assembly_references ORDER BY unit_key, ordinal")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            unit_mut(snapshot, &unit_positions, row_text(row, 0)?)?
                .assembly_references
                .push(DeclaredAssemblyReference {
                    include: row.get(1)?,
                    metadata: decode(row_text(row, 2)?)?,
                });
        }
    }
    Ok(())
}

fn write_units(conn: &Connection, evaluation_units: &[EvaluationUnit]) -> Result<()> {
    let file_ids = {
        let mut statement = conn.prepare("SELECT path, id FROM files")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        rows.collect::<rusqlite::Result<HashMap<_, _>>>()?
    };
    {
        let mut units = conn
            .prepare("INSERT INTO evaluation_units VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)")?;
        let mut sources =
            conn.prepare("INSERT INTO file_participation VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
        let mut projects =
            conn.prepare("INSERT INTO declared_project_references VALUES (?1, ?2, ?3, ?4, ?5)")?;
        let mut assemblies =
            conn.prepare("INSERT INTO declared_assembly_references VALUES (?1, ?2, ?3, ?4)")?;
        for (ordinal, unit) in evaluation_units.iter().enumerate() {
            let key = unit.key.as_str();
            units.execute(params![
                key,
                unit.project.as_str(),
                ordinal,
                unit.target_framework,
                encode(&unit.framework)?,
                encode(&unit.standing)?,
                encode(&unit.properties)?,
                encode(&unit.host)?,
                encode(&unit.restore)?
            ])?;
            for (ordinal, source) in unit.sources.iter().enumerate() {
                let path = path_wire::encode(&source.path);
                sources.execute(params![
                    key,
                    path,
                    file_ids.get(&path),
                    ordinal,
                    source.link,
                    encode(&source.metadata)?
                ])?;
            }
            for (ordinal, reference) in unit.project_references.iter().enumerate() {
                projects.execute(params![
                    key,
                    ordinal,
                    reference.target.as_str(),
                    reference.include,
                    encode(&reference.metadata)?
                ])?;
            }
            for (ordinal, reference) in unit.assembly_references.iter().enumerate() {
                assemblies.execute(params![
                    key,
                    ordinal,
                    reference.include,
                    encode(&reference.metadata)?
                ])?;
            }
        }
    }
    Ok(())
}

fn unit_mut<'a>(
    snapshot: &'a mut DiscoverySnapshot,
    positions: &HashMap<String, usize>,
    key: &str,
) -> Result<&'a mut EvaluationUnit> {
    let position = positions
        .get(key)
        .ok_or_else(|| Error::Internal(format!("orphaned discovery unit: {key}")))?;
    Ok(&mut snapshot.units[*position])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::{
        DiscoveryCacheObservation, DiscoveryDiagnostic, DiscoveryFailure, DiscoveryFailureReason,
        DiscoveryRestoreStyle, DiscoveryStanding, EvaluationHostKind, EvaluationInput,
        FrameworkIdentity, HostProvenance, RestoreProvenance,
    };
    use crate::error::IndexErrorKind;
    use crate::{CrateInfo, Language};
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::time::{Duration, UNIX_EPOCH};

    fn confirmed_unit(project: &ProjectKey) -> EvaluationUnit {
        EvaluationUnit {
            key: EvaluationUnitKey("app/net8".into()),
            project: project.clone(),
            target_framework: Some("net8.0".into()),
            framework: Some(FrameworkIdentity {
                short_name: Some("net8.0".into()),
                identifier: ".NETCoreApp".into(),
                version: "v8.0".into(),
                profile: String::new(),
                platform_identifier: "Windows".into(),
                platform_version: "10.0".into(),
            }),
            standing: DiscoveryStanding::Confirmed,
            properties: BTreeMap::from([("AssemblyName".into(), "SharedName".into())]),
            sources: vec![
                SourceMembership {
                    path: "shared.cs".into(),
                    link: None,
                    metadata: BTreeMap::from([("Visible".into(), "false".into())]),
                },
                SourceMembership {
                    path: "unparsed.cs".into(),
                    link: Some(String::new()),
                    metadata: BTreeMap::new(),
                },
            ],
            project_references: vec![DeclaredProjectReference {
                target: ProjectKey("missing/missing.csproj".into()),
                include: "../missing/missing.csproj".into(),
                metadata: BTreeMap::from([("ReferenceOutputAssembly".into(), "false".into())]),
            }],
            assembly_references: vec![
                DeclaredAssemblyReference {
                    include: "Zed".into(),
                    metadata: BTreeMap::from([("HintPath".into(), "../lib/Zed.dll".into())]),
                },
                DeclaredAssemblyReference {
                    include: "Alpha".into(),
                    metadata: BTreeMap::new(),
                },
            ],
            host: Some(HostProvenance {
                kind: EvaluationHostKind::Sdk,
                path: "/sdk/MSBuild.dll".into(),
                version: "17.0".into(),
                runtime: ".NET 8".into(),
            }),
            restore: RestoreProvenance {
                style: DiscoveryRestoreStyle::PackageReference,
                inputs: vec!["/repo/app/obj/project.assets.json".into()],
            },
        }
    }

    fn fixture() -> DiscoverySnapshot {
        let project = ProjectKey("app/app.csproj".into());
        let failure = DiscoveryFailure {
            reason: DiscoveryFailureReason::PartialTargetFrameworks,
            diagnostics: vec![DiscoveryDiagnostic {
                code: Some("SDK100".into()),
                message: "missing target SDK".into(),
                file: Some("app/app.csproj".into()),
                line: 3,
                column: 4,
                ..DiscoveryDiagnostic::default()
            }],
        };
        let unit = confirmed_unit(&project);
        let mut failed = unit.clone();
        failed.key = EvaluationUnitKey("app/net9".into());
        failed.target_framework = Some("net9.0".into());
        failed.framework = None;
        failed.standing = DiscoveryStanding::Indeterminate(failure.clone());
        failed.sources[0].link = Some(String::new());
        let input = EvaluationInput {
            path: "app/app.csproj".into(),
            canonical_path: "/repo/app/app.csproj".into(),
            length: 12,
            modified: UNIX_EPOCH + Duration::new(42, 123),
            digest: "first".into(),
        };
        let mut changed_input = input.clone();
        changed_input.digest = "invalidated".into();
        let mut snapshot = DiscoverySnapshot {
            crates: vec![CrateInfo {
                name: "rust".into(),
                path: "/repo/rust".into(),
                lib_path: Some("src/lib.rs".into()),
                bin_paths: vec![("tool".into(), "src/main.rs".into())],
            }],
            projects: vec![ProjectDiscovery {
                key: project.clone(),
                containers: vec!["all.sln".into(), "subset.slnf".into()],
                standing: DiscoveryStanding::Indeterminate(failure.clone()),
            }],
            units: vec![unit, failed],
            inputs: vec![
                DiscoveryInputScope {
                    project: project.clone(),
                    inputs: vec![input],
                },
                DiscoveryInputScope {
                    project: project.clone(),
                    inputs: vec![changed_input],
                },
                DiscoveryInputScope {
                    project: project.clone(),
                    inputs: vec![],
                },
            ],
            issues: vec![DiscoveryIssue {
                path: "bad.sln".into(),
                failure,
            }],
            cache: vec![EvaluationCacheEntry {
                key: "opaque".into(),
                payload: "not JSON; owned by discovery".into(),
            }],
            cache_observations: vec![DiscoveryCacheObservation {
                project,
                target_framework: None,
                reused: false,
                bypass_reasons: vec!["changed".into()],
            }],
            ..DiscoverySnapshot::default()
        };
        snapshot.grants.trust_msbuild = true;
        snapshot.grants.allow_restore = true;
        snapshot.context.configuration = Some("Release".into());
        snapshot
    }

    fn write_source(index: &Index) {
        index
            .index_parsed_file_atomic(
                Path::new("shared.cs"),
                Language::CSharp,
                1,
                1,
                None,
                &[],
                &[],
                &[],
            )
            .expect("syntax");
    }

    #[test]
    fn reopened_snapshot_preserves_shared_membership_and_partial_evidence() {
        let dir = tempfile::tempdir().expect("directory");
        let path = dir.path().join("index.db");
        let index = Index::open(&path).expect("index");
        assert!(index.discovery_snapshot().expect("unpublished").is_none());
        write_source(&index);
        let expected = fixture();
        let error = IndexError {
            path: "unparsed.cs".into(),
            kind: IndexErrorKind::ParseFailed,
            message: "invalid source".into(),
        };
        index
            .replace_discovery_snapshot(
                &expected,
                &[error],
                &[("ignored".into(), "unreadable".into())],
            )
            .expect("publish");
        drop(index);
        let reopened = Index::open(&path).expect("reopen");
        let actual = reopened
            .discovery_snapshot()
            .expect("hydrate")
            .expect("publication");
        assert_eq!(actual, expected);
        assert!(!actual.is_complete());
        assert_eq!(reopened.discovery_cache().expect("cache"), expected.cache);
        {
            let conn = reopened.connection().expect("connection");
            let joined: i64 = conn.query_row("SELECT COUNT(*) FROM file_participation JOIN files ON files.id = file_participation.file_id WHERE files.path = 'shared.cs'", [], |row| row.get(0)).expect("shared membership");
            assert_eq!(joined, 2);
            let absent: i64 = conn.query_row("SELECT COUNT(*) FROM file_participation WHERE path = 'unparsed.cs' AND file_id IS NULL AND link = ''", [], |row| row.get(0)).expect("unparsed membership");
            assert_eq!(absent, 2);
            let persisted: String = conn
                .query_row(
                    "SELECT error_json FROM source_diagnostics WHERE path = 'unparsed.cs'",
                    [],
                    |row| row.get(0),
                )
                .expect("source error");
            let persisted: IndexError = decode(&persisted).expect("typed error");
            assert_eq!(persisted.kind, IndexErrorKind::ParseFailed);
            let reason: String = conn
                .query_row(
                    "SELECT directory_reason FROM source_diagnostics WHERE path = 'ignored'",
                    [],
                    |row| row.get(0),
                )
                .expect("directory diagnostic");
            assert_eq!(reason, "unreadable");
        }
        let id = reopened
            .get_file_id(Path::new("shared.cs"))
            .expect("file")
            .expect("syntax id");
        reopened.delete_files(&[id]).expect("remove syntax");
        assert_eq!(
            reopened
                .discovery_snapshot()
                .expect("read after syntax deletion"),
            Some(expected)
        );
        let conn = reopened.connection().expect("connection");
        let attached: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM file_participation WHERE file_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .expect("remaining attachments");
        assert_eq!(attached, 0);
    }

    #[test]
    fn replacement_is_coherent_with_revision_and_empty_clears_metadata_not_syntax() {
        let dir = tempfile::tempdir().expect("directory");
        let path = dir.path().join("index.db");
        let index = Index::open(&path).expect("index");
        write_source(&index);
        let old = fixture();
        index
            .replace_discovery_snapshot(&old, &[], &[])
            .expect("old");
        let observer = Index::open(&path).expect("observer");
        let mut replacement = old.clone();
        replacement.units.remove(0);
        replacement.cache.clear();
        replacement.inputs.clear();
        replacement.issues.clear();
        let revision = index.begin_revision(false).expect("revision");
        index
            .replace_discovery_snapshot(&replacement, &[], &[])
            .expect("replacement");
        assert_eq!(
            index.discovery_snapshot().expect("nested read"),
            Some(replacement.clone())
        );
        assert_eq!(observer.discovery_snapshot().expect("old read"), Some(old));
        revision.commit().expect("publish");
        assert_eq!(
            observer.discovery_snapshot().expect("new read"),
            Some(replacement)
        );
        index
            .replace_discovery_snapshot(&DiscoverySnapshot::default(), &[], &[])
            .expect("empty");
        assert_eq!(
            observer.discovery_snapshot().expect("empty read"),
            Some(DiscoverySnapshot::default())
        );
        assert!(
            index
                .get_file_id(Path::new("shared.cs"))
                .expect("source remains")
                .is_some()
        );
        let conn = index.connection().expect("connection");
        for table in [
            "projects",
            "evaluation_units",
            "file_participation",
            "declared_project_references",
            "declared_assembly_references",
            "evaluation_inputs",
            "evaluation_cache",
            "discovery_issues",
            "source_diagnostics",
        ] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("empty table");
            assert_eq!(count, 0, "{table}");
        }
    }

    #[test]
    fn failed_constraint_rolls_back_replacement_inside_existing_revision() {
        let dir = tempfile::tempdir().expect("directory");
        let index = Index::open(&dir.path().join("index.db")).expect("index");
        let old = fixture();
        index
            .replace_discovery_snapshot(&old, &[], &[])
            .expect("old");
        let revision = index.begin_revision(false).expect("revision");
        write_source(&index);
        let mut invalid = old.clone();
        let duplicate = invalid.units[0].sources[0].clone();
        invalid.units[0].sources.push(duplicate);
        let error = index
            .replace_discovery_snapshot(&invalid, &[], &[])
            .expect_err("duplicate physical membership");
        assert!(
            matches!(&error, Error::Database(rusqlite::Error::SqliteFailure(error, _)) if error.code == rusqlite::ErrorCode::ConstraintViolation)
        );
        assert_eq!(
            index.discovery_snapshot().expect("rolled back metadata"),
            Some(old.clone())
        );
        assert!(
            index
                .get_file_id(Path::new("shared.cs"))
                .expect("outer syntax")
                .is_some()
        );
        revision.commit().expect("outer transaction remains usable");
        assert_eq!(
            index.discovery_snapshot().expect("published old metadata"),
            Some(old)
        );
    }

    fn issue_fixture(path: PathBuf) -> DiscoverySnapshot {
        let mut snapshot = DiscoverySnapshot::default();
        snapshot.issues.push(DiscoveryIssue {
            path,
            failure: DiscoveryFailure {
                reason: DiscoveryFailureReason::EvaluationFailed,
                diagnostics: Vec::new(),
            },
        });
        snapshot
    }

    #[test]
    fn metadata_only_replacement_detaches_units_without_destroying_architecture() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("index.db");
        let index = Index::open(&path).expect("index");
        {
            let conn = index.connection().expect("connection");
            conn.execute_batch(
                "INSERT INTO projects VALUES ('App.csproj', 0, '[]', '{\"standing\":\"confirmed\"}');
                 INSERT INTO evaluation_units VALUES ('app-unit', 'App.csproj', 0, 'net8.0', 'null', '{\"standing\":\"confirmed\"}', '{}', 'null', '{\"style\":\"none\",\"inputs\":[]}');
                 INSERT INTO arch_packages (id, name, path, source, evaluation_unit_key)
                     VALUES (1, 'msbuild:App.csproj:app-unit', 'app', 'msbuild', 'app-unit');
                 INSERT INTO arch_packages (id, name, path, source) VALUES (2, 'rust-dep', 'dep', 'manifest');
                 INSERT INTO arch_package_deps VALUES (1, 2, 3);",
            )
            .expect("fixture");
        }
        index
            .replace_discovery_snapshot(&DiscoverySnapshot::default(), &[], &[])
            .expect("replacing discovery metadata alone must succeed");
        let conn = index.connection().expect("connection");
        let packages: i64 = conn
            .query_row("SELECT COUNT(*) FROM arch_packages", [], |row| row.get(0))
            .expect("packages");
        assert_eq!(packages, 2, "architecture nodes survive a metadata replacement");
        let edges: i64 = conn
            .query_row("SELECT COUNT(*) FROM arch_package_deps", [], |row| row.get(0))
            .expect("edges");
        assert_eq!(edges, 1, "a metadata replacement must not delete architecture edges");
        let detached: Option<String> = conn
            .query_row(
                "SELECT evaluation_unit_key FROM arch_packages WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .expect("unit key");
        assert_eq!(
            detached, None,
            "a withdrawn unit is detached from its node, not cascaded through it"
        );
        let afferent: i64 = conn
            .query_row(
                "SELECT afferent FROM arch_coupling WHERE package_id = 2",
                [],
                |row| row.get(0),
            )
            .expect("coupling view");
        assert_eq!(
            afferent, 1,
            "the neighbour's measured afferent count is unchanged"
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_issue_path_round_trips_through_publication() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("index.db");
        let path_with_invalid_bytes = PathBuf::from(OsString::from_vec(b"/repo/bad\xff".to_vec()));
        assert!(path_with_invalid_bytes.to_str().is_none());
        let snapshot = issue_fixture(path_with_invalid_bytes.clone());
        let index = Index::open(&path).expect("index");
        index
            .replace_discovery_snapshot(&snapshot, &[], &[])
            .expect("a non-UTF-8 issue path must not abort publication");
        drop(index);
        let reopened = Index::open(&path).expect("reopen");
        let hydrated = reopened
            .discovery_snapshot()
            .expect("hydrate")
            .expect("publication");
        assert_eq!(
            hydrated.issues[0].path, path_with_invalid_bytes,
            "issue paths must round-trip byte-exactly"
        );
    }

    #[cfg(windows)]
    #[test]
    fn verbatim_issue_path_round_trips_through_publication() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("index.db");
        let verbatim = PathBuf::from(r"\\?\C:\repo\hidden");
        let snapshot = issue_fixture(verbatim.clone());
        let index = Index::open(&path).expect("index");
        index
            .replace_discovery_snapshot(&snapshot, &[], &[])
            .expect("publish");
        drop(index);
        let reopened = Index::open(&path).expect("reopen");
        let hydrated = reopened
            .discovery_snapshot()
            .expect("hydrate")
            .expect("publication");
        assert_eq!(
            hydrated.issues[0].path, verbatim,
            "a verbatim prefix must survive the round trip"
        );
    }

    #[test]
    fn corrupt_json_is_an_error_not_missing_or_empty_discovery() {
        let dir = tempfile::tempdir().expect("directory");
        let index = Index::open(&dir.path().join("index.db")).expect("index");
        index
            .replace_discovery_snapshot(&fixture(), &[], &[])
            .expect("publish");
        index
            .connection()
            .expect("connection")
            .execute("UPDATE evaluation_units SET standing_json = 'not json'", [])
            .expect("corrupt stored evidence");
        assert!(matches!(
            index.discovery_snapshot(),
            Err(Error::Internal(_))
        ));
    }
}
