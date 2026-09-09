- Discovery takes the environment as an explicit `DiscoveryOptions::environment`
  value, defaulting to the calling process's environment.
- A cache receipt now fingerprints exactly the environment the evaluation host is
  launched with, instead of two separate reads of ambient process state.
- Callers can set environment-only settings for evaluation and restore (such as
  `NUGET_PACKAGES`) without mutating their own process environment.
- `EvaluationEnvironment::RUNTIME_CODE_EXTENSIONS` publishes the settings that
  disqualify cache reuse.
