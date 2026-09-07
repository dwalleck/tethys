//! Installed host selection and the bounded, evaluation-only protocol transport.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use command_group::{CommandGroup, GroupChild};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::super::{
    DiscoveryDiagnostic, DiscoveryDiagnosticSeverity, DiscoveryFailure,
    DiscoveryFailureReason as Reason, DiscoveryRequest, EvaluationEnvironment, EvaluationHostKind,
    HostProvenance,
};

const REQUEST_LIMIT: usize = 1024 * 1024;
const OUTPUT_LIMIT: usize = 64 * 1024 * 1024;
const STDERR_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct EvaluatedProject {
    pub protocol_version: u32,
    pub success: bool,
    pub project_path: PathBuf,
    pub host: Option<HostProvenance>,
    pub properties: BTreeMap<String, String>,
    pub items: BTreeMap<String, Vec<EvaluatedItem>>,
    pub imports: Vec<PathBuf>,
    pub glob_patterns: Vec<GlobPattern>,
    pub diagnostics: Vec<DiscoveryDiagnostic>,
    pub cache_eligible: bool,
    pub cache_ineligibility: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct EvaluatedItem {
    pub include: String,
    pub full_path: PathBuf,
    pub metadata: BTreeMap<String, String>,
}

impl EvaluatedItem {
    /// `MSBuild` metadata names are case-insensitive; retain authored spelling in the map.
    pub(super) fn metadata_value(&self, name: &str) -> Option<&str> {
        self.metadata
            .get(name)
            .or_else(|| {
                self.metadata
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .map(|(_, value)| value)
            })
            .map(String::as_str)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct GlobPattern {
    pub project_path: PathBuf,
    pub item_type: String,
    pub include: String,
    pub exclude: String,
    pub remove: String,
}

#[derive(Debug, Clone)]
pub(super) struct HostSelection {
    pub kind: EvaluationHostKind,
    pub msbuild_path: PathBuf,
    pub worker_path: PathBuf,
    pub fingerprint: String,
    executable: PathBuf,
    sdk_version: Option<String>,
    fingerprint_paths: Vec<PathBuf>,
    fingerprint_seed: Vec<u8>,
}

impl HostSelection {
    /// The project's closed recipe cannot account for arbitrary code loaded by its host.
    /// Consult this on each lookup and publication, independently of persisted worker evidence.
    pub(super) fn cache_ineligibility(
        &self,
        environment: &EvaluationEnvironment,
    ) -> Option<&'static str> {
        if self.kind == EvaluationHostKind::Framework {
            // CLR4's stable version string does not identify the installed CLR/BCL
            // closure, which this host selection does not currently fingerprint.
            return Some("unqualified_framework_runtime");
        }
        if environment.loads_runtime_code() {
            return Some("unqualified_runtime_code_extension");
        }
        if self.kind == EvaluationHostKind::Sdk {
            match installation_muxer(&self.msbuild_path) {
                Ok(native) if native == self.executable => {}
                Ok(_) => return Some("unqualified_forwarding_muxer"),
                Err(error) => {
                    tracing::debug!(path = %self.msbuild_path.display(), %error, "Cannot qualify installed native muxer");
                    return Some("unqualified_forwarding_muxer");
                }
            }
        }
        None
    }

    pub(super) fn is_current(&self, environment: &EvaluationEnvironment) -> crate::Result<bool> {
        match installed_fingerprint(&self.fingerprint_paths, &self.fingerprint_seed, environment) {
            Ok(value) => Ok(value == self.fingerprint),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

fn installation_muxer(msbuild_path: &Path) -> io::Result<PathBuf> {
    let root = msbuild_path
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "selected SDK directory has no dotnet installation root",
            )
        })?;
    root.join(if cfg!(windows) {
        "dotnet.exe"
    } else {
        "dotnet"
    })
    .canonicalize()
}

fn installed_fingerprint(
    paths: &[PathBuf],
    seed: &[u8],
    environment: &EvaluationEnvironment,
) -> io::Result<String> {
    let mut hash = Sha256::new();
    hash.update(seed);
    for path in paths {
        if path.metadata()?.is_dir() {
            hash_directory(&mut hash, path)?;
        } else {
            hash_file(&mut hash, path)?;
        }
    }
    for (key, value) in environment.iter() {
        hash.update(key.as_encoded_bytes());
        hash.update([0]);
        hash.update(value.as_encoded_bytes());
        hash.update([0]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(super) fn failure(reason: Reason, message: impl Into<String>) -> DiscoveryFailure {
    DiscoveryFailure {
        reason,
        diagnostics: vec![DiscoveryDiagnostic {
            message: message.into(),
            ..DiscoveryDiagnostic::default()
        }],
    }
}

pub(super) struct ProcessOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[cfg_attr(test, derive(Debug))]
pub(super) enum ProcessFailure {
    Timeout,
    Io(io::Error),
    Overflow,
}

impl From<io::Error> for ProcessFailure {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

struct ProcessGuard(GroupChild);
impl Drop for ProcessGuard {
    fn drop(&mut self) {
        // Also terminate descendants whose leader already exited and closed its pipes.
        if let Err(error) = self.0.kill()
            && error.kind() != io::ErrorKind::InvalidInput
            && error.raw_os_error() != Some(3)
        {
            tracing::warn!(%error, "could not terminate discovery process group");
        }
    }
}

fn read_bounded(mut pipe: impl Read, limit: usize) -> Result<Vec<u8>, ProcessFailure> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let size = pipe.read(&mut chunk)?;
        if size == 0 {
            return Ok(bytes);
        }
        if size > limit - bytes.len() {
            return Err(ProcessFailure::Overflow);
        }
        bytes.extend_from_slice(&chunk[..size]);
    }
}

/// All three pipe operations run concurrently. The supervisor never waits on EOF.
/// Cancellation kills the entire group and polls reaping; it never joins a blocked reader.
pub(super) fn run(
    command: &mut Command,
    input: Vec<u8>,
    timeout: Duration,
    environment: &EvaluationEnvironment,
) -> Result<ProcessOutput, ProcessFailure> {
    if input.len() > REQUEST_LIMIT {
        return Err(ProcessFailure::Overflow);
    }
    let deadline = Instant::now() + timeout;
    // Every bounded launch goes through here, so the environment the recipe
    // fingerprinted is the only environment an evaluation host is ever given.
    let mut child = ProcessGuard(
        command
            .env_clear()
            .envs(environment.iter())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .group_spawn()?,
    );
    let stdin = child
        .0
        .inner()
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("missing child stdin"))?;
    let stdout = child
        .0
        .inner()
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("missing child stdout"))?;
    let stderr = child
        .0
        .inner()
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("missing child stderr"))?;
    let (sender, receiver) = mpsc::channel();
    let output_sender = sender.clone();
    let error_sender = sender.clone();
    let workers = [
        thread::spawn(move || {
            let result = read_bounded(stdout, OUTPUT_LIMIT);
            if output_sender.send((0, result)).is_err() {
                tracing::debug!("stdout supervisor closed");
            }
        }),
        thread::spawn(move || {
            let result = read_bounded(stderr, STDERR_LIMIT);
            if error_sender.send((1, result)).is_err() {
                tracing::debug!("stderr supervisor closed");
            }
        }),
        thread::spawn(move || {
            let mut stdin = stdin;
            let result = stdin
                .write_all(&input)
                .map(|()| Vec::new())
                .map_err(ProcessFailure::Io);
            drop(stdin);
            if sender.send((2, result)).is_err() {
                tracing::debug!("stdin supervisor closed");
            }
        }),
    ];
    let result = supervise(&mut child, &receiver, deadline);
    #[cfg(test)]
    eprintln!("[DEBUG-f26] supervision: {:?}", result.as_ref().map(|_| ()));
    let cleanup = terminate_and_reap(&mut child);
    #[cfg(test)]
    eprintln!("[DEBUG-f26] cleanup: {cleanup:?}");
    cleanup?;
    for worker in workers {
        // A process that escapes its group is outside the trust contract, but cannot stall this caller.
        if worker.is_finished() && worker.join().is_err() {
            return Err(ProcessFailure::Io(io::Error::other(
                "discovery pipe thread panicked",
            )));
        }
    }
    result
}

fn supervise(
    child: &mut ProcessGuard,
    receiver: &mpsc::Receiver<(usize, Result<Vec<u8>, ProcessFailure>)>,
    deadline: Instant,
) -> Result<ProcessOutput, ProcessFailure> {
    let mut streams: [Option<Vec<u8>>; 3] = [None, None, None];
    loop {
        match receiver.recv_timeout(
            Duration::from_millis(5).min(deadline.saturating_duration_since(Instant::now())),
        ) {
            Ok((index, Ok(bytes))) => streams[index] = Some(bytes),
            Ok((_, Err(error))) => break Err(error),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if streams.iter().any(Option::is_none) {
                    break Err(ProcessFailure::Io(io::Error::other(
                        "discovery pipe thread exited without reporting its result",
                    )));
                }
                // A child can close all pipes yet keep running. A disconnected
                // receiver returns immediately, so retain the bounded polling interval.
                thread::sleep(
                    Duration::from_millis(5)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
        }
        match child.0.try_wait() {
            Ok(Some(status)) if streams.iter().all(Option::is_some) => {
                break Ok(ProcessOutput {
                    status,
                    stdout: streams[0].take().unwrap_or_default(),
                    stderr: streams[1].take().unwrap_or_default(),
                });
            }
            Ok(_) => {}
            Err(error) => break Err(ProcessFailure::Io(error)),
        }
        if Instant::now() >= deadline {
            break Err(ProcessFailure::Timeout);
        }
    }
}

fn terminate_and_reap(child: &mut ProcessGuard) -> Result<(), ProcessFailure> {
    let reap_deadline = Instant::now() + Duration::from_secs(2);
    let mut signalled = false;
    let mut kill_error = None;
    loop {
        if !signalled {
            match child.0.kill() {
                Ok(()) => signalled = true,
                Err(error)
                    if error.kind() == io::ErrorKind::InvalidInput
                        || (cfg!(unix) && error.raw_os_error() == Some(3)) =>
                {
                    signalled = true;
                }
                Err(error)
                    if cfg!(target_os = "macos")
                        && error.kind() == io::ErrorKind::PermissionDenied =>
                {
                    // Darwin may deny signalling an unreaped zombie group. Reap
                    // and retry: leader exit alone does not prove the group is gone.
                    kill_error = Some(error);
                }
                Err(error) => return Err(ProcessFailure::Io(error)),
            }
        }
        if child.0.try_wait()?.is_some() && signalled {
            return Ok(());
        }
        if Instant::now() >= reap_deadline {
            return Err(ProcessFailure::Io(kill_error.unwrap_or_else(|| {
                io::Error::other("terminated discovery process could not be reaped")
            })));
        }
        thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn executable(name: &str, environment: &EvaluationEnvironment) -> io::Result<PathBuf> {
    let path = Path::new(name);
    if path.components().count() > 1 || path.is_absolute() {
        return path.canonicalize();
    }
    let search = environment.get("PATH").unwrap_or_default().to_owned();
    for directory in std::env::split_paths(&search) {
        let candidate = directory.join(name);
        match candidate.metadata() {
            Ok(metadata) if metadata.is_file() => return candidate.canonicalize(),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("installed executable {name} was not found on PATH"),
    ))
}

pub(super) struct HostSelector<'a> {
    request: &'a DiscoveryRequest,
    selections: BTreeMap<(PathBuf, bool), Result<HostSelection, DiscoveryFailure>>,
}

impl<'a> HostSelector<'a> {
    pub fn new(request: &'a DiscoveryRequest) -> Self {
        Self {
            request,
            selections: BTreeMap::new(),
        }
    }

    /// The invocation's environment: fingerprinted, searched, and given to every host.
    fn environment(&self) -> &'a EvaluationEnvironment {
        &self.request.options.environment
    }

    pub fn select(
        &mut self,
        project: &Path,
        uses_sdk: bool,
    ) -> Result<HostSelection, DiscoveryFailure> {
        if !self.request.options.trust_msbuild {
            return Err(failure(
                Reason::TrustRequired,
                "MSBuild evaluation requires explicit trust",
            ));
        }
        let directory = project
            .parent()
            .ok_or_else(|| failure(Reason::MalformedInput, "project has no parent directory"))?;
        let policy = nearest_global(directory)?;
        let key = (policy.clone().unwrap_or_default(), uses_sdk);
        if let Some(selection) = self.selections.get(&key) {
            return selection.clone();
        }
        let selection = self.select_installed(directory, policy.as_deref(), uses_sdk);
        self.selections.insert(key, selection.clone());
        selection
    }

    fn select_installed(
        &self,
        directory: &Path,
        policy: Option<&Path>,
        uses_sdk: bool,
    ) -> Result<HostSelection, DiscoveryFailure> {
        let unavailable =
            |error: io::Error| failure(Reason::ToolchainUnavailable, error.to_string());
        let explicit = self
            .request
            .options
            .msbuild_path
            .as_ref()
            .map(|path| path.canonicalize())
            .transpose()
            .map_err(unavailable)?;
        let framework = explicit.as_ref().is_some_and(|path| {
            path.join("MSBuild.exe").is_file() && !path.join("MSBuild.dll").is_file()
        }) || (!uses_sdk && cfg!(windows) && explicit.is_none());
        let (kind, msbuild_path, executable, sdk_version, runtime_evidence) = if framework {
            if !cfg!(windows) {
                return Err(failure(
                    Reason::ToolchainUnavailable,
                    "Framework MSBuild requires Windows",
                ));
            }
            let path = if let Some(path) = explicit {
                path
            } else {
                self.visual_studio(directory)?
            };
            if path
                .file_name()
                .is_none_or(|name| !name.to_string_lossy().eq_ignore_ascii_case("Bin"))
                || path
                    .parent()
                    .and_then(Path::file_name)
                    .is_none_or(|name| !name.to_string_lossy().eq_ignore_ascii_case("Current"))
            {
                return Err(failure(
                    Reason::ToolchainUnavailable,
                    "select Visual Studio 17.14 MSBuild/Current/Bin, not amd64",
                ));
            }
            (
                EvaluationHostKind::Framework,
                path.clone(),
                path.join("MSBuild.exe"),
                None,
                Vec::new(),
            )
        } else {
            let dotnet = if let Some(path) = &explicit {
                installation_muxer(path).map_err(unavailable)?
            } else if let Some(path) = self.environment().get("DOTNET") {
                PathBuf::from(path).canonicalize().map_err(unavailable)?
            } else {
                executable(
                    if cfg!(windows) {
                        "dotnet.exe"
                    } else {
                        "dotnet"
                    },
                    self.environment(),
                )
                .map_err(unavailable)?
            };
            let (path, version) =
                self.installed_sdk(directory, &dotnet, explicit.as_deref(), policy)?;
            let runtimes = self.probe(
                Command::new(&dotnet)
                    .arg("--list-runtimes")
                    .current_dir(directory),
            )?;
            if !runtimes.status.success() {
                return Err(failure(
                    Reason::ToolchainUnavailable,
                    diagnostic_text(&runtimes),
                ));
            }
            (
                EvaluationHostKind::Sdk,
                path,
                dotnet,
                Some(version),
                runtimes.stdout,
            )
        };
        self.complete_selection(
            kind,
            msbuild_path,
            executable,
            sdk_version,
            runtime_evidence,
            policy,
        )
    }

    fn complete_selection(
        &self,
        kind: EvaluationHostKind,
        msbuild_path: PathBuf,
        executable: PathBuf,
        sdk_version: Option<String>,
        runtime_evidence: Vec<u8>,
        policy: Option<&Path>,
    ) -> Result<HostSelection, DiscoveryFailure> {
        let unavailable =
            |error: io::Error| failure(Reason::ToolchainUnavailable, error.to_string());
        let framework = kind == EvaluationHostKind::Framework;
        let companion = if let Some(directory) = &self.request.options.companion_directory {
            directory.clone()
        } else {
            std::env::current_exe()
                .map_err(unavailable)?
                .parent()
                .ok_or_else(|| {
                    failure(
                        Reason::ToolchainUnavailable,
                        "current executable has no directory",
                    )
                })?
                .join("msbuild-evaluate")
        };
        let worker_directory = companion.join(if framework { "framework" } else { "sdk" });
        let worker_path = worker_directory
            .join(if framework {
                "Tethys.MSBuild.Evaluate.exe"
            } else {
                "Tethys.MSBuild.Evaluate.dll"
            })
            .canonicalize()
            .map_err(unavailable)?;
        let mut fingerprint_paths = vec![
            executable.clone(),
            msbuild_path.join("Microsoft.Build.dll"),
            msbuild_path.join(if framework {
                "MSBuild.exe"
            } else {
                "MSBuild.dll"
            }),
            worker_directory,
        ];
        for line in String::from_utf8_lossy(&runtime_evidence).lines() {
            if let Some((_, location)) = line.split_once(" [")
                && let Some(location) = location.strip_suffix(']')
            {
                fingerprint_paths.push(PathBuf::from(location));
            }
        }
        for name in ["SdkResolvers", "Sdks"] {
            let path = msbuild_path.join(name);
            match path.metadata() {
                Ok(_) => fingerprint_paths.push(path),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(unavailable(error)),
            }
        }
        if let Some(policy) = policy {
            fingerprint_paths.push(policy.to_owned());
        }
        fingerprint_paths.sort();
        fingerprint_paths.dedup();
        let fingerprint =
            installed_fingerprint(&fingerprint_paths, &runtime_evidence, self.environment())
                .map_err(unavailable)?;
        Ok(HostSelection {
            kind,
            msbuild_path,
            worker_path,
            fingerprint,
            executable,
            sdk_version,
            fingerprint_paths,
            fingerprint_seed: runtime_evidence,
        })
    }

    fn installed_sdk(
        &self,
        directory: &Path,
        dotnet: &Path,
        explicit: Option<&Path>,
        policy: Option<&Path>,
    ) -> Result<(PathBuf, String), DiscoveryFailure> {
        let unavailable =
            |error: io::Error| failure(Reason::ToolchainUnavailable, error.to_string());
        // The installed muxer is the authority for global.json, prerelease and roll-forward.
        let selected_version = if explicit.is_none() || policy.is_some() {
            let output =
                self.probe(Command::new(dotnet).arg("--version").current_dir(directory))?;
            if !output.status.success() {
                return Err(failure(
                    if policy.is_some() {
                        Reason::SdkUnresolved
                    } else {
                        Reason::ToolchainUnavailable
                    },
                    diagnostic_text(&output),
                ));
            }
            Some(
                String::from_utf8(output.stdout)
                    .map_err(|error| failure(Reason::ToolchainUnavailable, error.to_string()))?
                    .trim()
                    .to_owned(),
            )
        } else {
            None
        };
        let list = self.probe(
            Command::new(dotnet)
                .arg("--list-sdks")
                .current_dir(directory),
        )?;
        if !list.status.success() {
            return Err(failure(
                Reason::ToolchainUnavailable,
                diagnostic_text(&list),
            ));
        }
        let text = String::from_utf8(list.stdout)
            .map_err(|error| failure(Reason::ToolchainUnavailable, error.to_string()))?;
        let mut matched = None;
        for line in text.lines() {
            if let Some((version, location)) = line.split_once(" [")
                && let Some(location) = location.strip_suffix(']')
            {
                let candidate = Path::new(location)
                    .join(version)
                    .canonicalize()
                    .map_err(unavailable)?;
                if explicit.map_or_else(
                    || selected_version.as_deref() == Some(version),
                    |path| path == candidate,
                ) {
                    if policy.is_some() && selected_version.as_deref() != Some(version) {
                        return Err(failure(
                            Reason::SdkUnresolved,
                            "explicit MSBuild SDK conflicts with the SDK selected by global.json",
                        ));
                    }
                    matched = Some((candidate, version.to_owned()));
                }
            }
        }
        matched.ok_or_else(|| {
            failure(
                Reason::ToolchainUnavailable,
                "selected SDK is not in the installed dotnet SDK inventory",
            )
        })
    }

    fn probe(&self, command: &mut Command) -> Result<ProcessOutput, DiscoveryFailure> {
        run(
            command,
            Vec::new(),
            self.request.options.timeout,
            self.environment(),
        )
        .map_err(|error| match error {
            ProcessFailure::Timeout => failure(
                Reason::Timeout,
                "installed host probe exceeded its deadline",
            ),
            ProcessFailure::Io(error) => failure(Reason::ToolchainUnavailable, error.to_string()),
            ProcessFailure::Overflow => failure(
                Reason::ToolchainUnavailable,
                "installed host probe exceeded its output limit",
            ),
        })
    }

    fn visual_studio(&self, directory: &Path) -> Result<PathBuf, DiscoveryFailure> {
        let base = self.environment().get("ProgramFiles(x86)").ok_or_else(|| {
            failure(
                Reason::ToolchainUnavailable,
                "Visual Studio Installer directory is unavailable",
            )
        })?;
        let output = self.probe(
            Command::new(Path::new(&base).join("Microsoft Visual Studio/Installer/vswhere.exe"))
                .args([
                    "-latest",
                    "-products",
                    "*",
                    "-version",
                    "[17.14,17.15)",
                    "-requires",
                    "Microsoft.Component.MSBuild",
                    "-property",
                    "installationPath",
                ])
                .current_dir(directory),
        )?;
        if !output.status.success() {
            return Err(failure(
                Reason::ToolchainUnavailable,
                diagnostic_text(&output),
            ));
        }
        let text = String::from_utf8(output.stdout)
            .map_err(|error| failure(Reason::ToolchainUnavailable, error.to_string()))?;
        if text.trim().is_empty() {
            return Err(failure(
                Reason::ToolchainUnavailable,
                "Visual Studio 17.14 MSBuild is not installed",
            ));
        }
        Path::new(text.trim())
            .join("MSBuild/Current/Bin")
            .canonicalize()
            .map_err(|error| failure(Reason::ToolchainUnavailable, error.to_string()))
    }
}

fn nearest_global(directory: &Path) -> Result<Option<PathBuf>, DiscoveryFailure> {
    for ancestor in directory.ancestors() {
        let path = ancestor.join("global.json");
        match path.metadata() {
            Ok(_) => {
                let mut bytes = Vec::new();
                std::fs::File::open(&path)
                    .and_then(|file| {
                        file.take((REQUEST_LIMIT + 1) as u64)
                            .read_to_end(&mut bytes)
                    })
                    .map_err(|error| failure(Reason::MalformedInput, error.to_string()))?;
                if bytes.len() > REQUEST_LIMIT {
                    return Err(failure(Reason::MalformedInput, "global.json exceeds 1 MiB"));
                }
                // dotnet supports comments/trailing commas; let its native parser determine policy validity.
                return Ok(Some(path));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(failure(Reason::MalformedInput, error.to_string())),
        }
    }
    Ok(None)
}

fn hash_file(hash: &mut Sha256, path: &Path) -> io::Result<()> {
    hash.update(path.as_os_str().as_encoded_bytes().len().to_le_bytes());
    hash.update(path.as_os_str().as_encoded_bytes());
    let mut file = std::fs::File::open(path)?;
    hash.update(file.metadata()?.len().to_le_bytes());
    let mut chunk = [0_u8; 16_384];
    loop {
        let size = file.read(&mut chunk)?;
        if size == 0 {
            break;
        }
        hash.update(&chunk[..size]);
    }
    Ok(())
}

fn hash_directory(hash: &mut Sha256, path: &Path) -> io::Result<()> {
    let mut entries = std::fs::read_dir(path)?.collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let kind = entry.file_type()?;
        if kind.is_dir() {
            hash_directory(hash, &entry.path())?;
        } else if kind.is_file() {
            hash_file(hash, &entry.path())?;
        } else {
            return Err(io::Error::other(
                "worker distribution contains a nonregular entry",
            ));
        }
    }
    Ok(())
}

pub(super) fn diagnostic_text(output: &ProcessOutput) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

pub(super) fn restore_command(host: &HostSelection) -> Command {
    let mut command = Command::new(&host.executable);
    if host.kind == EvaluationHostKind::Sdk {
        let assembly = host.msbuild_path.join("MSBuild.dll");
        command.arg(dunce::simplified(&assembly));
    }
    command
}

fn native_host_failure(code: i32) -> bool {
    // dotnet/runtime src/native/corehost/error_codes.h; Unix preserves the low exit byte.
    let code = if cfg!(windows) {
        u32::from_ne_bytes(code.to_ne_bytes())
    } else {
        0x8000_8000 | u32::from_ne_bytes(code.to_ne_bytes())
    };
    matches!(code, 0x8000_8082..=0x8000_8085 | 0x8000_8087..=0x8000_808C | 0x8000_808E | 0x8000_8093 | 0x8000_8095..=0x8000_8096 | 0x8000_809A | 0x8000_809C | 0x8000_80A2 | 0x8000_80A5..=0x8000_80A7)
}

pub(super) fn evaluate(
    request: &DiscoveryRequest,
    host: &HostSelection,
    project: &Path,
    target_framework: Option<&str>,
) -> crate::Result<Result<EvaluatedProject, DiscoveryFailure>> {
    if !request.options.trust_msbuild {
        return Ok(Err(failure(
            Reason::TrustRequired,
            "MSBuild evaluation requires explicit trust",
        )));
    }
    // MSBuild globbing needs native presentation; domain and cache identities stay canonical.
    let native_project = dunce::simplified(project);
    let payload = serde_json::to_vec(&serde_json::json!({
        "protocol_version": 1, "workspace_root": dunce::simplified(&request.workspace_root), "project_path": native_project,
        "target_framework": target_framework, "global_properties": request.options.context.effective_globals(),
        "msbuild_path": dunce::simplified(&host.msbuild_path), "trust_granted": true
    })).map_err(|error| crate::Error::Internal(error.to_string()))?;
    let mut command = Command::new(if host.kind == EvaluationHostKind::Sdk {
        &host.executable
    } else {
        &host.worker_path
    });
    if host.kind == EvaluationHostKind::Sdk {
        command.arg(&host.worker_path);
    }
    command.current_dir(dunce::simplified(
        project
            .parent()
            .ok_or_else(|| crate::Error::Config("project has no directory".into()))?,
    ));
    let output = match run(
        &mut command,
        payload,
        request.options.timeout,
        &request.options.environment,
    ) {
        Ok(output) => output,
        Err(ProcessFailure::Timeout) => {
            return Ok(Err(failure(
                Reason::Timeout,
                "MSBuild evaluation exceeded its deadline",
            )));
        }
        Err(ProcessFailure::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Err(failure(
                Reason::ToolchainUnavailable,
                error.to_string(),
            )));
        }
        Err(ProcessFailure::Io(error)) => return Err(error.into()),
        Err(ProcessFailure::Overflow) => {
            return Err(crate::Error::Internal(
                "MSBuild protocol exceeded a byte limit".into(),
            ));
        }
    };
    let mut response = decode_evaluation(&output, host, native_project)?;
    if let Ok(response) = &mut response {
        project.clone_into(&mut response.project_path);
        if let Some(actual) = &mut response.host {
            actual.path.clone_from(&host.msbuild_path);
        }
    }
    Ok(response)
}

fn decode_evaluation(
    output: &ProcessOutput,
    host: &HostSelection,
    project: &Path,
) -> crate::Result<Result<EvaluatedProject, DiscoveryFailure>> {
    // Runtime-host failures have no worker JSON; their native exit status identifies the host layer.
    if output.stdout.is_empty()
        && host.kind == EvaluationHostKind::Sdk
        && output.status.code().is_some_and(native_host_failure)
    {
        return Ok(Err(failure(
            Reason::ToolchainUnavailable,
            diagnostic_text(output),
        )));
    }
    let mut response: EvaluatedProject =
        serde_json::from_slice(&output.stdout).map_err(|error| {
            crate::Error::Internal(format!("malformed MSBuild worker protocol: {error}"))
        })?;
    if response.protocol_version != 1 {
        return Ok(Err(failure(
            Reason::ToolchainUnavailable,
            "incompatible MSBuild worker protocol version",
        )));
    }
    if response.project_path != project
        || response.success != output.status.success()
        || (response.success && response.host.is_none())
        || (!response.success && response.diagnostics.is_empty())
    {
        return Err(crate::Error::Internal(
            "inconsistent MSBuild worker response identity/status".into(),
        ));
    }
    if let Err(error) = check_evaluated_host(&response, host)? {
        return Ok(Err(error));
    }
    if response.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.exception_type.as_deref(),
            Some(
                "System.IO.IOException"
                    | "Newtonsoft.Json.JsonSerializationException"
                    | "Newtonsoft.Json.JsonReaderException"
            )
        )
    }) {
        return Err(crate::Error::Internal(format!(
            "worker infrastructure failure: {:?}",
            response.diagnostics
        )));
    }
    if response.success
        && (response
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiscoveryDiagnosticSeverity::Error)
            || !["Compile", "ProjectReference", "Reference"]
                .iter()
                .all(|key| response.items.contains_key(*key)))
    {
        return Err(crate::Error::Internal(
            "inconsistent successful MSBuild metadata".into(),
        ));
    }
    if !output.stderr.is_empty() {
        response.diagnostics.push(DiscoveryDiagnostic {
            severity: DiscoveryDiagnosticSeverity::Warning,
            message: String::from_utf8_lossy(&output.stderr).into_owned(),
            ..DiscoveryDiagnostic::default()
        });
    }
    Ok(Ok(response))
}

fn check_evaluated_host(
    response: &EvaluatedProject,
    host: &HostSelection,
) -> crate::Result<Result<(), DiscoveryFailure>> {
    if let Some(actual) = &response.host {
        let runtime_matches = match host.kind {
            EvaluationHostKind::Framework => actual.runtime.starts_with(".NET Framework CLR "),
            EvaluationHostKind::Sdk => {
                actual.runtime.strip_prefix(".NET ").is_some_and(|version| {
                    String::from_utf8_lossy(&host.fingerprint_seed)
                        .lines()
                        .any(|line| {
                            let mut words = line.split_whitespace();
                            words.next() == Some("Microsoft.NETCore.App")
                                && words.next() == Some(version)
                        })
                })
            }
        };
        if actual.kind != host.kind
            || actual.path.canonicalize()? != host.msbuild_path
            || actual.version.is_empty()
            || !runtime_matches
        {
            return Ok(Err(failure(
                Reason::ToolchainUnavailable,
                "loaded MSBuild host does not match the selected installation",
            )));
        }
        if response.success {
            let property = |key: &str| {
                response
                    .properties
                    .get(key)
                    .map(String::as_str)
                    .unwrap_or_default()
            };
            let bin = Path::new(property("MSBuildBinPath")).canonicalize()?;
            let expected_runtime = if host.kind == EvaluationHostKind::Sdk {
                "Core"
            } else {
                "Full"
            };
            if bin != host.msbuild_path
                || property("MSBuildRuntimeType") != expected_runtime
                || property("MSBuildFileVersion") != actual.version
                || (host.kind == EvaluationHostKind::Framework
                    && !actual.version.starts_with("17.14."))
                || host.sdk_version.as_ref().is_some_and(|version| {
                    property("UsingMicrosoftNETSdk").eq_ignore_ascii_case("true")
                        && property("NETCoreSdkVersion") != version
                })
            {
                return Ok(Err(failure(
                    Reason::ToolchainUnavailable,
                    "effective MSBuild runtime/toolset/SDK differs from the selected installed host",
                )));
            }
        }
    }
    Ok(Ok(()))
}

pub(super) fn classify_failure(project: &EvaluatedProject) -> DiscoveryFailure {
    let mut reason = Reason::EvaluationFailed;
    for diagnostic in &project.diagnostics {
        if diagnostic.severity != DiscoveryDiagnosticSeverity::Error {
            continue;
        }
        match (
            diagnostic.code.as_deref(),
            diagnostic.exception_type.as_deref(),
        ) {
            (
                Some("MSB4236" | "MSB4276" | "MSB4242" | "MSB4247" | "NETSDK1147" | "NETSDK1045"),
                _,
            ) => {
                reason = Reason::SdkUnresolved;
                break;
            }
            (Some("MSB4025" | "MSB4066" | "MSB4067" | "MSB4118"), _)
            | (_, Some("System.Xml.XmlException")) => reason = Reason::MalformedInput,
            (
                _,
                Some(
                    "System.BadImageFormatException"
                    | "System.TypeLoadException"
                    | "System.MissingMethodException"
                    | "System.IO.FileLoadException",
                ),
            ) => reason = Reason::ToolchainUnavailable,
            (_, Some("System.IO.FileNotFoundException")) if project.host.is_none() => {
                reason = Reason::ToolchainUnavailable;
            }
            _ => {}
        }
    }
    DiscoveryFailure {
        reason,
        diagnostics: project.diagnostics.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The transport tests exercise pipes and reaping, not recipe identity, and
    /// resolve `sh` on `PATH`; they inherit rather than construct an environment.
    fn transport_environment() -> EvaluationEnvironment {
        EvaluationEnvironment::inherited()
    }

    fn unavailable_host(root: &Path) -> HostSelection {
        HostSelection {
            kind: EvaluationHostKind::Sdk,
            msbuild_path: root.join("missing-sdk"),
            worker_path: root.join("missing-worker"),
            executable: root.join("missing-executable"),
            fingerprint: String::new(),
            sdk_version: None,
            fingerprint_paths: Vec::new(),
            fingerprint_seed: Vec::new(),
        }
    }

    #[test]
    fn absent_trust_precedes_all_process_and_host_access() {
        let root = tempfile::tempdir().unwrap();
        let request = DiscoveryRequest::new(
            root.path(),
            super::super::super::DiscoveryOptions::default(),
        )
        .unwrap();
        let project = root.path().join("App.csproj");
        let outcome = evaluate(&request, &unavailable_host(root.path()), &project, None).unwrap();
        assert!(matches!(
            outcome,
            Err(DiscoveryFailure {
                reason: Reason::TrustRequired,
                ..
            })
        ));
        assert!(matches!(
            HostSelector::new(&request).select(&project, true),
            Err(DiscoveryFailure {
                reason: Reason::TrustRequired,
                ..
            })
        ));
    }

    #[test]
    fn package_restore_requires_separate_grant_before_host_access() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("App.csproj");
        std::fs::write(&project, "<Project/>").unwrap();
        std::fs::write(
            root.path().join("packages.config"),
            "<packages><package id=\"Absent\" version=\"1.0.0\"/></packages>",
        )
        .unwrap();
        let request = DiscoveryRequest::new(
            root.path(),
            super::super::super::DiscoveryOptions {
                trust_msbuild: true,
                ..Default::default()
            },
        )
        .unwrap();
        let inputs = super::super::restore::inspect(&request, &project, &[]).unwrap();
        let result = super::super::restore::ensure(
            &request,
            &project,
            &inputs,
            &unavailable_host(root.path()),
            None,
            None,
        )
        .unwrap();
        assert!(matches!(
            result,
            Err(DiscoveryFailure {
                reason: Reason::RestoreRequired,
                ..
            })
        ));
        #[cfg(not(windows))]
        {
            let mut request = request;
            request.options.allow_restore = true;
            let result = super::super::restore::ensure(
                &request,
                &project,
                &inputs,
                &unavailable_host(root.path()),
                None,
                None,
            )
            .unwrap();
            assert!(matches!(
                result,
                Err(DiscoveryFailure {
                    reason: Reason::RestoreUnsupportedOnHost,
                    ..
                })
            ));
        }
    }

    /// A bounded launch must receive exactly the environment its recipe
    /// fingerprinted. If the host inherited the caller's ambient settings instead,
    /// the hashed environment and the evaluated one could differ without evidence.
    #[cfg(unix)]
    #[test]
    fn a_bounded_launch_receives_exactly_the_given_environment() {
        let environment = EvaluationEnvironment::empty()
            .with("PATH", "/usr/bin:/bin")
            .with("TETHYS_PROBE", "GIVEN");
        let output = run(
            Command::new("/bin/sh").args(["-c", "env"]),
            Vec::new(),
            Duration::from_secs(5),
            &environment,
        )
        .expect("the probe must run");
        let reported = String::from_utf8_lossy(&output.stdout);
        assert!(
            reported.lines().any(|line| line == "TETHYS_PROBE=GIVEN"),
            "the given environment must reach the host: {reported}"
        );
        // Cargo sets this for the test process; production must not silently
        // extend a host's environment with the caller's ambient settings.
        assert!(
            !reported.contains("CARGO_PKG_NAME"),
            "ambient caller settings leaked into the host: {reported}"
        );
    }

    /// Every name the product publishes as a runtime code extension must actually
    /// disqualify reuse. Driving the assertion from the constant itself means a new
    /// entry is covered without editing a second list anywhere.
    #[test]
    fn every_runtime_code_extension_disqualifies_reuse() {
        let root = tempfile::tempdir().unwrap();
        let host = unavailable_host(root.path());
        assert_eq!(
            host.cache_ineligibility(&EvaluationEnvironment::empty()),
            Some("unqualified_forwarding_muxer"),
            "the clean baseline must fail for an unrelated reason, not vacuously",
        );
        for name in EvaluationEnvironment::RUNTIME_CODE_EXTENSIONS {
            let extended = EvaluationEnvironment::empty().with(*name, "Probe.dll");
            assert_eq!(
                host.cache_ineligibility(&extended),
                Some("unqualified_runtime_code_extension"),
                "{name} must disqualify reuse",
            );
            let blank = EvaluationEnvironment::empty().with(*name, "");
            assert_eq!(
                host.cache_ineligibility(&blank),
                Some("unqualified_forwarding_muxer"),
                "{name} bound to an empty value loads no code",
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn executable_permission_errors_remain_infrastructure_failures() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("not-executable");
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        let result = run(
            &mut Command::new(path),
            Vec::new(),
            Duration::from_secs(1),
            &transport_environment(),
        );
        assert!(
            matches!(result, Err(ProcessFailure::Io(error)) if error.kind() == io::ErrorKind::PermissionDenied)
        );
    }

    #[test]
    fn read_bound_rejects_overflow_without_truncating() {
        assert!(matches!(
            read_bounded(&b"four"[..], 3),
            Err(ProcessFailure::Overflow)
        ));
        assert_eq!(read_bounded(&b"four"[..], 4).ok(), Some(b"four".to_vec()));
    }

    #[cfg(unix)]
    #[test]
    fn deadline_includes_descendant_held_stdout() {
        let start = Instant::now();
        let result = run(
            Command::new("sh").args(["-c", "sleep 30 & exit 0"]),
            Vec::new(),
            Duration::from_millis(100),
            &transport_environment(),
        );
        assert!(matches!(result, Err(ProcessFailure::Timeout)));
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[test]
    fn overflowing_stderr_terminates_process_group() {
        let start = Instant::now();
        let result = run(
            Command::new("sh").args(["-c", "yes >&2 & wait"]),
            Vec::new(),
            Duration::from_secs(5),
            &transport_environment(),
        );
        match &result {
            Err(ProcessFailure::Overflow) => {}
            Err(error) => panic!("[DEBUG-f26] expected Overflow, got {error:?}"),
            Ok(output) => panic!(
                "[DEBUG-f26] expected Overflow, got status={}, stdout_bytes={}, stderr_bytes={}, stderr_prefix={:?}",
                output.status,
                output.stdout.len(),
                output.stderr.len(),
                String::from_utf8_lossy(&output.stderr[..output.stderr.len().min(512)]),
            ),
        }
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[test]
    fn already_exited_process_group_is_reaped() {
        let mut child = ProcessGuard(
            Command::new("sh")
                .args(["-c", "exit 0"])
                .group_spawn()
                .expect("should spawn an isolated process group"),
        );
        let pid = child.0.inner().id().to_string();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            // Observe exit without wait/try_wait: the unreaped group is the
            // boundary at which Darwin can deny a signal to zombie members.
            let state = Command::new("ps")
                .args(["-o", "stat=", "-p", &pid])
                .output()
                .expect("should inspect the child without reaping it");
            assert!(state.status.success(), "child must still exist before reap");
            if String::from_utf8_lossy(&state.stdout)
                .trim_start()
                .starts_with('Z')
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "child did not reach exited state"
            );
            thread::sleep(Duration::from_millis(5));
        }
        terminate_and_reap(&mut child).expect("an exited group must be reaped successfully");
        assert!(
            child.0.try_wait().unwrap().unwrap().success(),
            "cleanup must retain the child's successful exit"
        );
    }

    #[cfg(unix)]
    #[test]
    fn simultaneous_full_pipes_do_not_deadlock() {
        let result = run(
            Command::new("sh").args(["-c", "cat >&2"]),
            vec![b'x'; REQUEST_LIMIT],
            Duration::from_secs(5),
            &transport_environment(),
        );
        let output = result.unwrap_or_else(|_| panic!("concurrent pipe transport failed"));
        assert!(output.status.success());
        assert_eq!(output.stderr, vec![b'x'; REQUEST_LIMIT]);
    }
}
