//! Architecture-analysis storage layer.
//!
//! Owns the four `arch_*` schema objects and the queries that read and write them.
//! Wired into the indexing pipeline by `Tethys::run_architecture_phase`.

use std::collections::HashMap;

use rusqlite::{Connection, params};
use tracing::trace;

use super::Index;
use crate::architecture::{
    ArchStats, CouplingDetail, CouplingMetrics, CouplingSort, Package, PackageDependency,
    PackageId, PackageSource,
};
use crate::architecture::{
    DeclaredContext, EvaluationUnitCoupling, MetricEvidence, apply_evidence,
};
use crate::discovery::{DeclaredProjectReference, EvaluationUnitKey, ProjectKey};
use crate::error::Result;
use crate::types::FileId;

/// Insert payload for `repopulate_architecture`.
pub struct PackageInsert<'a> {
    pub name: &'a str,
    pub path: &'a str,
    pub source: PackageSource,
    pub evaluation_unit_key: Option<&'a EvaluationUnitKey>,
}

impl Index {
    /// Rebuild every `arch_*` table from the supplied package list and file
    /// mappings, plus the current state of `file_deps`. Single transaction.
    ///
    /// Idempotent: identical input produces identical state.
    ///
    /// `file_to_package_name` entries whose name is not present in `packages`
    /// are silently skipped (logged at `trace!`); this lets callers feed
    /// best-effort name lookups without pre-filtering.
    ///
    /// # Errors
    /// Returns an error if `packages` contains duplicate names (violates the
    /// UNIQUE constraint on `arch_packages.name`). Callers must de-duplicate
    /// before calling.
    pub fn repopulate_architecture(
        &self,
        packages: &[PackageInsert<'_>],
        file_to_package_name: &[(FileId, &str)],
    ) -> Result<ArchStats> {
        // Fail fast in dev/test if the caller forgot to dedupe. In release the
        // `UNIQUE` constraint still catches it; this just gives a clearer panic
        // before that opaque DB error reaches the user.
        debug_assert_eq!(
            packages
                .iter()
                .map(|p| p.name)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            packages.len(),
            "repopulate_architecture: duplicate package names in input"
        );

        let mut conn = self.connection()?;
        let tx = conn.savepoint()?;

        // 1. Wipe. Cascade clears the two child tables.
        tx.execute("DELETE FROM arch_packages", [])?;

        // 2. Insert packages; capture last_insert_rowid() instead of selecting
        //    back — avoids a round-trip query and keeps the mapping close to insertion.
        let mut name_to_id: HashMap<&str, PackageId> = HashMap::with_capacity(packages.len());
        {
            let mut stmt =
                tx.prepare("INSERT INTO arch_packages (name, path, source, evaluation_unit_key) VALUES (?1, ?2, ?3, ?4)")?;
            for pkg in packages {
                stmt.execute(params![
                    pkg.name,
                    pkg.path,
                    pkg.source.as_str(),
                    pkg.evaluation_unit_key.map(EvaluationUnitKey::as_str)
                ])?;
                // Safe because (a) the transaction holds the exclusive write lock for
                // its lifetime, so no other writer interleaves, and (b) `arch_packages`
                // uses INTEGER PRIMARY KEY — `last_insert_rowid()` returns that rowid.
                // If the PK is ever migrated to a non-rowid type (e.g., UUID, text key,
                // or WITHOUT ROWID), switch back to a SELECT-back approach.
                name_to_id.insert(pkg.name, PackageId::new(tx.last_insert_rowid()));
            }
        }
        let packages_recorded = packages.len();

        // 3. Insert file → package mappings, skipping unknown names.
        let mut files_assigned: usize = 0;
        {
            let mut stmt =
                tx.prepare("INSERT INTO arch_file_packages (file_id, package_id) VALUES (?1, ?2)")?;
            for (file_id, name) in file_to_package_name {
                if let Some(pkg_id) = name_to_id.get(name) {
                    stmt.execute(params![file_id.as_i64(), pkg_id.as_i64()])?;
                    files_assigned += 1;
                } else {
                    trace!(
                        file_id = file_id.as_i64(),
                        package_name = %name,
                        "skipping file with unknown package name"
                    );
                }
            }
        }

        // 4. Roll up cross-package edges.
        let package_deps_recorded = tx.execute(
            "INSERT INTO arch_package_deps (source_pkg, target_pkg, dep_count)
             SELECT sp.package_id, tp.package_id, COUNT(*)
             FROM file_deps fd
             JOIN arch_file_packages sp ON sp.file_id = fd.from_file_id
             JOIN arch_file_packages tp ON tp.file_id = fd.to_file_id
             WHERE sp.package_id <> tp.package_id
             GROUP BY sp.package_id, tp.package_id",
            [],
        )?;

        tx.commit()?;

        Ok(ArchStats {
            packages_recorded,
            files_assigned,
            package_deps_recorded,
        })
    }

    /// Return every package row, ordered alphabetically by name for determinism.
    /// Unknown `source` values produce a `warn!` and are skipped.
    pub fn get_packages(&self) -> Result<Vec<Package>> {
        use std::path::PathBuf;

        let conn = self.connection()?;
        let mut stmt =
            conn.prepare("SELECT id, name, path, source FROM arch_packages ORDER BY name ASC")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, name, path, source_str) = row?;
            let Some(source) = PackageSource::parse(&source_str) else {
                tracing::warn!(
                    package_name = %name,
                    source = %source_str,
                    "skipping package with unknown source value"
                );
                continue;
            };
            out.push(Package {
                id: PackageId::new(id),
                name,
                path: PathBuf::from(path),
                source,
            });
        }
        Ok(out)
    }

    /// Coupling metrics for every package, sorted per the requested key.
    ///
    /// Rows are fetched unsorted from the DB. For [`CouplingSort::Instability`],
    /// the Rust-side sort calls [`CouplingMetrics::instability`], keeping the
    /// formula in a single canonical location. The other three sort variants
    /// (`Afferent`, `Efferent`, `Name`) compare the stored `afferent`/`efferent`
    /// integer fields and the `name` string directly.
    pub fn get_coupling_metrics(&self, sort: CouplingSort) -> Result<Vec<CouplingMetrics>> {
        let mut conn = self.connection()?;
        let tx = conn.savepoint()?;
        let mut out = read_metrics(&tx, None)?;
        tx.commit()?;
        out.sort_by(|a, b| {
            let ordering = match sort {
                CouplingSort::Instability => {
                    compare_evidence(a.instability(), b.instability(), f64::total_cmp)
                }
                CouplingSort::Afferent => compare_evidence(a.afferent, b.afferent, u32::cmp),
                CouplingSort::Efferent => compare_evidence(a.efferent, b.efferent, u32::cmp),
                CouplingSort::Name => std::cmp::Ordering::Equal,
            };
            ordering.then_with(|| a.package.name.cmp(&b.package.name))
        });
        Ok(out)
    }
}

/// Convert an `i64` from the DB to a `u32`, saturating at `u32::MAX` with a
/// `warn!` log when the value doesn't fit. Mirrors `lib.rs::saturating_depth_to_u32`.
fn saturating_coupling_to_u32(value: i64, package_name: &str, field: &str) -> u32 {
    u32::try_from(value).unwrap_or_else(|_| {
        tracing::warn!(
            package_name = %package_name,
            field = %field,
            requested = value,
            cap = u32::MAX,
            "coupling value exceeds u32::MAX; saturating"
        );
        u32::MAX
    })
}

#[cfg(test)]
mod get_packages_tests {
    use super::*;
    use tempfile::TempDir;

    fn seeded_index() -> (TempDir, Index) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("idx.db");
        let index = Index::open(&path).expect("open");
        let packages = [
            PackageInsert {
                name: "z_crate",
                path: "crates/z",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "a_crate",
                path: "crates/a",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
        ];
        index
            .repopulate_architecture(&packages, &[])
            .expect("repopulate");
        (dir, index)
    }

    #[test]
    fn get_packages_returns_alphabetical_by_name() {
        let (_dir, index) = seeded_index();
        let pkgs = index.get_packages().expect("get_packages");
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].name, "a_crate");
        assert_eq!(pkgs[1].name, "z_crate");
    }

    #[test]
    fn get_packages_decodes_source_field() {
        let (_dir, index) = seeded_index();
        let pkgs = index.get_packages().expect("get_packages");
        assert!(pkgs.iter().all(|p| p.source == PackageSource::Manifest));
    }

    #[test]
    fn get_packages_empty_for_fresh_index() {
        let dir = tempfile::tempdir().expect("temp dir");
        let index = Index::open(&dir.path().join("idx.db")).expect("open");
        assert!(index.get_packages().expect("get_packages").is_empty());
    }
}

/// Direction used by [`Index::fetch_neighbors`] to query either dependents or
/// dependencies of a package.
#[derive(Clone, Copy, Debug)]
enum Direction {
    Outgoing,
    Incoming,
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::Outgoing => f.write_str("outgoing"),
            Direction::Incoming => f.write_str("incoming"),
        }
    }
}

impl Index {
    /// Detailed coupling for one package by exact name.
    ///
    /// Returns `Ok(None)` when no package matches. Returns `Err` on a DB failure
    /// or if the matched package row has an unrecognised `source` value (which
    /// represents corruption or a schema-version mismatch for the target — see
    /// the asymmetric behaviour note below).
    ///
    /// Incoming and outgoing lists are sorted by `dep_count` descending, then
    /// by name ascending.
    ///
    /// # Error asymmetry between target and neighbours
    ///
    /// Corrupt-source handling is deliberately asymmetric:
    ///
    /// - For the **target package** (this method): an unrecognised `source` is
    ///   `Err`, because the caller asked for this specific package and silently
    ///   returning `Ok(None)` would lie about its existence.
    /// - For **neighbour packages** (in `fetch_neighbors`): an unrecognised
    ///   `source` is logged at `warn!` and the neighbour is omitted from the
    ///   returned list. One corrupt neighbour does not abort the whole detail
    ///   query.
    ///
    /// A consequence: in the presence of corruption, the `incoming` /
    /// `outgoing` lists may be silently truncated. Callers that need strict
    /// integrity should monitor the `warn!` log channel from this module.
    pub fn get_package_coupling(&self, name: &str) -> Result<Option<CouplingDetail>> {
        let mut conn = self.connection()?;
        let tx = conn.savepoint()?;
        let metrics = read_metrics(&tx, Some(name))?
            .into_iter()
            .find(|metric| metric.package.name == name);
        let Some(metrics) = metrics else {
            tracing::debug!(
                package_name = name,
                "no architecture package matches selector"
            );
            tx.commit()?;
            return Ok(None);
        };
        let outgoing = Self::fetch_neighbors(&tx, metrics.package.id, Direction::Outgoing)?;
        let incoming = Self::fetch_neighbors(&tx, metrics.package.id, Direction::Incoming)?;
        tx.commit()?;
        Ok(Some(CouplingDetail {
            metrics,
            incoming,
            outgoing,
        }))
    }

    /// Fetch neighbouring packages in the requested direction.
    ///
    /// The two queries are spelled out as separate `const` strings rather than
    /// assembled at runtime via `format!`. This makes the column-name choice
    /// visible to a reader without relying on a "no user input reaches SQL"
    /// comment, and makes the safety property structural rather than
    /// behavioural.
    fn fetch_neighbors(
        conn: &Connection,
        package_id: PackageId,
        dir: Direction,
    ) -> Result<Vec<PackageDependency>> {
        use std::path::PathBuf;

        /// SQL for "packages this package depends on" (outgoing edges).
        const OUTGOING_SQL: &str = "
            SELECT p.id, p.name, p.path, p.source, d.dep_count
            FROM arch_package_deps d
            JOIN arch_packages p ON p.id = d.target_pkg
            WHERE d.source_pkg = ?1
            ORDER BY d.dep_count DESC, p.name ASC";

        /// SQL for "packages that depend on this package" (incoming edges).
        const INCOMING_SQL: &str = "
            SELECT p.id, p.name, p.path, p.source, d.dep_count
            FROM arch_package_deps d
            JOIN arch_packages p ON p.id = d.source_pkg
            WHERE d.target_pkg = ?1
            ORDER BY d.dep_count DESC, p.name ASC";

        let sql = match dir {
            Direction::Outgoing => OUTGOING_SQL,
            Direction::Incoming => INCOMING_SQL,
        };

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![package_id.as_i64()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, name, path, source_str, dep_count) = row?;
            let Some(source) = PackageSource::parse(&source_str) else {
                tracing::warn!(
                    package_name = %name,
                    source = %source_str,
                    direction = %dir,
                    "neighbor package has unknown source value; omitting from results"
                );
                continue;
            };
            let dep_count_u32 = saturating_coupling_to_u32(dep_count, &name, "dep_count");
            out.push(PackageDependency {
                package: Package {
                    id: PackageId::new(id),
                    name,
                    path: PathBuf::from(path),
                    source,
                },
                dep_count: dep_count_u32,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod package_coupling_tests {
    use super::*;
    use crate::types::Language;
    use std::path::Path;
    use tempfile::TempDir;
    use tracing_test::traced_test;

    fn seeded_index() -> (TempDir, Index) {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut index = Index::open(&dir.path().join("idx.db")).expect("open");

        let f_a = index
            .upsert_file(Path::new("a/lib.rs"), Language::Rust, 0, 0, None)
            .expect("a");
        let f_b = index
            .upsert_file(Path::new("b/lib.rs"), Language::Rust, 0, 0, None)
            .expect("b");
        let f_c = index
            .upsert_file(Path::new("c/lib.rs"), Language::Rust, 0, 0, None)
            .expect("c");

        index.insert_file_dependency(f_a, f_b).expect("a→b");
        index.insert_file_dependency(f_a, f_c).expect("a→c");
        index.insert_file_dependency(f_b, f_c).expect("b→c");

        let packages = [
            PackageInsert {
                name: "a",
                path: "a",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "b",
                path: "b",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "c",
                path: "c",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
        ];
        let mappings = [(f_a, "a"), (f_b, "b"), (f_c, "c")];
        index
            .repopulate_architecture(&packages, &mappings)
            .expect("repopulate");
        (dir, index)
    }

    #[test]
    fn package_coupling_returns_outgoing_and_incoming() {
        let (_dir, index) = seeded_index();
        let detail = index
            .get_package_coupling("b")
            .expect("query")
            .expect("found");

        assert_eq!(detail.metrics.package.name, "b");
        assert_eq!(
            (detail.metrics.afferent, detail.metrics.efferent),
            (MetricEvidence::Known(1), MetricEvidence::Known(1))
        );

        let in_names: Vec<_> = detail
            .incoming
            .iter()
            .map(|d| d.package.name.as_str())
            .collect();
        assert_eq!(in_names, ["a"]);

        let out_names: Vec<_> = detail
            .outgoing
            .iter()
            .map(|d| d.package.name.as_str())
            .collect();
        assert_eq!(out_names, ["c"]);
    }

    #[test]
    fn package_coupling_none_for_missing_package() {
        let (_dir, index) = seeded_index();
        assert!(
            index
                .get_package_coupling("does-not-exist")
                .expect("query")
                .is_none()
        );
    }

    #[test]
    fn package_coupling_for_isolated_package_returns_empty_lists() {
        let dir = tempfile::tempdir().expect("temp dir");
        let index = Index::open(&dir.path().join("idx.db")).expect("open");
        let packages = [PackageInsert {
            name: "lonely",
            path: "lonely",
            source: PackageSource::Manifest,
            evaluation_unit_key: None,
        }];
        index
            .repopulate_architecture(&packages, &[])
            .expect("repopulate");

        let detail = index
            .get_package_coupling("lonely")
            .expect("query")
            .expect("found");
        assert!(detail.incoming.is_empty());
        assert!(detail.outgoing.is_empty());
        assert_eq!(detail.metrics.instability(), MetricEvidence::Known(0.0));
    }

    #[test]
    fn package_coupling_dep_count_aggregates_multiple_file_edges() {
        // a has TWO files, both depending on b's single file.
        // Expected: arch_package_deps(a, b).dep_count == 2.
        let dir = tempfile::tempdir().expect("temp dir");
        let mut index = Index::open(&dir.path().join("idx.db")).expect("open");

        let f_a1 = index
            .upsert_file(Path::new("a/lib.rs"), Language::Rust, 0, 0, None)
            .expect("a1");
        let f_a2 = index
            .upsert_file(Path::new("a/helpers.rs"), Language::Rust, 0, 0, None)
            .expect("a2");
        let f_b = index
            .upsert_file(Path::new("b/lib.rs"), Language::Rust, 0, 0, None)
            .expect("b");

        index.insert_file_dependency(f_a1, f_b).expect("a1→b");
        index.insert_file_dependency(f_a2, f_b).expect("a2→b");

        let packages = [
            PackageInsert {
                name: "a",
                path: "a",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "b",
                path: "b",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
        ];
        let mappings = [(f_a1, "a"), (f_a2, "a"), (f_b, "b")];
        index
            .repopulate_architecture(&packages, &mappings)
            .expect("repopulate");

        let detail = index
            .get_package_coupling("a")
            .expect("query")
            .expect("found");
        assert_eq!(detail.outgoing.len(), 1, "single edge to b");
        assert_eq!(detail.outgoing[0].package.name, "b");
        assert_eq!(
            detail.outgoing[0].dep_count, 2,
            "two file-edges roll up to dep_count=2"
        );
    }

    #[test]
    fn repopulate_architecture_filters_intra_package_deps() {
        // Two files in the same package, with a file_dep between them.
        // The dep must NOT appear in arch_package_deps (no self-edges allowed).
        let dir = tempfile::tempdir().expect("temp dir");
        let mut index = Index::open(&dir.path().join("idx.db")).expect("open");

        let f1 = index
            .upsert_file(Path::new("a/lib.rs"), Language::Rust, 0, 0, None)
            .expect("f1");
        let f2 = index
            .upsert_file(Path::new("a/helpers.rs"), Language::Rust, 0, 0, None)
            .expect("f2");
        index.insert_file_dependency(f1, f2).expect("f1→f2");

        let packages = [PackageInsert {
            name: "a",
            path: "a",
            source: PackageSource::Manifest,
            evaluation_unit_key: None,
        }];
        let mappings = [(f1, "a"), (f2, "a")];
        let stats = index
            .repopulate_architecture(&packages, &mappings)
            .expect("repopulate");

        assert_eq!(
            stats.package_deps_recorded, 0,
            "intra-package dep filtered out"
        );

        let detail = index
            .get_package_coupling("a")
            .expect("query")
            .expect("found");
        assert!(detail.outgoing.is_empty());
        assert!(detail.incoming.is_empty());
        assert_eq!(detail.metrics.afferent, MetricEvidence::Known(0));
        assert_eq!(detail.metrics.efferent, MetricEvidence::Known(0));
    }

    #[test]
    fn fetch_neighbors_sorts_by_dep_count_desc_then_name_asc() {
        // Fixture: package "hub" has three outgoing neighbors with varying edge counts.
        //   alpha  — 1 edge   (ties with gamma; alpha < gamma alphabetically)
        //   beta   — 3 edges  (highest dep_count → first)
        //   gamma  — 1 edge   (ties with alpha; gamma > alpha alphabetically)
        //
        // Expected outgoing order: [beta, alpha, gamma].
        let dir = tempfile::tempdir().expect("temp dir");
        let mut index = Index::open(&dir.path().join("idx.db")).expect("open");

        let f_hub_a = index
            .upsert_file(Path::new("hub/a.rs"), Language::Rust, 0, 0, None)
            .expect("hub a");
        let f_hub_b = index
            .upsert_file(Path::new("hub/b.rs"), Language::Rust, 0, 0, None)
            .expect("hub b");
        let f_hub_c = index
            .upsert_file(Path::new("hub/c.rs"), Language::Rust, 0, 0, None)
            .expect("hub c");
        let f_alpha = index
            .upsert_file(Path::new("alpha/lib.rs"), Language::Rust, 0, 0, None)
            .expect("alpha");
        let f_beta_1 = index
            .upsert_file(Path::new("beta/lib.rs"), Language::Rust, 0, 0, None)
            .expect("beta 1");
        let f_beta_2 = index
            .upsert_file(Path::new("beta/helpers.rs"), Language::Rust, 0, 0, None)
            .expect("beta 2");
        let f_beta_3 = index
            .upsert_file(Path::new("beta/util.rs"), Language::Rust, 0, 0, None)
            .expect("beta 3");
        let f_gamma = index
            .upsert_file(Path::new("gamma/lib.rs"), Language::Rust, 0, 0, None)
            .expect("gamma");

        // hub → alpha: 1 file edge
        index
            .insert_file_dependency(f_hub_a, f_alpha)
            .expect("hub→alpha");
        // hub → beta: 3 file edges
        index
            .insert_file_dependency(f_hub_a, f_beta_1)
            .expect("hub→beta1");
        index
            .insert_file_dependency(f_hub_b, f_beta_2)
            .expect("hub→beta2");
        index
            .insert_file_dependency(f_hub_c, f_beta_3)
            .expect("hub→beta3");
        // hub → gamma: 1 file edge
        index
            .insert_file_dependency(f_hub_a, f_gamma)
            .expect("hub→gamma");

        let packages = [
            PackageInsert {
                name: "hub",
                path: "hub",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "alpha",
                path: "alpha",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "beta",
                path: "beta",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "gamma",
                path: "gamma",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
        ];
        let mappings = [
            (f_hub_a, "hub"),
            (f_hub_b, "hub"),
            (f_hub_c, "hub"),
            (f_alpha, "alpha"),
            (f_beta_1, "beta"),
            (f_beta_2, "beta"),
            (f_beta_3, "beta"),
            (f_gamma, "gamma"),
        ];
        index
            .repopulate_architecture(&packages, &mappings)
            .expect("repopulate");

        let detail = index
            .get_package_coupling("hub")
            .expect("query")
            .expect("found");
        let names: Vec<_> = detail
            .outgoing
            .iter()
            .map(|d| d.package.name.as_str())
            .collect();
        let counts: Vec<_> = detail.outgoing.iter().map(|d| d.dep_count).collect();

        assert_eq!(
            names,
            ["beta", "alpha", "gamma"],
            "dep_count DESC then name ASC"
        );
        assert_eq!(
            counts,
            [3, 1, 1],
            "beta has 3 edges, alpha and gamma have 1 each"
        );
    }

    #[test]
    fn fetch_neighbors_sorts_incoming_by_dep_count_desc_then_name_asc() {
        // Fixture: package "hub" has three packages depending on it with varying edge counts.
        //   alpha → hub: 1 edge   (tied with gamma; alpha < gamma alphabetically)
        //   beta  → hub: 3 edges  (highest dep_count → first)
        //   gamma → hub: 1 edge   (tied with alpha)
        //
        // Expected incoming order: [beta, alpha, gamma].
        let dir = tempfile::tempdir().expect("temp dir");
        let mut index = Index::open(&dir.path().join("idx.db")).expect("open");

        let f_hub = index
            .upsert_file(Path::new("hub/lib.rs"), Language::Rust, 0, 0, None)
            .expect("hub");
        let f_alpha = index
            .upsert_file(Path::new("alpha/lib.rs"), Language::Rust, 0, 0, None)
            .expect("alpha");
        let f_beta_1 = index
            .upsert_file(Path::new("beta/a.rs"), Language::Rust, 0, 0, None)
            .expect("beta 1");
        let f_beta_2 = index
            .upsert_file(Path::new("beta/b.rs"), Language::Rust, 0, 0, None)
            .expect("beta 2");
        let f_beta_3 = index
            .upsert_file(Path::new("beta/c.rs"), Language::Rust, 0, 0, None)
            .expect("beta 3");
        let f_gamma = index
            .upsert_file(Path::new("gamma/lib.rs"), Language::Rust, 0, 0, None)
            .expect("gamma");

        // alpha → hub: 1 edge
        index
            .insert_file_dependency(f_alpha, f_hub)
            .expect("alpha→hub");
        // beta → hub: 3 edges (three files in beta each reference hub)
        index
            .insert_file_dependency(f_beta_1, f_hub)
            .expect("beta1→hub");
        index
            .insert_file_dependency(f_beta_2, f_hub)
            .expect("beta2→hub");
        index
            .insert_file_dependency(f_beta_3, f_hub)
            .expect("beta3→hub");
        // gamma → hub: 1 edge
        index
            .insert_file_dependency(f_gamma, f_hub)
            .expect("gamma→hub");

        let packages = [
            PackageInsert {
                name: "hub",
                path: "hub",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "alpha",
                path: "alpha",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "beta",
                path: "beta",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "gamma",
                path: "gamma",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
        ];
        let mappings = [
            (f_hub, "hub"),
            (f_alpha, "alpha"),
            (f_beta_1, "beta"),
            (f_beta_2, "beta"),
            (f_beta_3, "beta"),
            (f_gamma, "gamma"),
        ];
        index
            .repopulate_architecture(&packages, &mappings)
            .expect("repopulate");

        let detail = index
            .get_package_coupling("hub")
            .expect("query")
            .expect("found");
        let names: Vec<_> = detail
            .incoming
            .iter()
            .map(|d| d.package.name.as_str())
            .collect();
        let counts: Vec<_> = detail.incoming.iter().map(|d| d.dep_count).collect();

        assert_eq!(
            names,
            ["beta", "alpha", "gamma"],
            "dep_count DESC then name ASC"
        );
        assert_eq!(
            counts,
            [3, 1, 1],
            "beta has 3 incoming, alpha and gamma have 1 each"
        );
    }

    #[test]
    fn get_package_coupling_returns_err_for_corrupt_target_source() {
        use rusqlite::Connection;

        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("idx.db");

        // Open via Index to apply schema, then drop and reopen raw to bypass CHECK.
        {
            let _ = Index::open(&db_path).expect("schema");
        }
        let raw = Connection::open(&db_path).expect("raw open");
        raw.execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             INSERT INTO arch_packages (id, name, path, source)
             VALUES (1, 'broken', 'broken', 'totally-bogus');",
        )
        .expect("inject corrupt row");

        // Verify the corrupt row actually landed — guards against a future SQLite
        // version silently no-op'ing `PRAGMA ignore_check_constraints`, which would
        // otherwise make this test pass for the wrong reason (target row absent,
        // so any Err — even an unrelated one — satisfies the loose assertion).
        let landed: String = raw
            .query_row(
                "SELECT source FROM arch_packages WHERE name = 'broken'",
                [],
                |row| row.get(0),
            )
            .expect("corrupt row must exist after INSERT");
        assert_eq!(landed, "totally-bogus", "PRAGMA bypass must have worked");
        drop(raw);

        // Now query via Index — should return Err::Internal mentioning the bogus value.
        let index = Index::open(&db_path).expect("reopen");
        let err = index
            .get_package_coupling("broken")
            .expect_err("corrupt target source should return Err");
        assert!(
            matches!(err, crate::Error::Internal(_)),
            "expected Error::Internal for corrupt source, got {err:?}"
        );
    }

    /// A corrupt neighbour is omitted without losing a valid neighbour.
    /// The documented contract is "silent skip + warn! log": the corrupt
    /// neighbour is absent from the list *and* the skip is observable. If the
    /// warn! is removed, this fails on the log assertion rather than silently
    /// becoming "completely silent".
    #[traced_test]
    #[test]
    fn fetch_neighbors_skips_neighbors_with_corrupt_source() {
        use rusqlite::Connection;

        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("idx.db");

        // Set up two packages where the target is valid and one neighbour is corrupt.
        let (f_target, f_valid, f_bad) = {
            let mut index = Index::open(&db_path).expect("schema");
            let f_target = index
                .upsert_file(Path::new("target/lib.rs"), Language::Rust, 0, 0, None)
                .expect("target");
            let f_valid = index
                .upsert_file(Path::new("valid/lib.rs"), Language::Rust, 0, 0, None)
                .expect("valid");
            let f_bad = index
                .upsert_file(Path::new("bad/lib.rs"), Language::Rust, 0, 0, None)
                .expect("bad");
            // target → valid AND target → bad
            index
                .insert_file_dependency(f_target, f_valid)
                .expect("→valid");
            index.insert_file_dependency(f_target, f_bad).expect("→bad");
            (f_target, f_valid, f_bad)
        };

        // Insert the three packages via raw SQL so we can plant a corrupt source.
        let raw = Connection::open(&db_path).expect("raw open");
        raw.execute_batch(&format!(
            "PRAGMA ignore_check_constraints = ON;
             DELETE FROM arch_packages;
             INSERT INTO arch_packages (id, name, path, source)
             VALUES (1, 'target', 'target', 'manifest');
             INSERT INTO arch_packages (id, name, path, source)
             VALUES (2, 'valid', 'valid', 'manifest');
             INSERT INTO arch_packages (id, name, path, source)
             VALUES (3, 'bad', 'bad', 'totally-bogus');
             INSERT INTO arch_file_packages (file_id, package_id)
             VALUES ({target_id}, 1);
             INSERT INTO arch_file_packages (file_id, package_id)
             VALUES ({valid_id}, 2);
             INSERT INTO arch_file_packages (file_id, package_id)
             VALUES ({bad_id}, 3);
             INSERT INTO arch_package_deps (source_pkg, target_pkg, dep_count)
             VALUES (1, 2, 1);
             INSERT INTO arch_package_deps (source_pkg, target_pkg, dep_count)
             VALUES (1, 3, 1);",
            target_id = f_target.as_i64(),
            valid_id = f_valid.as_i64(),
            bad_id = f_bad.as_i64(),
        ))
        .expect("seed");

        // Verify the corrupt row landed — see the same-shaped guard in
        // get_package_coupling_returns_err_for_corrupt_target_source for why.
        let landed: String = raw
            .query_row(
                "SELECT source FROM arch_packages WHERE name = 'bad'",
                [],
                |row| row.get(0),
            )
            .expect("corrupt neighbour row must exist after INSERT");
        assert_eq!(landed, "totally-bogus", "PRAGMA bypass must have worked");
        drop(raw);

        let index = Index::open(&db_path).expect("reopen");
        let detail = index
            .get_package_coupling("target")
            .expect("target query should succeed (its source is valid)")
            .expect("target package exists");

        // Neighbour 'valid' should appear; 'bad' should be silently skipped.
        let neighbour_names: Vec<_> = detail
            .outgoing
            .iter()
            .map(|d| d.package.name.as_str())
            .collect();
        assert_eq!(
            neighbour_names,
            ["valid"],
            "corrupt-source neighbour should be skipped from outgoing list"
        );
        assert!(
            logs_contain("unknown source value"),
            "expected a warn! log mentioning the corrupt source value"
        );
    }
}

#[cfg(test)]
mod coupling_metrics_tests {
    use super::*;
    use crate::types::Language;
    use std::path::Path;
    use tempfile::TempDir;

    /// Three-crate fixture: a → b, a → c, b → c.
    fn seeded_index() -> (TempDir, Index) {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut index = Index::open(&dir.path().join("idx.db")).expect("open");

        let f_a = index
            .upsert_file(Path::new("a/lib.rs"), Language::Rust, 0, 0, None)
            .expect("a");
        let f_b = index
            .upsert_file(Path::new("b/lib.rs"), Language::Rust, 0, 0, None)
            .expect("b");
        let f_c = index
            .upsert_file(Path::new("c/lib.rs"), Language::Rust, 0, 0, None)
            .expect("c");

        index.insert_file_dependency(f_a, f_b).expect("a→b");
        index.insert_file_dependency(f_a, f_c).expect("a→c");
        index.insert_file_dependency(f_b, f_c).expect("b→c");

        let packages = [
            PackageInsert {
                name: "a",
                path: "a",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "b",
                path: "b",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "c",
                path: "c",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
        ];
        let mappings = [(f_a, "a"), (f_b, "b"), (f_c, "c")];
        index
            .repopulate_architecture(&packages, &mappings)
            .expect("repopulate");

        (dir, index)
    }

    fn metrics_for<'a>(rows: &'a [CouplingMetrics], name: &str) -> &'a CouplingMetrics {
        rows.iter()
            .find(|m| m.package.name == name)
            .unwrap_or_else(|| panic!("no row for {name}"))
    }

    #[test]
    fn coupling_metrics_match_expected_ca_ce_instability() {
        let (_dir, index) = seeded_index();
        let rows = index
            .get_coupling_metrics(CouplingSort::Name)
            .expect("metrics");

        let a = metrics_for(&rows, "a");
        assert_eq!(
            (a.afferent, a.efferent),
            (MetricEvidence::Known(0), MetricEvidence::Known(2))
        );
        assert_eq!(a.instability(), MetricEvidence::Known(1.0));

        let b = metrics_for(&rows, "b");
        assert_eq!(
            (b.afferent, b.efferent),
            (MetricEvidence::Known(1), MetricEvidence::Known(1))
        );
        assert_eq!(b.instability(), MetricEvidence::Known(0.5));

        let c = metrics_for(&rows, "c");
        assert_eq!(
            (c.afferent, c.efferent),
            (MetricEvidence::Known(2), MetricEvidence::Known(0))
        );
        assert_eq!(c.instability(), MetricEvidence::Known(0.0));
    }

    #[test]
    fn sort_by_instability_descending() {
        let (_dir, index) = seeded_index();
        let rows = index
            .get_coupling_metrics(CouplingSort::Instability)
            .expect("metrics");
        assert_eq!(rows[0].package.name, "a");
        assert_eq!(rows[1].package.name, "b");
        assert_eq!(rows[2].package.name, "c");
    }

    #[test]
    fn sort_by_name_ascending() {
        let (_dir, index) = seeded_index();
        let rows = index
            .get_coupling_metrics(CouplingSort::Name)
            .expect("metrics");
        assert_eq!(rows[0].package.name, "a");
        assert_eq!(rows[1].package.name, "b");
        assert_eq!(rows[2].package.name, "c");
    }

    #[test]
    fn afferent_and_efferent_sorts_break_ties_by_name_ascending() {
        // Fixture: 4 packages where Ca pairs (x=2, y=2) and (w=0, z=0) tie, and
        // Ce pairs (w=2, z=2) and (x=0, y=0) tie. The secondary key
        // `.then_with(|| a.name.cmp(&b.name))` must order each tied pair
        // alphabetically, regardless of insertion order.
        let dir = tempfile::tempdir().expect("temp dir");
        let mut index = Index::open(&dir.path().join("idx.db")).expect("open");

        let f_w = index
            .upsert_file(Path::new("w/lib.rs"), Language::Rust, 0, 0, None)
            .expect("w");
        let f_x = index
            .upsert_file(Path::new("x/lib.rs"), Language::Rust, 0, 0, None)
            .expect("x");
        let f_y = index
            .upsert_file(Path::new("y/lib.rs"), Language::Rust, 0, 0, None)
            .expect("y");
        let f_z = index
            .upsert_file(Path::new("z/lib.rs"), Language::Rust, 0, 0, None)
            .expect("z");

        for (from, to) in [(f_w, f_x), (f_w, f_y), (f_z, f_x), (f_z, f_y)] {
            index.insert_file_dependency(from, to).expect("dep");
        }

        // Deliberately insert in non-alphabetical order to prove the sort
        // (not the storage order) is what ties get broken on.
        let packages = [
            PackageInsert {
                name: "z",
                path: "z",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "x",
                path: "x",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "w",
                path: "w",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "y",
                path: "y",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
        ];
        index
            .repopulate_architecture(&packages, &[(f_w, "w"), (f_x, "x"), (f_y, "y"), (f_z, "z")])
            .expect("repopulate");

        let afferent_sorted = index
            .get_coupling_metrics(CouplingSort::Afferent)
            .expect("ca");
        let afferent_names: Vec<_> = afferent_sorted
            .iter()
            .map(|m| m.package.name.as_str())
            .collect();
        assert_eq!(
            afferent_names,
            ["x", "y", "w", "z"],
            "Ca-descending then name-ascending breaks ties alphabetically"
        );

        let efferent_sorted = index
            .get_coupling_metrics(CouplingSort::Efferent)
            .expect("ce");
        let efferent_names: Vec<_> = efferent_sorted
            .iter()
            .map(|m| m.package.name.as_str())
            .collect();
        assert_eq!(
            efferent_names,
            ["w", "z", "x", "y"],
            "Ce-descending then name-ascending breaks ties alphabetically"
        );
    }

    #[test]
    fn isolated_package_has_zero_instability() {
        let dir = tempfile::tempdir().expect("temp dir");
        let index = Index::open(&dir.path().join("idx.db")).expect("open");
        let packages = [PackageInsert {
            name: "lonely",
            path: "lonely",
            source: PackageSource::Manifest,
            evaluation_unit_key: None,
        }];
        index
            .repopulate_architecture(&packages, &[])
            .expect("repopulate");

        let rows = index
            .get_coupling_metrics(CouplingSort::Name)
            .expect("metrics");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].afferent, rows[0].efferent),
            (MetricEvidence::Known(0), MetricEvidence::Known(0))
        );
        assert_eq!(rows[0].instability(), MetricEvidence::Known(0.0));
    }
}

#[cfg(test)]
mod repopulate_tests {
    use super::*;
    use crate::types::Language;
    use std::path::Path;
    use tempfile::TempDir;

    fn temp_index() -> (TempDir, Index) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("idx.db");
        let index = Index::open(&path).expect("open index");
        (dir, index)
    }

    /// Inserts a file and returns its `FileId`.
    fn add_file(index: &mut Index, rel_path: &str) -> FileId {
        index
            .upsert_file(Path::new(rel_path), Language::Rust, 0, 0, None)
            .expect("upsert file")
    }

    #[test]
    fn repopulate_architecture_inserts_packages() {
        let (_dir, index) = temp_index();

        let packages = [
            PackageInsert {
                name: "crate_a",
                path: "crate_a",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "crate_b",
                path: "crate_b",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
        ];

        let stats = index
            .repopulate_architecture(&packages, &[])
            .expect("repopulate");

        assert_eq!(stats.packages_recorded, 2);
        assert_eq!(stats.files_assigned, 0);
        assert_eq!(stats.package_deps_recorded, 0);
    }

    #[test]
    fn repopulate_architecture_assigns_files_and_deps() {
        let (_dir, mut index) = temp_index();

        let f_a = add_file(&mut index, "crate_a/lib.rs");
        let f_b = add_file(&mut index, "crate_b/lib.rs");
        let f_c = add_file(&mut index, "crate_c/lib.rs");

        // crate_a depends on crate_b and crate_c; crate_b depends on crate_c.
        index.insert_file_dependency(f_a, f_b).expect("dep a→b");
        index.insert_file_dependency(f_a, f_c).expect("dep a→c");
        index.insert_file_dependency(f_b, f_c).expect("dep b→c");

        let packages = [
            PackageInsert {
                name: "crate_a",
                path: "crate_a",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "crate_b",
                path: "crate_b",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
            PackageInsert {
                name: "crate_c",
                path: "crate_c",
                source: PackageSource::Manifest,
                evaluation_unit_key: None,
            },
        ];

        let mappings = [(f_a, "crate_a"), (f_b, "crate_b"), (f_c, "crate_c")];

        let stats = index
            .repopulate_architecture(&packages, &mappings)
            .expect("repopulate");

        assert_eq!(stats.packages_recorded, 3);
        assert_eq!(stats.files_assigned, 3);
        assert_eq!(stats.package_deps_recorded, 3, "a→b, a→c, b→c");
    }

    #[test]
    fn repopulate_architecture_is_idempotent() {
        let (_dir, mut index) = temp_index();
        let f = add_file(&mut index, "crate_a/lib.rs");
        let packages = [PackageInsert {
            name: "crate_a",
            path: "crate_a",
            source: PackageSource::Manifest,
            evaluation_unit_key: None,
        }];
        let mappings = [(f, "crate_a")];

        let s1 = index
            .repopulate_architecture(&packages, &mappings)
            .expect("first");
        let s2 = index
            .repopulate_architecture(&packages, &mappings)
            .expect("second");

        assert_eq!(s1, s2);
    }

    #[test]
    fn repopulate_architecture_skips_unknown_package_names() {
        let (_dir, mut index) = temp_index();
        let f = add_file(&mut index, "orphan.rs");
        let packages = [PackageInsert {
            name: "crate_a",
            path: "crate_a",
            source: PackageSource::Manifest,
            evaluation_unit_key: None,
        }];
        // file_to_package_name references a package not in `packages`.
        let mappings = [(f, "missing_crate")];

        let stats = index
            .repopulate_architecture(&packages, &mappings)
            .expect("repopulate");

        assert_eq!(stats.packages_recorded, 1);
        assert_eq!(stats.files_assigned, 0, "unknown name skipped");
    }
}

fn compare_evidence<T>(
    a: MetricEvidence<T>,
    b: MetricEvidence<T>,
    compare: impl FnOnce(&T, &T) -> std::cmp::Ordering,
) -> std::cmp::Ordering {
    use MetricEvidence::{Indeterminate, Known};
    match (a, b) {
        (Known(a), Known(b)) => compare(&b, &a),
        (Known(_), Indeterminate(_)) => std::cmp::Ordering::Less,
        (Indeterminate(_), Known(_)) => std::cmp::Ordering::Greater,
        (Indeterminate(_), Indeterminate(_)) => std::cmp::Ordering::Equal,
    }
}

fn decode_evidence<T: serde::de::DeserializeOwned>(text: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|error| {
        crate::Error::Internal(format!("corrupt coupling discovery metadata: {error}"))
    })
}

/// Whether any discovery evidence is missing anywhere in the workspace.
///
/// Deliberately workspace-global: a traversal issue can hide a project or an
/// input that no surviving row mentions, so no narrower scope can rule out an
/// unconfirmed edge. Counted in SQL, so a targeted read never has to decode
/// every unit to answer a single-package question.
fn discovery_is_incomplete(conn: &Connection) -> Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM discovery_issues)
             OR EXISTS(SELECT 1 FROM projects
                       WHERE json_extract(standing_json, '$.standing') <> 'confirmed')
             OR EXISTS(SELECT 1 FROM evaluation_units
                       WHERE json_extract(standing_json, '$.standing') <> 'confirmed')",
        [],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

/// Projects that own a confirmed evaluation unit, workspace-wide.
fn selected_projects(conn: &Connection) -> Result<std::collections::HashSet<ProjectKey>> {
    let mut statement = conn.prepare(
        "SELECT DISTINCT project_key FROM evaluation_units
         WHERE json_extract(standing_json, '$.standing') = 'confirmed'",
    )?;
    let rows = statement.query_map([], |row| Ok(ProjectKey(row.get(0)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Targets named by a contributing declaration, workspace-wide.
///
/// `ReferenceOutputAssembly = false` is the same non-contribution rule the
/// in-memory check uses, applied here so a targeted read still sees every
/// declaration in the workspace.
fn declared_targets(conn: &Connection) -> Result<std::collections::HashSet<ProjectKey>> {
    let mut statement = conn.prepare(
        "SELECT DISTINCT target_project_key FROM declared_project_references AS declaration
         WHERE NOT EXISTS (
             SELECT 1 FROM json_each(declaration.metadata_json) AS entry
             WHERE lower(entry.key) = 'referenceoutputassembly'
               AND lower(trim(entry.value)) = 'false'
         )",
    )?;
    let rows = statement.query_map([], |row| Ok(ProjectKey(row.get(0)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// One coupling row per package.
const COUPLING_SQL: &str = "SELECT p.id, p.name, p.path, p.source, c.afferent, c.efferent, p.evaluation_unit_key \
     FROM arch_coupling c JOIN arch_packages p ON p.id = c.package_id";

/// Collect coupling rows, narrowed to one package when a target is given.
///
/// The targeted form is an indexed single-row fetch: it must not scan, and must
/// not decode, rows that belong to other packages.
fn collect_metrics(
    conn: &Connection,
    target: Option<&str>,
) -> Result<(Vec<CouplingMetrics>, Vec<Option<String>>)> {
    // The first SELECT pins the caller's savepoint snapshot. All projections and
    // neighbor reads stay on that same transaction even against WAL writers.
    let mut metrics = Vec::new();
    let mut keys = Vec::new();
    let mut statement = match target {
        Some(_) => conn.prepare(&format!("{COUPLING_SQL} WHERE p.name = ?1"))?,
        None => conn.prepare(COUPLING_SQL)?,
    };
    let mut rows = match target {
        Some(name) => statement.query([name])?,
        None => statement.query([])?,
    };
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        let source_text: String = row.get(3)?;
        let Some(source) = PackageSource::parse(&source_text) else {
            if target.is_some() {
                return Err(crate::Error::Internal(format!(
                    "package '{name}' has unknown source value '{source_text}'"
                )));
            }
            tracing::warn!(
                package_name = name,
                source = source_text,
                "skipping coupling row with unknown source"
            );
            continue;
        };
        let afferent =
            MetricEvidence::Known(saturating_coupling_to_u32(row.get(4)?, &name, "afferent"));
        let efferent =
            MetricEvidence::Known(saturating_coupling_to_u32(row.get(5)?, &name, "efferent"));
        keys.push(row.get::<_, Option<String>>(6)?);
        metrics.push(CouplingMetrics {
            package: Package {
                id: PackageId::new(row.get(0)?),
                name,
                path: std::path::PathBuf::from(row.get::<_, String>(2)?),
                source,
            },
            afferent,
            efferent,
            evaluation_unit: None,
        });
    }
    Ok((metrics, keys))
}

/// Load persisted evidence for exactly the requested units.
fn read_units(
    conn: &Connection,
    keys: &[String],
) -> Result<HashMap<String, EvaluationUnitCoupling>> {
    let mut units: HashMap<String, EvaluationUnitCoupling> = HashMap::new();
    if keys.is_empty() {
        return Ok(units);
    }
    let placeholders = vec!["?"; keys.len()].join(",");
    {
        let statement_sql = format!(
            "SELECT unit_key, project_key, target_framework, framework_json, standing_json, \
             json_extract(properties_json, '$.AssemblyName') FROM evaluation_units \
             WHERE unit_key IN ({placeholders}) ORDER BY ordinal"
        );
        let mut statement = conn.prepare(&statement_sql)?;
        let mut rows = statement.query(rusqlite::params_from_iter(keys.iter()))?;
        while let Some(row) = rows.next()? {
            let key: String = row.get(0)?;
            units.insert(
                key.clone(),
                EvaluationUnitCoupling {
                    key: EvaluationUnitKey(key),
                    project: ProjectKey(row.get(1)?),
                    target_framework: row.get(2)?,
                    framework: decode_evidence(coupling_text(row, 3)?)?,
                    standing: decode_evidence(coupling_text(row, 4)?)?,
                    assembly_name: row.get(5)?,
                    declared_references: Vec::new(),
                    unresolved_assembly_references: false,
                },
            );
        }
    }
    {
        let statement_sql = format!(
            "SELECT unit_key, target_project_key, include, metadata_json FROM \
             declared_project_references WHERE unit_key IN ({placeholders}) \
             ORDER BY unit_key, ordinal"
        );
        let mut statement = conn.prepare(&statement_sql)?;
        let mut rows = statement.query(rusqlite::params_from_iter(keys.iter()))?;
        while let Some(row) = rows.next()? {
            let key = coupling_text(row, 0)?;
            units
                .get_mut(key)
                .ok_or_else(|| {
                    crate::Error::Internal(format!("orphaned coupling declaration: {key}"))
                })?
                .declared_references
                .push(DeclaredProjectReference {
                    target: ProjectKey(row.get(1)?),
                    include: row.get(2)?,
                    metadata: decode_evidence(coupling_text(row, 3)?)?,
                });
        }
    }
    {
        let statement_sql = format!(
            "SELECT unit_key, metadata_json FROM declared_assembly_references \
             WHERE unit_key IN ({placeholders}) ORDER BY unit_key, ordinal"
        );
        let mut statement = conn.prepare(&statement_sql)?;
        let mut rows = statement.query(rusqlite::params_from_iter(keys.iter()))?;
        while let Some(row) = rows.next()? {
            let key = coupling_text(row, 0)?;
            let metadata: std::collections::BTreeMap<String, String> =
                decode_evidence(coupling_text(row, 1)?)?;
            if !crate::architecture::contributes_metadata(&metadata) {
                continue;
            }
            units
                .get_mut(key)
                .ok_or_else(|| {
                    crate::Error::Internal(format!("orphaned coupling declaration: {key}"))
                })?
                .unresolved_assembly_references = true;
        }
    }
    Ok(units)
}

fn read_metrics(conn: &Connection, strict_target: Option<&str>) -> Result<Vec<CouplingMetrics>> {
    let (mut metrics, keys) = collect_metrics(conn, strict_target)?;
    if metrics.is_empty() {
        return Ok(metrics);
    }
    let incomplete = discovery_is_incomplete(conn)?;
    let context = DeclaredContext {
        selected: selected_projects(conn)?,
        declared_targets: declared_targets(conn)?,
    };
    let requested: Vec<String> = keys.iter().flatten().cloned().collect();
    let mut units = read_units(conn, &requested)?;
    for (metric, key) in metrics.iter_mut().zip(keys) {
        if let Some(key) = key {
            metric.evaluation_unit = Some(units.remove(&key).ok_or_else(|| {
                crate::Error::Internal(format!("missing coupling unit metadata: {key}"))
            })?);
        }
    }
    apply_evidence(&mut metrics, incomplete, &context);
    Ok(metrics)
}

fn coupling_text<'a>(row: &'a rusqlite::Row<'_>, column: usize) -> rusqlite::Result<&'a str> {
    let value = row.get_ref(column)?;
    value.as_str().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, value.data_type(), Box::new(error))
    })
}

#[cfg(test)]
mod coupling_snapshot_tests {
    use super::*;
    use crate::architecture::CouplingIndeterminacy;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, mpsc};
    use std::time::Duration;

    static BARRIER: Mutex<Option<ReadBarrier>> = Mutex::new(None);
    static COMMITS: AtomicUsize = AtomicUsize::new(0);
    struct ReadBarrier {
        start: mpsc::Sender<()>,
        committed: mpsc::Receiver<()>,
    }

    fn snapshot_trace(sql: &str) {
        if !sql.contains("FROM evaluation_units") {
            return;
        }
        let Some(barrier) = BARRIER.lock().expect("barrier mutex").take() else {
            return;
        };
        barrier.start.send(()).expect("release independent writer");
        barrier
            .committed
            .recv_timeout(Duration::from_secs(5))
            .expect("writer must commit while composite reader is pinned");
    }

    #[test]
    fn coupling_detail_uses_one_snapshot_across_wal_publication() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("index.db");
        let index = Index::open(&path).expect("index");
        {
            let conn = index.connection().expect("connection");
            conn.execute_batch(r#"
                INSERT INTO projects VALUES ('App.csproj', 0, '[]', '{"standing":"confirmed"}');
                INSERT INTO evaluation_units VALUES ('app-unit', 'App.csproj', 0, 'net8.0', 'null', '{"standing":"confirmed"}', '{"AssemblyName":"Before"}', 'null', '{"style":"none","inputs":[]}');
                INSERT INTO arch_packages (id, name, path, source, evaluation_unit_key) VALUES (1, 'msbuild:App.csproj:app-unit', 'before', 'msbuild', 'app-unit');
                INSERT INTO arch_packages (id, name, path, source) VALUES (2, 'dependency', 'dep', 'manifest');
            "#).expect("old revision");
        }
        let (start, started) = mpsc::channel();
        let (committed, completed) = mpsc::channel();
        COMMITS.store(0, Ordering::SeqCst);
        let writer = std::thread::spawn(move || {
            let mut conn = Connection::open(path).expect("independent WAL writer");
            conn.busy_timeout(Duration::from_secs(3))
                .expect("bounded writer");
            started
                .recv_timeout(Duration::from_secs(5))
                .expect("reader reaches component boundary");
            let tx = conn.transaction().expect("writer transaction");
            tx.execute_batch(r#"
                UPDATE evaluation_units SET properties_json = '{"AssemblyName":"After"}' WHERE unit_key = 'app-unit';
                UPDATE arch_packages SET path = 'after' WHERE id = 1;
                INSERT INTO arch_package_deps VALUES (1, 2, 7);
            "#).expect("new revision");
            tx.commit().expect("publish new revision");
            COMMITS.fetch_add(1, Ordering::SeqCst);
            committed
                .send(())
                .expect("notify reader of committed revision");
        });
        *BARRIER.lock().expect("barrier mutex") = Some(ReadBarrier {
            start,
            committed: completed,
        });
        index
            .connection()
            .expect("connection")
            .trace(Some(snapshot_trace));
        let before = index
            .get_package_coupling("msbuild:App.csproj:app-unit")
            .expect("detail")
            .expect("unit");
        index.connection().expect("connection").trace(None);
        writer.join().expect("writer succeeds");
        assert_eq!(COMMITS.load(Ordering::SeqCst), 1, "writer commit canary");
        assert!(BARRIER.lock().expect("barrier mutex").is_none());
        assert_eq!(before.metrics.package.path, std::path::Path::new("before"));
        // An evaluation unit carries no file attribution, so a structural zero
        // is published as unknown rather than as a measurement.
        assert_eq!(
            (before.metrics.afferent, before.metrics.efferent),
            (
                MetricEvidence::Indeterminate(CouplingIndeterminacy::UnattributedEvaluationUnit),
                MetricEvidence::Indeterminate(CouplingIndeterminacy::UnattributedEvaluationUnit)
            )
        );
        assert_eq!(
            before
                .metrics
                .evaluation_unit
                .expect("old metadata")
                .assembly_name
                .as_deref(),
            Some("Before")
        );
        assert!(before.incoming.is_empty());
        assert!(before.outgoing.is_empty());
        let after = index
            .get_package_coupling("msbuild:App.csproj:app-unit")
            .expect("new detail")
            .expect("unit");
        assert_eq!(after.metrics.package.path, std::path::Path::new("after"));
        // A real graph edge is evidence: the measured count survives even
        // though this unit still has no file attribution.
        assert_eq!(
            (after.metrics.afferent, after.metrics.efferent),
            (
                MetricEvidence::Indeterminate(CouplingIndeterminacy::UnattributedEvaluationUnit),
                MetricEvidence::Known(1)
            )
        );
        assert_eq!(
            after
                .metrics
                .evaluation_unit
                .expect("new metadata")
                .assembly_name
                .as_deref(),
            Some("After")
        );
        assert!(after.incoming.is_empty());
        assert_eq!(
            after
                .outgoing
                .iter()
                .map(|edge| (edge.package.name.as_str(), edge.dep_count))
                .collect::<Vec<_>>(),
            [("dependency", 7)]
        );
    }
}
