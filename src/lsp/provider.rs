//! LSP provider trait and implementations for different language servers.

use crate::types::Language;
use serde_json::Value;

/// Readiness signal a language server emits after `initialize`, gating
/// Pass 3 queries (see `LspClient::wait_until_ready`).
///
/// Both bundled servers load their workspace asynchronously after
/// `initialize` but signal completion differently; declaring the wrong
/// kind means the wait never fires and LSP resolution degrades to the
/// (warned) timeout path — queries then silently return nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessWait {
    /// `$/progress` with a title starting "Loading workspace"
    /// (csharp-ls; see `LspClient::wait_for_solution_load`).
    SolutionLoad,
    /// `experimental/serverStatus` with `quiescent: true`
    /// (rust-analyzer; see `LspClient::wait_for_quiescence`). Its
    /// progress titles never match the csharp-ls pattern, so
    /// [`ReadinessWait::SolutionLoad`] can never fire for it.
    Quiescence,
}

/// Trait for configuring LSP server providers.
///
/// Implementations define how to spawn and configure a specific LSP server.
///
/// # Example
///
/// ```rust
/// use tethys::lsp::{LspProvider, ReadinessWait};
///
/// struct MyCustomLsp;
///
/// impl LspProvider for MyCustomLsp {
///     fn command(&self) -> &'static str { "my-lsp" }
///     fn args(&self) -> Vec<&str> { vec!["--stdio"] }
///     fn readiness_wait(&self) -> ReadinessWait { ReadinessWait::Quiescence }
/// }
/// ```
pub trait LspProvider: Send + Sync {
    /// The command to spawn (e.g., "rust-analyzer", "csharp-ls").
    fn command(&self) -> &'static str;

    /// Additional command-line arguments for the LSP server.
    fn args(&self) -> Vec<&str> {
        vec![]
    }

    /// Language-specific initialization options for the LSP server.
    ///
    /// These are passed in the `initializationOptions` field of `InitializeParams`.
    fn initialize_options(&self) -> Option<Value> {
        None
    }

    /// Installation hint shown when the LSP server is not found.
    fn install_hint(&self) -> &'static str {
        "Please install the language server and ensure it's in your PATH."
    }

    /// Which readiness signal this server emits after `initialize`.
    ///
    /// Pass 3 waits on this signal before its query loop
    /// (`LspClient::wait_until_ready`). Required rather than defaulted:
    /// there is no signal every server emits, and a wrong declaration
    /// silently degrades resolution to the timeout path.
    fn readiness_wait(&self) -> ReadinessWait;
}

/// LSP provider for rust-analyzer.
///
/// rust-analyzer is the official LSP implementation for Rust.
/// Install via: `rustup component add rust-analyzer`
///
/// # Example
///
/// ```rust
/// use tethys::lsp::{LspClient, RustAnalyzerProvider};
/// use std::path::Path;
///
/// let provider = RustAnalyzerProvider;
/// // let client = LspClient::start(&provider, Path::new("."))?;
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct RustAnalyzerProvider;

impl LspProvider for RustAnalyzerProvider {
    fn command(&self) -> &'static str {
        "rust-analyzer"
    }

    fn install_hint(&self) -> &'static str {
        "Install rust-analyzer: https://rust-analyzer.github.io/manual.html#installation"
    }

    fn readiness_wait(&self) -> ReadinessWait {
        ReadinessWait::Quiescence
    }
}

/// LSP provider for csharp-ls.
///
/// csharp-ls is a lightweight C# language server.
/// Install via: `dotnet tool install --global csharp-ls`
///
/// # Example
///
/// ```rust
/// use tethys::lsp::{LspClient, CSharpLsProvider};
/// use std::path::Path;
///
/// let provider = CSharpLsProvider;
/// // let client = LspClient::start(&provider, Path::new("."))?;
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct CSharpLsProvider;

impl LspProvider for CSharpLsProvider {
    fn command(&self) -> &'static str {
        "csharp-ls"
    }

    fn install_hint(&self) -> &'static str {
        "Install csharp-ls: dotnet tool install --global csharp-ls"
    }

    fn readiness_wait(&self) -> ReadinessWait {
        ReadinessWait::SolutionLoad
    }
}

/// Provider type that can be used with dynamic dispatch.
///
/// This enum allows selecting the appropriate LSP provider at runtime
/// based on the language being processed.
#[derive(Debug, Clone, Copy)]
pub enum AnyProvider {
    /// rust-analyzer for Rust files
    Rust(RustAnalyzerProvider),
    /// csharp-ls for C# files
    CSharp(CSharpLsProvider),
}

impl LspProvider for AnyProvider {
    fn command(&self) -> &'static str {
        match self {
            Self::Rust(p) => p.command(),
            Self::CSharp(p) => p.command(),
        }
    }

    fn args(&self) -> Vec<&str> {
        match self {
            Self::Rust(p) => p.args(),
            Self::CSharp(p) => p.args(),
        }
    }

    fn initialize_options(&self) -> Option<Value> {
        match self {
            Self::Rust(p) => p.initialize_options(),
            Self::CSharp(p) => p.initialize_options(),
        }
    }

    fn install_hint(&self) -> &'static str {
        match self {
            Self::Rust(p) => p.install_hint(),
            Self::CSharp(p) => p.install_hint(),
        }
    }

    fn readiness_wait(&self) -> ReadinessWait {
        match self {
            Self::Rust(p) => p.readiness_wait(),
            Self::CSharp(p) => p.readiness_wait(),
        }
    }
}

impl AnyProvider {
    /// Select the appropriate LSP provider for a language.
    #[must_use]
    pub fn for_language(language: Language) -> Self {
        match language {
            Language::Rust => Self::Rust(RustAnalyzerProvider),
            Language::CSharp => Self::CSharp(CSharpLsProvider),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_analyzer_provider_has_correct_command() {
        let provider = RustAnalyzerProvider;
        assert_eq!(provider.command(), "rust-analyzer");
    }

    #[test]
    fn rust_analyzer_provider_has_no_args_by_default() {
        let provider = RustAnalyzerProvider;
        assert!(provider.args().is_empty());
    }

    #[test]
    fn rust_analyzer_provider_has_no_init_options_by_default() {
        let provider = RustAnalyzerProvider;
        assert!(provider.initialize_options().is_none());
    }

    #[test]
    fn rust_analyzer_provider_has_install_hint() {
        let provider = RustAnalyzerProvider;
        assert!(provider.install_hint().contains("rust-analyzer"));
    }

    #[test]
    fn csharp_ls_provider_has_correct_command() {
        let provider = CSharpLsProvider;
        assert_eq!(provider.command(), "csharp-ls");
    }

    #[test]
    fn csharp_ls_provider_has_no_args_by_default() {
        let provider = CSharpLsProvider;
        assert!(provider.args().is_empty());
    }

    #[test]
    fn csharp_ls_provider_has_no_init_options_by_default() {
        let provider = CSharpLsProvider;
        assert!(provider.initialize_options().is_none());
    }

    #[test]
    fn csharp_ls_provider_has_install_hint() {
        let provider = CSharpLsProvider;
        assert!(provider.install_hint().contains("csharp-ls"));
    }

    // ========================================================================
    // AnyProvider Tests
    // ========================================================================

    #[test]
    fn any_provider_for_rust_returns_rust_analyzer() {
        use crate::types::Language;

        let provider = AnyProvider::for_language(Language::Rust);
        assert_eq!(provider.command(), "rust-analyzer");
    }

    #[test]
    fn any_provider_for_csharp_returns_csharp_ls() {
        use crate::types::Language;

        let provider = AnyProvider::for_language(Language::CSharp);
        assert_eq!(provider.command(), "csharp-ls");
    }

    #[test]
    fn any_provider_delegates_install_hint() {
        use crate::types::Language;

        let rust = AnyProvider::for_language(Language::Rust);
        let csharp = AnyProvider::for_language(Language::CSharp);

        assert!(rust.install_hint().contains("rust-analyzer"));
        assert!(csharp.install_hint().contains("csharp-ls"));
    }

    /// Readiness-dispatch fence (tethys-2mjj bug class): each provider must
    /// declare the signal its server actually emits. Declaring the csharp-ls
    /// progress-title wait for rust-analyzer (or vice versa) never fires —
    /// the wait times out and Pass 3 reverts to silently resolving nothing
    /// on cold workspaces.
    #[test]
    fn readiness_wait_per_provider() {
        assert_eq!(
            RustAnalyzerProvider.readiness_wait(),
            ReadinessWait::Quiescence
        );
        assert_eq!(
            CSharpLsProvider.readiness_wait(),
            ReadinessWait::SolutionLoad
        );
    }

    #[test]
    fn any_provider_delegates_readiness_wait() {
        use crate::types::Language;

        assert_eq!(
            AnyProvider::for_language(Language::Rust).readiness_wait(),
            ReadinessWait::Quiescence
        );
        assert_eq!(
            AnyProvider::for_language(Language::CSharp).readiness_wait(),
            ReadinessWait::SolutionLoad
        );
    }

    #[test]
    fn any_provider_delegates_args_and_init_options() {
        use crate::types::Language;

        // Both providers should have default (empty) args and None init options
        let rust = AnyProvider::for_language(Language::Rust);
        let csharp = AnyProvider::for_language(Language::CSharp);

        assert!(rust.args().is_empty());
        assert!(csharp.args().is_empty());
        assert!(rust.initialize_options().is_none());
        assert!(csharp.initialize_options().is_none());
    }
}
