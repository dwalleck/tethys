//! Language-neutral, explicitly authorized workspace discovery.
//!
//! Discovery returns owned metadata and incomplete-coverage evidence. It does not
//! publish an index revision or grant evaluation/restore permission implicitly.

mod msbuild;
mod types;

pub use crate::cargo::CargoDiscovery;
pub use msbuild::MsBuildDiscovery;
pub use types::{
    DeclaredAssemblyReference, DeclaredProjectReference, DiscoveryCacheObservation,
    DiscoveryCachePolicy, DiscoveryDiagnostic, DiscoveryDiagnosticSeverity, DiscoveryFailure,
    DiscoveryFailureReason, DiscoveryGrants, DiscoveryInputScope, DiscoveryIssue, DiscoveryOptions,
    DiscoveryRequest, DiscoveryRestoreStyle, DiscoverySnapshot, DiscoveryStanding,
    EvaluationCacheEntry, EvaluationContext, EvaluationEnvironment, EvaluationHostKind, EvaluationInput, EvaluationUnit,
    EvaluationUnitKey, FrameworkIdentity, HostProvenance, ImportToleranceProfile, ProjectDiscovery,
    ProjectKey, RestoreProvenance, SourceMembership,
};

use crate::Result;

/// The seam implemented by Cargo and evaluated-project discovery adapters.
pub trait WorkspaceDiscovery {
    /// Discover owned metadata or typed incomplete-coverage evidence.
    ///
    /// # Errors
    /// Fatal infrastructure/protocol errors abort the invocation rather than
    /// producing a publishable partial snapshot.
    fn discover(&self, request: &DiscoveryRequest) -> Result<DiscoverySnapshot>;
}

/// Discover both Cargo attribution and evaluated projects under explicit grants.
///
/// # Errors
/// Returns fatal adapter errors; bounded project/unit failures remain explicit
/// standing in the returned snapshot.
pub fn discover_workspace(request: &DiscoveryRequest) -> Result<DiscoverySnapshot> {
    let cargo = CargoDiscovery.discover(request)?;
    let mut snapshot = MsBuildDiscovery.discover(request)?;
    snapshot.crates = cargo.crates;
    Ok(snapshot)
}
