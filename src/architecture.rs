//! Architecture records, evidence propagation and publication assembly.

use crate::discovery::{
    DeclaredProjectReference, DiscoveryStanding, EvaluationUnitKey, FrameworkIdentity, ProjectKey,
};
use crate::{Result, Tethys};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Availability of a coupling metric; unavailable evidence is never numeric zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricEvidence<T> {
    /// Evidence establishes this value.
    Known(T),
    /// Required evidence is unavailable.
    Indeterminate(CouplingIndeterminacy),
}

/// Why a coupling metric cannot be established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CouplingIndeterminacy {
    /// Discovery did not establish complete coverage.
    IncompleteDiscovery,
    /// A contributing declaration has no compiler-selected target unit.
    UnselectedProjectReference,
}

/// Lightweight persisted evidence for one architecture evaluation-unit node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationUnitCoupling {
    /// Exact evaluation-unit identity.
    pub key: EvaluationUnitKey,
    /// Owning project-file identity.
    pub project: ProjectKey,
    /// Requested SDK framework selector.
    pub target_framework: Option<String>,
    /// Full evaluated framework identity.
    pub framework: Option<FrameworkIdentity>,
    /// Discovery completeness and failure evidence.
    pub standing: DiscoveryStanding,
    /// Evaluated assembly name, not a node identity.
    pub assembly_name: Option<String>,
    /// Declarations including noncontributing references, never selected edges.
    pub declared_references: Vec<DeclaredProjectReference>,
}

/// Outcome of the architecture-analysis indexing phase.
///
/// `None` on `IndexStats::arch_phase` means the phase has not yet run.
/// Current indexing records `Completed` after success and returns errors
/// directly on failure so publication can roll back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchPhaseResult {
    /// The phase completed successfully, including an empty architecture graph.
    Completed(ArchStats),
    /// The phase failed. The string carries the Display form of the error.
    Failed(String),
}

/// Internal numeric ID for a package row. Mirrors the `FileId` / `SymbolId` pattern.
///
/// This is the database rowid for `arch_packages` and is stable only within
/// a single index lifetime — `tethys index --rebuild` (or any other path
/// that clears `arch_packages`) reassigns these values. External consumers
/// that need stable cross-run identity should use [`Package::name`] instead,
/// which is enforced unique per workspace at the schema level.
///
/// `PackageId` is part of the public API for symmetry with `FileId` /
/// `SymbolId`, but no public method takes one as input. Treat it as opaque.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageId(i64);

impl PackageId {
    /// Return the underlying `i64` value.
    #[must_use]
    pub fn as_i64(self) -> i64 {
        self.0
    }

    /// Construct a `PackageId` from a raw database rowid.
    ///
    /// Prefer obtaining `PackageId` values from [`Package::id`] on records
    /// returned by the API rather than constructing them directly. This
    /// constructor exists for the DB layer and for test fixtures (including
    /// the tethys binary crate's own unit tests, which is why this is `pub`
    /// rather than `pub(crate)`). Hidden from rustdoc — external callers who
    /// fabricate IDs bypass the opaque-type contract described in the
    /// type-level docs.
    #[doc(hidden)]
    #[must_use]
    pub fn new(id: i64) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for PackageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How a package was discovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PackageSource {
    /// Discovered via Cargo.toml.
    Manifest,
    /// An evaluated or explicitly failed `MSBuild` unit.
    MsBuild,
    /// Directory-fallback for files outside any manifest. Reserved for future use.
    Directory,
}

impl PackageSource {
    /// Stable string form used in SQL storage.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            PackageSource::Manifest => "manifest",
            PackageSource::MsBuild => "msbuild",
            PackageSource::Directory => "directory",
        }
    }

    /// Inverse of `as_str`. Returns `None` for unknown values, which lets the
    /// caller decide whether to skip the row or surface a warning.
    #[must_use]
    pub fn parse(s: &str) -> Option<PackageSource> {
        match s {
            "manifest" => Some(PackageSource::Manifest),
            "msbuild" => Some(PackageSource::MsBuild),
            "directory" => Some(PackageSource::Directory),
            _ => None,
        }
    }
}

/// A crate or evaluation-unit architecture node, with a workspace-unique name.
#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    /// Database row ID.
    pub id: PackageId,
    /// Exact selector: Cargo name or `msbuild:<project>:<unit key>`.
    pub name: String,
    /// Path to the package root, relative to the workspace root.
    pub path: std::path::PathBuf,
    /// How this package was discovered.
    pub source: PackageSource,
}

/// Coupling metrics for one crate or evaluation-unit architecture node.
#[derive(Debug, Clone, PartialEq)]
pub struct CouplingMetrics {
    /// The package these metrics describe.
    pub package: Package,
    /// Afferent coupling: distinct packages depending on this one.
    pub afferent: MetricEvidence<u32>,
    /// Efferent coupling: distinct packages this one depends on.
    pub efferent: MetricEvidence<u32>,
    /// Persisted unit evidence, absent for Cargo packages.
    pub evaluation_unit: Option<EvaluationUnitCoupling>,
}

impl CouplingMetrics {
    /// Instability score: Ce / (Ca + Ce). Returns 0.0 when both Ca and Ce are zero.
    ///
    /// Martin's original formula is undefined at 0/0. We treat an isolated
    /// package (no incoming or outgoing edges) as maximally stable (I=0):
    /// with no consumers, there is no churn pressure on it. The alternative
    /// of NaN propagates poorly through sort and JSON output; the alternative
    /// of 1.0 would treat truly disconnected code as "maximally unstable",
    /// which is the opposite of the everyday meaning.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "denom is the sum of two u32 counts, so its maximum value is \
                  2 × u32::MAX ≈ 8.6 billion. f64 can represent all integers \
                  up to 2^53 ≈ 9 quadrillion exactly, so this cast never loses \
                  precision in practice. The lint fires on the cast syntax alone."
    )]
    pub fn instability(&self) -> MetricEvidence<f64> {
        use CouplingIndeterminacy::{IncompleteDiscovery, UnselectedProjectReference};
        use MetricEvidence::{Indeterminate, Known};
        match (self.afferent, self.efferent) {
            (Indeterminate(IncompleteDiscovery), _) | (_, Indeterminate(IncompleteDiscovery)) => {
                Indeterminate(IncompleteDiscovery)
            }
            (Indeterminate(UnselectedProjectReference), _)
            | (_, Indeterminate(UnselectedProjectReference)) => {
                Indeterminate(UnselectedProjectReference)
            }
            (Known(ca), Known(ce)) => {
                let denom = u64::from(ca) + u64::from(ce);
                Known(if denom == 0 {
                    0.0
                } else {
                    f64::from(ce) / denom as f64
                })
            }
        }
    }
}

/// Sort key for `get_coupling_metrics`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CouplingSort {
    /// Most unstable first.
    #[default]
    Instability,
    /// Most depended-on first.
    Afferent,
    /// Most dependent first.
    Efferent,
    /// Alphabetical.
    Name,
}

/// One package together with how many cross-package edges contribute to a relationship.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageDependency {
    /// The related package.
    pub package: Package,
    /// Number of file-level dependency edges between the two packages.
    pub dep_count: u32,
}

/// Detailed coupling for a single package, with incoming and outgoing edges.
#[derive(Debug, Clone, PartialEq)]
pub struct CouplingDetail {
    /// Coupling metrics for the queried package.
    pub metrics: CouplingMetrics,
    /// Packages that depend on this one.
    pub incoming: Vec<PackageDependency>,
    /// Packages this one depends on.
    pub outgoing: Vec<PackageDependency>,
}

/// Statistics emitted by the architecture indexing phase.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArchStats {
    /// Number of packages (crates) inserted into `arch_packages`.
    pub packages_recorded: usize,
    /// Number of files mapped to a package in `arch_file_packages`.
    pub files_assigned: usize,
    /// Number of distinct cross-package dependency pairs inserted into
    /// `arch_package_deps`. This is a count of unique (source, target) edges
    /// in the package graph, not a sum of underlying file-edge weights.
    pub package_deps_recorded: usize,
}

#[cfg(test)]
mod arch_type_tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn package_id_roundtrip() {
        let id = PackageId::new(42);
        assert_eq!(id.as_i64(), 42);
    }

    #[test]
    fn package_source_as_str_round_trips_through_parse() {
        for variant in [
            PackageSource::Manifest,
            PackageSource::Directory,
            PackageSource::MsBuild,
        ] {
            assert_eq!(PackageSource::parse(variant.as_str()), Some(variant));
        }
    }

    #[test]
    fn package_source_parse_returns_none_for_unknown() {
        assert_eq!(PackageSource::parse("git"), None);
    }

    fn metrics(name: &str, afferent: u32, efferent: u32) -> CouplingMetrics {
        CouplingMetrics {
            package: Package {
                id: PackageId::new(1),
                name: name.into(),
                path: name.into(),
                source: PackageSource::Manifest,
            },
            afferent: MetricEvidence::Known(afferent),
            efferent: MetricEvidence::Known(efferent),
            evaluation_unit: None,
        }
    }

    #[test]
    fn instability_preserves_large_counts_and_incomplete_precedence() {
        use CouplingIndeterminacy::{IncompleteDiscovery, UnselectedProjectReference};
        use MetricEvidence::{Indeterminate, Known};
        assert_eq!(
            metrics("large", u32::MAX, u32::MAX).instability(),
            Known(0.5)
        );
        let mut metric = metrics("unknown", 0, 0);
        metric.afferent = Indeterminate(UnselectedProjectReference);
        metric.efferent = Indeterminate(IncompleteDiscovery);
        assert_eq!(metric.instability(), Indeterminate(IncompleteDiscovery));
        std::mem::swap(&mut metric.afferent, &mut metric.efferent);
        assert_eq!(metric.instability(), Indeterminate(IncompleteDiscovery));
    }

    #[rstest]
    #[case::isolated_is_zero_not_nan(0, 0, 0.0_f64)]
    #[case::pure_efferent_is_one(0, 3, 1.0_f64)]
    #[case::pure_afferent_is_zero(3, 0, 0.0_f64)]
    fn instability_boundary_cases(
        #[case] afferent: u32,
        #[case] efferent: u32,
        #[case] expected: f64,
    ) {
        let i = metrics("p", afferent, efferent).instability();
        assert_eq!(i, MetricEvidence::Known(expected));
    }
}

/// Owned adapter output retained while the DB insert payload borrows it.
pub(crate) struct ArchitecturePackage {
    pub name: String,
    pub path: String,
    pub source: PackageSource,
    pub evaluation_unit_key: Option<EvaluationUnitKey>,
}

impl Tethys {
    /// Rebuild architecture inside the existing whole-run publication.
    pub(crate) fn run_architecture_phase(&self) -> Result<ArchStats> {
        let files = self.db.list_all_files()?;
        let (mut packages, assignments) =
            crate::cargo::architecture_inputs(self.crates(), &files, &self.workspace_root);
        packages.extend(crate::discovery::architecture_packages(
            &self.discovery_snapshot().units,
        ));
        let inserts: Vec<_> = packages
            .iter()
            .map(|package| crate::db::PackageInsert {
                name: &package.name,
                path: &package.path,
                source: package.source,
                evaluation_unit_key: package.evaluation_unit_key.as_ref(),
            })
            .collect();
        self.db.repopulate_architecture(&inserts, &assignments)
    }
}

/// Propagate unavailable evidence by project identity without expanding target units.
pub(crate) fn apply_evidence(metrics: &mut [CouplingMetrics], incomplete: bool) {
    use CouplingIndeterminacy::{IncompleteDiscovery, UnselectedProjectReference};
    use MetricEvidence::Indeterminate;
    let mut targets = HashSet::new();
    for metric in metrics.iter() {
        if let Some(unit) = &metric.evaluation_unit {
            for reference in &unit.declared_references {
                if contributes(reference) && !targets.contains(&reference.target) {
                    targets.insert(reference.target.clone());
                }
            }
        }
    }
    for metric in metrics {
        let Some(unit) = &metric.evaluation_unit else {
            continue;
        };
        if !matches!(unit.standing, DiscoveryStanding::Confirmed) {
            metric.afferent = Indeterminate(IncompleteDiscovery);
            metric.efferent = Indeterminate(IncompleteDiscovery);
            continue;
        }
        if incomplete {
            metric.afferent = Indeterminate(IncompleteDiscovery);
        } else if targets.contains(&unit.project) {
            metric.afferent = Indeterminate(UnselectedProjectReference);
        }
        if unit.declared_references.iter().any(contributes) {
            metric.efferent = Indeterminate(UnselectedProjectReference);
        }
    }
}

fn contributes(reference: &DeclaredProjectReference) -> bool {
    !reference.metadata.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("ReferenceOutputAssembly")
            && value.trim().eq_ignore_ascii_case("false")
    })
}

#[cfg(test)]
mod instability_property_tests {
    use crate::architecture::{CouplingMetrics, MetricEvidence, Package, PackageId, PackageSource};
    use proptest::prelude::*;
    use rusqlite::Connection;
    use std::path::PathBuf;

    /// Compute instability via the canonical `CouplingMetrics::instability()` so
    /// the proptest never drifts from the production formula.
    fn instability_of(afferent: u32, efferent: u32) -> f64 {
        let metrics = CouplingMetrics {
            package: Package {
                id: PackageId::new(0),
                name: String::new(),
                path: PathBuf::new(),
                source: PackageSource::Manifest,
            },
            afferent: MetricEvidence::Known(afferent),
            efferent: MetricEvidence::Known(efferent),
            evaluation_unit: None,
        };
        let MetricEvidence::Known(value) = metrics.instability() else {
            panic!("known counts")
        };
        value
    }

    /// Build an in-memory DB with `n` packages and the listed cross-package edges,
    /// then query `arch_coupling`. Edges are (`source_index`, `target_index`) pairs.
    ///
    /// Returns `(afferent, efferent, instability)` triples; instability is routed
    /// through `CouplingMetrics::instability()` so this helper can't silently drift
    /// from the production formula.
    fn instability_for(n: usize, edges: &[(usize, usize)]) -> Vec<(u32, u32, f64)> {
        let conn = Connection::open_in_memory().expect("open");
        // Match the prod Index::open setup so FK constraints (e.g. arch_file_packages
        // CASCADE deletes) are exercised under the same semantics in tests.
        conn.pragma_update(None, "foreign_keys", "ON")
            .expect("enable fks");
        conn.execute_batch(crate::db::SCHEMA).expect("schema");

        for i in 0..n {
            conn.execute(
                "INSERT INTO arch_packages (id, name, path, source) VALUES (?1, ?2, ?3, 'manifest')",
                rusqlite::params![i64::try_from(i + 1).expect("package index fits in i64"), format!("p{i}"), format!("p{i}")],
            )
            .expect("insert pkg");
        }
        for (src, tgt) in edges {
            if src == tgt {
                continue;
            }
            // INSERT OR IGNORE to dedupe (src, tgt) pairs (PK constraint).
            conn.execute(
                "INSERT OR IGNORE INTO arch_package_deps (source_pkg, target_pkg, dep_count)
                 VALUES (?1, ?2, 1)",
                rusqlite::params![
                    i64::try_from(src + 1).expect("source index fits in i64"),
                    i64::try_from(tgt + 1).expect("target index fits in i64")
                ],
            )
            .expect("insert dep");
        }

        let mut stmt = conn
            .prepare("SELECT afferent, efferent FROM arch_coupling")
            .expect("prepare");
        let rows: Vec<(u32, u32, f64)> = stmt
            .query_map([], |r| {
                Ok((
                    u32::try_from(r.get::<_, i64>(0)?).unwrap_or(u32::MAX),
                    u32::try_from(r.get::<_, i64>(1)?).unwrap_or(u32::MAX),
                ))
            })
            .expect("query")
            .map(|r| {
                let (ca, ce) = r.expect("row");
                (ca, ce, instability_of(ca, ce))
            })
            .collect();
        rows
    }

    proptest! {
        /// For every package and every random edge set, instability stays in [0, 1].
        #[test]
        fn instability_within_unit_interval(
            n in 1usize..8,
            edges in prop::collection::vec((0usize..8, 0usize..8), 0..30),
        ) {
            let edges: Vec<_> = edges.into_iter()
                .filter(|(s, t)| *s < n && *t < n)
                .collect();
            for (_ca, _ce, i) in instability_for(n, &edges) {
                prop_assert!((0.0..=1.0).contains(&i), "instability out of range: {i}");
            }
        }

        /// A package with no edges has instability exactly 0.
        #[test]
        fn isolated_package_has_zero_instability(n in 1usize..6) {
            let rows = instability_for(n, &[]);
            for (ca, ce, i) in rows {
                prop_assert_eq!((ca, ce), (0, 0));
                prop_assert!((i - 0.0_f64).abs() < 1e-9);
            }
        }
    }
}
