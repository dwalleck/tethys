# csharp-ls in C# CLI workflows — verification (tethys-sn05)

Research ticket: [tethys-sn05] "Verify csharp-ls in C# CLI workflows".
Branch `research/csharp-parity-lsp`, worktree `/tmp/tethys-research-lsp`, commit `7ca48f8` base.
Question under test: what does tethys do end to end for `index --lsp`, `callers --lsp`, and
`impact --lsp` when csharp-ls is **available**, **unavailable**, **slow**, or **unable to load
the workspace**; which results are actually LSP-contributed; and where does the Rust-only CLI
availability gate diverge from the library's per-language provider behavior.

Evidence conventions: `[E]` = empirically observed (probe), `[S]` = source-verified, `[I]` = inference.
Tethys paths are relative to the worktree root; csharp-ls paths are relative to
`razzmatazz/csharp-language-server` at the pinned commit (below). All timestamps in probe
transcripts are seconds since process spawn.

## 0. Scope, environment, and the empirical boundary

- Tethys worktree `/tmp/tethys-research-lsp` has **no built `target/`**, so **no tethys CLI
  experiment was run**. Every tethys-integrated cell below is `[S]` (source-traced) unless a
  standalone probe of the installed csharp-ls could prove the server side `[E]`. The exact
  unproved cells are itemized in §7.
- csharp-ls 0.26.0 installed as a dotnet global tool at `~/.dotnet/tools/csharp-ls` `[E]`
  (`csharp-ls --version` → `csharp-ls, 0.26.0 (Dūmiškės)+b714e27ca7608b1c77fa77caa5f0bb9330fdab49`).
  Installed-package nuspec pins upstream: `https://github.com/razzmatazz/csharp-language-server`,
  commit `b714e27ca7608b1c77fa77caa5f0bb9330fdab49` `[E]`.
- Runtime: .NET SDK 10.0.102 (csharp-ls ≥0.21 requires .NET 10 `[S]`, upstream CHANGELOG).
- Standalone probes drove csharp-ls over stdio against scratch fixtures in `/tmp/csharp-probe`
  (fixture A: SDK-style `App.csproj` net10.0 + `Program.cs` + `Lib.cs`; fixture B: loose
  `Program.cs`, no project files). Probe driver: `/tmp/csharp-probe/probe_lsp.py` (LSP framing,
  acks every server→client request with null, records all messages with timestamps). Scratch
  only — nothing committed except this document.

## 1. Path-by-path behavior matrix

Rows: the three `--lsp` CLI commands. Columns: csharp-ls availability/behavior states.
`ra` = rust-analyzer presence (the only thing the CLI gate checks), `cs` = csharp-ls presence.

### 1.1 `tethys index --lsp`

| State | Flow (verified) | LSP-contributed results | Output / diagnostics / exit |
|---|---|---|---|
| cs available; workspace has `.csproj`/`.sln` (`ra` irrelevant once gate passes) | Gate (`ra`-only) → Pass 2 unresolved refs → `resolve_via_lsp(CSharp)` → spawn → initialize → **didOpen pre-open of every unresolved file** → readiness wait (`SolutionLoad`, default 60 s) → `goto_definition` per unresolved ref → line-granular match-back to `find_symbol_at_line` → `resolve_reference(…, Lsp)` | `[E]/[S]` refs that Pass 2 left `symbol_id IS NULL` and that LSP binds (index `--lsp` prints `LSP resolved: N references via LSP` when N>0) | `[S]` exit 0; LSP session `Completed`; progress observed `[E]`: first didOpen triggers server load (begin progress ~30 ms later), load ~2 s for 1-project fixture, `end` before first query response |
| cs available; workspace is **loose `.cs` files, no `.sln`/`.slnx`/`.csproj`/`.fsproj`** | didOpen pre-open triggers server load → server raises unhandled exception → **server crashes (SIGABRT)** `[E]` → readiness wait hits EOF → `Err(ServerExited)` → warn; query loop: every `goto_definition` errors → refs stay unresolved | none | `[S]` exit 0 (index data valid); stderr: warn "Error while waiting for LSP readiness", first `goto_definition` error at warn, session `Completed{resolved=0, errors=N}` → `LSP error: csharp - …` stderr block; server exit code not captured (shutdown fails) |
| cs **not installed**; ra installed | Gate passes (checks ra only) → C# session `LspClient::start` → spawn `NotFound` → `LspOutcome::ServerUnavailable` | none | `[S]` exit 0; stderr: `LSP error: csharp - LSP server for CSharp failed to start: …` + `hint: Install csharp-ls: dotnet tool install --global csharp-ls` (cli/index.rs `print_lsp_session_errors`); refs stay unresolved |
| cs **not installed** AND ra not installed | `ensure_lsp_if_requested` fails **before any indexing** | — | `[S]` exit 1; `error: rust-analyzer not found …` (LspError::NotFound text + ra install hint); C# path never reached |
| cs slow (large solution; probe used the `csharp.debug.solutionLoadDelay` knob — settable only via `workspace/didChangeConfiguration`/config pull, which tethys cannot send, so real slowness = many/large projects) | Load gated on first didOpen `[E]`; readiness wait observes progress messages; first query is held by the server until load completes `[E]` (fixture: 8 s delay → references answered ~10.5 s after send) | same as available (if load finishes within `lsp_timeout_secs` + per-request 60 s); if load > timeout → readiness `Ok(false)` + warn, queries proceed anyway | `[S]` exit 0; `--lsp-timeout` / `TETHYS_LSP_TIMEOUT` (default 60) bounds readiness at message boundaries only (reads block — tethys-24tj) |

### 1.2 `tethys callers --lsp` (non-transitive; `CallerMode::LspRefined`)

| State | Flow (verified) | LSP-contributed results | Output / diagnostics / exit |
|---|---|---|---|
| cs available; any C# workspace | Gate (`ra`-only) → `get_lsp_refined_callers` → `start_ready_caller_lsp`: spawn → initialize → **readiness wait with NO didOpen and NO requests** → with tethys's actual advertised capabilities (`window.workDoneProgress`, `positionEncodings`, `serverStatusNotification` only — **no** `workspace.configuration`, transport.rs:841-858) csharp-ls sends **nothing at all** after initialize `[S]` (probe R4: with the config advert added, only the probe-induced `workspace/configuration` arrived, then 70 s total silence `[E]`) → `wait_for_solution_load` blocks in the **first read, which has no deadline** (tethys-24tj) → the 60 s timeout never fires (between-messages check) → **`callers --lsp` hangs indefinitely for C# symbols** | none (function never returns; LSP never queried) | `[S]` process hang; no output; Ctrl-C required |
| cs not installed; ra installed | Gate passes → `LspClient::start` fails → warn + **indexed-only fallback** | none | `[S]` exit 0; stderr warn "LSP server failed to start — returning indexed callers only …" |
| cs not installed AND ra not installed | Gate fails first | — | `[S]` exit 1, ra error (C# never reached) |
| (comparison) Rust symbol, ra installed | ra emits `experimental/serverStatus` ~60 ms after initialize **without any request** `[S]` (transport.rs docs; tethys-24tj) → wait succeeds → `find_references` → merge | callers found by ra and absent from index | `[S]` exit 0, "(with LSP)" heading |

### 1.3 `tethys impact --lsp`

| State | Flow (verified) | LSP-contributed results | Output / diagnostics / exit |
|---|---|---|---|
| any | The `lsp` flag **only** runs `ensure_lsp_if_requested` (`cli/impact.rs:19`); `get_symbol_impact` (`lib.rs:472`) is pure indexed — no LSP code path exists in impact | none, by construction (identical for Rust) | ra absent → `[S]` exit 1 gate failure even though no LSP work would occur; ra present → byte-identical to `impact` without `--lsp`, exit 0 |

## 2. The exact Rust-only gate divergence

Library side is per-language and correct:

- `AnyProvider::for_language(Language::CSharp)` → `CSharpLsProvider` (`src/lsp/provider.rs:189-205`),
  command `csharp-ls`, readiness `ReadinessWait::SolutionLoad` (`provider.rs:118-131`).
- Pass 3 runs a C# session unconditionally in `indexing.rs:482-498` (any language with
  unresolved refs), degrading per-language (`LspOutcome::ServerUnavailable` etc.).

CLI side is Rust-only and runs **before** everything:

- `src/cli/mod.rs:76-104` `check_lsp_availability()` hard-wires `RustAnalyzerProvider`
  (`let provider = RustAnalyzerProvider;`) and shells out to `which rust-analyzer`.
- `ensure_lsp_if_requested(lsp)` (`cli/mod.rs:114-120`) maps failure to
  `tethys::Error::Config(e.to_string())`; `main.rs:283-297` prints `error: …` and returns
  `ExitCode::FAILURE` (1).
- Invoked first by all three commands: `cli/index.rs:18`, `cli/callers.rs:21`, `cli/impact.rs:19`.

Consequences:

1. **On a csharp-ls-only machine** (ra absent), every `--lsp` command fails at preflight with
   exit 1, before any C# code runs — despite the library supporting csharp-ls. The C# Pass-3
   and caller-refinement paths are unreachable from the CLI.
2. **On a ra-present machine**, the gate is a no-op for C# and the library dispatch decides:
   C# sessions start/fall back per-language. So the gate's language coupling is the sole
   divergence point; there is no C#-specific CLI availability check anywhere.
3. `impact --lsp` fails on a ra-only machine **even though it performs no LSP work** — the
   flag is gate-only for both languages (`impact.rs` never reads `lsp` after the gate; the
   `--lsp` flag also `conflicts_with = "symbol"`, main.rs).

Seam note: the gate lives in the language-neutral CLI layer (`cli/mod.rs`), not in
`resolve.rs`/`indexing.rs`/`batch_writer.rs`, so a per-language availability check would not
violate `tests/seam_lint.rs` fences — but the ticket's non-goal is fixing code; this is
recorded as the divergence to decide in the parent ticket tethys-8axz.

## 3. Tethys source evidence (paths → symbols)

- Gate: `src/cli/mod.rs:76-104` (`check_lsp_availability`, `RustAnalyzerProvider` only, `which`),
  `:114-120` (`ensure_lsp_if_requested` → `Error::Config`). Tests `:127-184` assert the
  nonexistent-command error path for rust-analyzer only.
- Exit mapping: `src/main.rs:237-297` (`ExitCode::FAILURE` on error; affected-tests owns 0/1/2).
- Command bodies: `src/cli/index.rs:14-18` (gate then `IndexOptions::with_lsp()`),
  `:100-104` (`LSP resolved: {total} references via LSP`), `:162-185`
  (`print_lsp_session_errors`: stderr `LSP error: <lang> - …` + `hint:`);
  `src/cli/callers.rs:19-21` (gate), `:65-67` (`CallerMode::LspRefined` when `lsp`);
  `src/cli/impact.rs:17-19` (gate only; `lsp` unused afterwards — `:22-45`).
- Provider registry: `src/lsp/provider.rs:118-131` (`CSharpLsProvider`), `:189-205`
  (`AnyProvider::for_language`), `:290-300` (readiness-dispatch fence test, tethys-2mjj).
- Pass 3: `src/indexing.rs:471-500` (per-language sessions, `options.lsp_timeout_secs()`);
  `src/resolve.rs:688-901` (`resolve_via_lsp`): unresolved filter by extension `:698-711`,
  pre-open didOpen loop `:768-831`, readiness wait `:835-851` (comment: cold-workspace
  empty-result problem), query loop `:853-868`, shutdown `:884-893`;
  `src/resolve.rs:904-1017` (`resolve_single_ref_via_lsp`: `goto_definition`, line-only
  match-back fence, `ResolutionStrategy::Lsp` bind); `src/resolve.rs:1122-1186`
  (`start_ready_caller_lsp`: **no didOpen before `wait_until_ready`**, fixed
  `DEFAULT_LSP_TIMEOUT_SECS`); `src/resolve.rs:1189-1264` (`get_lsp_refined_callers`:
  indexed callers → `AnyProvider::for_language(symbol_file.language)` → merge, dedup by
  symbol id, out-of-workspace/unindexed skipped).
- Timeouts: `src/types.rs:905` (`DEFAULT_LSP_TIMEOUT_SECS = 60`), `:949-985` (`with_lsp`,
  `TETHYS_LSP_TIMEOUT` env), `src/lsp/transport.rs:32` (`DEFAULT_RESPONSE_TIMEOUT = 60`),
  `transport.rs:494-606` (`wait_for_solution_load`: `starts_with("Loading workspace")`,
  between-message timeout check, acks server requests), `transport.rs:607-641`
  (`wait_until_ready` dispatch), `transport.rs:721-750` (`find_references`),
  `transport.rs:299-363` (`read_response` timeout), `transport.rs:281-297` (`read_message`
  — blocking reads, no deadline; tethys-24tj).
- Pass-3 candidate set: `src/db/references.rs:139-151` (`get_unresolved_references_for_lsp`:
  `symbol_id IS NULL AND reference_name IS NOT NULL`) — LSP never re-verifies Pass-2 binds
  (tethys-k543).
- Outcomes: `src/types.rs:1106-1197` (`LspSessionResult`/`LspOutcome`/`LspCompletedSession`;
  `has_errors`, `server_exited_cleanly`).
- LSP language id: `src/types.rs:316-322` (`Language::as_str` → `"csharp"`).
- No C# LSP integration test: `tests/lsp_*.rs` (`lsp_resolution.rs`, `lsp_callers.rs`,
  `lsp_multi_crate.rs`) are all `#[ignore]`d and rust-analyzer-only; no `.cs` fixture `[E]`.
- CI never installs csharp-ls: `.github/workflows/` contains only `ci.yml` + `release.yml`;
  no dotnet/csharp-ls steps `[E]`. tethys-xpc4 (open) proposes a nightly lsp-smoke job
  installing rust-analyzer only.

## 4. Upstream csharp-ls evidence

Pinned: `razzmatazz/csharp-language-server` @ `b714e27ca7608b1c77fa77caa5f0bb9330fdab49`
(v0.26.0, 2026-07-15), MIT, author Saulius Menkevičius.

- On-demand load: `Runtime/ServerStateLoop.fs:170-184` (`LoadWorkspaceFolder` → posts
  `ProcessSolutionAwaiters` → `workspaceLoadingStarted`); `Lsp/Workspace.fs:188-232`
  (load kicks off only for `Uninitialized` folders; `Async.Start`), `:234-252`
  (awaiters released on `Loaded`/`Defunct` only). Nothing at initialize/initialized starts
  the load. `Handlers/DocumentSync.fs:181-183`: `didOpen` calls `LoadWorkspaceFolder` —
  **the first didOpen is what triggers the load** (probe-confirmed `[E]`).
- Progress title: `Lsp/WorkspaceFolder.fs:725-727` — `Loading workspace folder "<dir>"[ , solution "<name>"]..`;
  end message `Finished loading workspace folder "<dir>"` (`:733-734`); the load raises
  before `End` when no project files exist. tethys's `starts_with("Loading workspace")`
  matches this title (transport.rs:522-530) `[S]/[E]`.
- No-project crash: `Roslyn/Solution.fs:339-382` (`solutionFindAndLoadOnDir`: `*.sln`,
  `*.slnx`, then `*.csproj`/`*.fsproj`; `:377` `Exception message |> raise` when none);
  unhandled → process abort. `[E]` probe B: `Unhandled exception. System.Exception: no or
  .csproj/.fsproj or sln files found on /tmp/csharp-probe/B`, exit −6 (SIGABRT).
- Defunct (non-crash) path: `Roslyn/Solution.fs:270-286` catches load exceptions in
  `solutionTryLoadOnPath` → `showMessage` → `None`; `Lsp/WorkspaceFolder.fs:740` →
  `Defunct "Solution could not be loaded on path …"`; handlers then return `null`
  (`Handlers/Definition.fs:48`, `Handlers/References.fs:96-97` `| _, _ -> success None`).
  Reachable only via explicit bad `--solution` / `csharp.solutionPathOverride` — tethys never
  passes `--solution` (provider args are empty, provider.rs:127-133) `[S]`.
- Request gating during load: `Handlers/References.fs:55-59` (`LoadWorkspaceFolder` then
  `match wf, solution`); first query latency ≈ remaining load time `[E]` (slow probe:
  references answered ~10.5 s after send with an 8 s `solutionLoadDelay`).
- Position encoding: `Lsp/Server.fs:70-118` (`getServerCapabilities` — no
  `positionEncoding` field) → UTF-16 per LSP default; tethys negotiates utf-8→rejected and
  converts byte→utf-16 columns (`src/lsp/encoding.rs`, tethys-2d1x) `[S]`.
- Initialize: `Handlers/LifeCycle.fs:49-75` (rootUri → rootPath → CWD fallback for the
  workspace folder); the server pulls `workspace/configuration` ("csharp") right after
  initialize **only when the client advertises `workspace.configuration`**
  (`LifeCycle.fs:145-148` `configurationSupported`; else "client does not support
  workspace/configuration, skipping") — tethys does **not** advertise it
  (transport.rs:841-858), so no config request occurs in real tethys flows; the config
  request in the probe transcripts is probe-induced (`[E]` probe advert;
  `[S]` skip path). A null response would mean default `CSharpConfiguration` (analyzers
  off, `solutionPathOverride` None) `[E]`.
- References: `SymbolFinder.FindReferencesAsync(symbol, solution, allDocs, ct)` with
  `includeDeclaration` honored (References.fs:59-93; changelog 0.16.0); probe:
  `includeDeclaration: false` → only the call site returned `[E]`.
- Requires .NET 10 SDK (changelog 0.21.0); install `dotnet tool install --global csharp-ls`
  (README). `--version` exits 0; clean shutdown via `shutdown`+`exit` → exit 0 `[E]`.
- Changelog anchors: 0.14.0 "Add progress reporting when loading a solution/project";
  0.20.0 "Actually ingest InitializeParams.rootPath and .rootUri"; 0.23.0 "Enable solution
  load on-demand"; 0.25.0 "Add workspace phase tracking"; 0.19.0 ".sln/.slnx selection
  heuristics".

## 5. Empirical experiment record

Driver: `/tmp/csharp-probe/probe_lsp.py` (scratch, uncommitted). Each run: spawn csharp-ls,
initialize (rootUri=fixture, advertises `window.workDoneProgress`, `positionEncodings
["utf-8","utf-16"]`, `workspace.configuration`), `initialized`, ack every server→client
request with null, record all messages with timestamps.

Fixture A (`App.csproj` net10.0; `Program.cs` calls `g.SayHello("world")`; `Lib.cs` declares
`class Greeter { public string SayHello(string name) … }`).

**Run R1 (available, `.csproj` present)** — `probe_lsp.py /tmp/csharp-probe/A ready`
(excerpt; t = seconds since spawn). Note: the `workspace/configuration` request in these
transcripts is **probe-induced** — the probe advertises `workspace.configuration`, tethys
does not (transport.rs:841-858), so real tethys flows never see it.

```
0.555 window/logMessage  csharp-ls: initializing, version 0.26.0 (Dūmiškės)+b714e27…
0.736 initialize result   capabilities: … referencesProvider: true … (no positionEncoding)
0.800 SRVREQ workspace/configuration   (acked null → default config)
6.772 POST-INIT-IDLE-6s: no progress, no load — nothing arrives after the config request
6.813 $/progress begin   "Loading workspace folder \"/tmp/csharp-probe/A\".."   ← first didOpen
6.822 window/logMessage  csharp-ls: attempting to find and load solution on path …
6.834 window/logMessage  csharp-ls: 0 solution(s) found: []
6.835 window/logMessage  csharp-ls: no single preferred .sln/.slnx file found …; fill load project files manually
6.839 window/logMessage  csharp-ls: looking for .csproj/fsproj files on …
7.508 window/logMessage  csharp-ls: loading project "/tmp/csharp-probe/A/App.csproj"..
8.544 $/progress end     "Finished loading workspace folder \"/tmp/csharp-probe/A\""
9.262 textDocument/references (Lib.cs 2:18, includeDeclaration:false) →
      [{"uri":"file:///tmp/csharp-probe/A/Program.cs","range":{start:{line:5,character:10},end:{line:5,character:18}}}]
9.279 textDocument/definition (Program.cs 5:10) → Lib.cs 2:18-26 (SayHello declaration)
9.632 shutdown → exit 0
```

**Run R2 (workspace-load failure, loose `.cs` only)** — `probe_lsp.py /tmp/csharp-probe/B noproj`:

```
0.738 initialize result; 0.768 SRVREQ workspace/configuration (acked)
6.773 POST-INIT-IDLE-6s
6.802 SRVREQ window/workDoneProgress/create      ← first didOpen
6.814 $/progress begin   "Loading workspace folder \"/tmp/csharp-probe/B\".."
6.822 logMessage attempting to find and load solution … 6.837 "0 solution(s) found: []"
6.840 logMessage no single preferred .sln/.slnx … 6.841 looking for .csproj/fsproj …
6.843 logMessage no or .csproj/.fsproj or sln files found on /tmp/csharp-probe/B
   → stderr: "Unhandled exception. System.Exception: no or .csproj/.fsproj or sln files
     found on /tmp/csharp-probe/B … at Roslyn/Solution.solutionFindAndLoadOnDir … line 377"
36.863 TIMEOUT waiting response to textDocument/references   (request sent at ~6.8 s, never answered)
38.864 PROC_POLL −6        (SIGABRT; no "end" progress ever sent)
```

**Run R3 (slow load)** — `probe_lsp.py /tmp/csharp-probe/A slow` (sets
`workspace/didChangeConfiguration` `csharp.debug.solutionLoadDelay: 8000` before queries).
Probe-only knob: tethys never sends configuration, so real-world slowness is a large
solution; the load-gating mechanism shown here is identical.

```
15.286 $/progress begin   "Loading workspace folder \"/tmp/csharp-probe/A\".."   ← 8.0 s after first didOpen
17.203 $/progress end     "Finished loading workspace folder \"/tmp/csharp-probe/A\""
18.338 textDocument/references answered   ← first query held ~11 s by the server (8 s delay + 2 s load)
18.388 shutdown → exit 0
```

**Run R4 (callers-style: initialize + acks only, no didOpen, 70 s wait)** — fixture A:

```
messages during 70 s wait: 4 — two window/logMessage (startup), initialize response,
workspace/configuration (acked). Then 70 s of total silence. No progress, no load.
```

The `workspace/configuration` here is probe-induced (probe advertises the capability);
tethys's capabilities (transport.rs:841-858) trigger **zero** post-initialize messages
`[S]` — the silence is total from the start.

Cross-checks: `references` result for a position on the return type token (`string`) returned
the two `string` occurrences (mechanism sanity); `includeDeclaration: false` excludes the
declaration `[E]`.

## 6. Which results are LSP-contributed (summary)

- `index --lsp`: LSP contributes exactly the Pass-2-unresolved refs it binds via
  `goto_definition` (candidate set `symbol_id IS NULL`, db/references.rs:139-151). The CLI
  reports the count (`LSP resolved: N references via LSP`); all other refs/edges are
  tree-sitter-derived. `[S]` (candidate set, bind, output) + `[E]` (server answers
  definition/references correctly at name positions).
- `callers --lsp`: intended LSP contribution = `find_references` locations merged with
  indexed callers (dedup by symbol id; out-of-workspace/unindexed skipped). For C# this is
  unreachable in practice: the readiness wait precedes any didOpen/request, csharp-ls
  emits nothing after initialize with tethys's capabilities (no `workspace.configuration`
  advert — transport.rs:841-858; probe R4 `[E]` for the probe-advert case), and the
  blocking read hangs the command (tethys-24tj). `[S]+[E]`.
- `impact --lsp`: no LSP contribution exists by construction (flag is gate-only). `[S]`.

## 7. Unproved empirical cells (no tethys binary in the worktree)

The following remain source-derived only; each is exactly what a follow-up run with a built
tethys (same commit) against fixtures A/B should verify:

1. `index --lsp` integrated happy path: actual stderr/stdout text, "LSP resolved" count
   rendering, session stats, exit 0 — including the readiness wait actually observing the
   begin/end progress (ordering proven: pre-open didOpen precedes wait, resolve.rs:768-851;
   server side proven in R1).
2. `index --lsp` on fixture B: that tethys's wait surfaces `ServerExited` and the query loop
   accumulates N errors with refs left unresolved, and that the command still exits 0.
3. `index --lsp` with csharp-ls removed from PATH (or `--lsp-timeout 1`): stderr
   `LSP error: csharp - …` + hint rendering, and exit code.
4. `callers --lsp` on a C# symbol: the integrated hang (server emits nothing at all with
   tethys's capabilities `[S]` — stronger than R4's probe-advert silence; transport
   blocking reads are source-proven; the combined hang is inferred `[I]`).
5. `callers --lsp` / `impact --lsp` gate failures on a ra-less PATH (exit 1, exact message).
6. csharp-ls with a restore-failing project (PackageReference without restore) or a
   multi-project `.sln`; `--solution` override via `csharp.solutionPathOverride` (Defunct
   path, upstream Solution.fs:270-286) — not probed.
7. Slow-load boundary: load > 60 s with messages flowing (readiness `Ok(false)` at a message
   boundary) vs. load > 60 s with silence (blocked read, tethys-24tj) — not probed.

## 8. Unresolved risks / open questions for the decision ticket (tethys-8axz)

- The `callers --lsp` C# hang couples two independent defects: csharp-ls's load-on-demand
  (nothing fires until a request) and tethys's deadlineless reads (tethys-24tj). Fixing
  either changes the observable: a didOpen-before-wait (as Pass 3 already does) would make
  the wait succeed; read deadlines would turn the hang into a bounded fallback.
- csharp-ls crashes (SIGABRT) on project-less workspaces — a workspace shape tethys itself
  can index fine via tree-sitter. Pass 3 degrades to errors, not a crash, but the cell
  (exact error stream, counts) is unproved (§7.2).
- The CLI gate requires rust-analyzer for C#-only workflows; the C#-only machine cell
  (§7.5) is unproved.
- `impact --lsp` is a no-op flag for both languages (gate only); decide whether the C# parity
  contract should keep or drop it.
- csharp-ls version drift: behaviors above are pinned to 0.26.0/b714e27c; upstream 0.14→0.25
  changelog shows load/progress semantics changed repeatedly (on-demand since 0.23.0).

## 9. Sources consulted

- Tethys worktree `/tmp/tethys-research-lsp` @ 7ca48f8: paths/symbols cited in §3.
- Upstream source clone `/tmp/csharp-language-server-src` @ b714e27c (razzmatazz/csharp-language-server):
  files cited in §4; CHANGELOG.md; README.md; installed nuspec (repository/commit pin).
- Installed tool: `~/.dotnet/tools/csharp-ls` 0.26.0; SDK 10.0.102.
- Probe artifacts (scratch): `/tmp/csharp-probe/{probe_lsp.py, A/, B/, ready.out, noproj.out,
  slow.out, stderr.log, stderr-silence.log}`.
- Tracker: tethys-sn05 (this ticket), tethys-8axz (parent decision), tethys-24tj (read
  deadlines), tethys-k543 (Pass-3 candidate set), tethys-xpc4 (nightly lsp-smoke, ra-only),
  tethys-6x7g (CSharpLsProvider shipped; CLI gate residual), tethys-2mjj (readiness waits),
  tethys-2d1x (UTF-16 positions).
