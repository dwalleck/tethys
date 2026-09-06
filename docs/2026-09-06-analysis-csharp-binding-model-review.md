# Review: the approved C# binding model against a real legacy repository

Date: 2026-09-06. Reviewed: `prototype/tethys-07eh-binding-model@138c6a6`
(`.csharp-parity/prototypes/binding-model/index.html`, `observations.json`,
`.csharp-parity/research/binding-semantic-host.md`), the sibling prototype
`prototype/tethys-chlt-project-model@c98c726`, the four `research/csharp-parity-*`
branches, and the full C#-parity ticket chain under `tethys-4015` (07eh, chlt,
rvr5, teax, lyl6, umjq, oxds, 8axz, jp80, rduz, a75r, ff9k, sgq2, xfhp, sn05,
zqw3, f1rt, jmj0) plus the open C# gap tickets they cite.

Reference repository: a private legacy .NET solution, referred to below as the legacy reference repository; its identity and every identifier behind the neutral phrases used here are in the gitignored `docs/private/legacy-reference-repo.md`. Shape: 55 classic non-SDK
`.csproj` + 17 `.vbproj`, 1,696 `.cs` and 378 `.vb` files, .NET Framework 4.8,
60 `packages.config`, ASP.NET Web Forms, WCF service references, LINQ-to-SQL
designer files. A modern SDK-style sibling repository (net8.0, 300 files) was indexed for
contrast.

Supporting reports produced for this review (same directory):

- `2026-09-06-csharp-extractor-capability-matrix.md` — construct-by-construct
  audit of the shipping tree-sitter C# path, every claim cited to `file:line`
  and confirmed with `sqlite3` rows on throwaway fixtures.
- `2026-09-06-csharp-semantic-route-alternatives.md` — primary-source research
  and Linux probes of scip-dotnet, Roslyn LSIF, CodeQL, `MSBuildWorkspace`,
  `complog`, documentation-comment IDs, legacy hosting, and VB interop.
- `docs/private/reference-repo-project-graph-probe.py` (gitignored) — the script
  that checks index bindings against the `ProjectReference` graph.

---

## 1. Verdict

**Is there a working model?** As a *contract* the twenty approved choices are
coherent and mostly right: evaluation-unit identity, compile-time selection,
direct-call versus possible-dispatch, accessor effects, explicit grants,
fail-closed standing. As a *plan for reaching parity on repositories like
the reference one* it is not yet working, for three reasons that are not visible in the
prototype but show up immediately against the real repository:

1. **The VB boundary.** 17 C# projects in the reference repository reference VB projects; 89 C#
   files `using` VB-declared namespaces; 656 C# references target VB-only types.
   The map declares VB out of scope and "no cross-language binding". A Roslyn
   compilation of those C# projects without VB metadata has an error on every
   VB type use and, under the approved Q17 rule, can never be Confirmed. The rule
   conflates Rust↔C# isolation (correct) with C#→VB metadata references (a
   same-runtime, compiler-supported reference). Without an amendment,
   compiler-backed parity is unreachable for roughly a third of this repository.
2. **`packages.config` cannot be restored by the dotnet CLI** (verified:
   "Nothing to do" under every flag combination; NuGet docs say packages.config
   restore is MSBuild-16.5+/Windows/Mono only). The legacy reference repository has 295 distinct
   packages across 60 `packages.config` files and its `src/packages/` folder is
   not populated on this machine. On Linux the approved "restore grant" is
   unsatisfiable for this repository; the only workable contract is "packages
   folder must already be present, else `restore-required`".
3. **Evaluation-only discovery cannot produce compiler inputs for classic
   projects.** Framework references (`System`, `System.Core`, 24 of 96
   `Reference` items in one class-library project have no `HintPath`) are
   resolved only by the `ResolveAssemblyReferences` target or by a tethys-owned
   targeting-pack resolver. The prototype models "target grant required" as a
   free toggle; in practice it is required for every classic unit, so the
   "semantic mode without target execution" path in the model is empty for this
   repository class.

**Are there easier ways?** Yes, and they keep every non-negotiable of the
contract. Roslyn already ships the out-of-process, host-selecting,
design-time-build "compiler-input acquisition" stage the plan proposes to build
(`Microsoft.CodeAnalysis.Workspaces.MSBuild` BuildHost, out-of-proc since
Roslyn 4.8, 2023). A 60-line sidecar on it opened a generated classic Web Forms
csproj + vbproj solution on Linux with three MSBuild properties and produced a
0-error C# compilation with the VB project bound. `complog` (Basic.CompilerLog)
turns the repository's own Windows build log into a portable artifact from which
both compilations were rebuilt in 27 lines with no MSBuild execution by tethys.
Documentation-comment IDs give a spec-defined, overload-disambiguating symbol
identity for free. Details in §6.

**How is tethys handling C# today?** Usefully but shallowly. On the reference repository the
tree-sitter path indexes 1,696 files in 3.4 s and resolves 32 % of references,
but it has no project identity, no namespace in any qualified name, no
receiver awareness, no overload identity, no type references, and it binds
`this.Invoke` in a WCF proxy to an OWIN middleware in an unrelated project.
The capability matrix (§3) is the honest baseline the roadmap should be
measured from.

---

## 2. What the prototype does and does not demonstrate

`index.html` is a 38-field in-memory state machine with an `assess()` function
and ten guided scenarios. It illustrates the *vocabulary* of the approved
contract well. It does not test the contract, and its evidence claim is weaker
than the ticket resolution states:

- **Free-play observations are vacuous.** All 10 free-play toggles report
  "Inactive syntax only" because state accumulates across clicks and the first
  toggle (`active=false`) is never undone. Only the 33 guided steps show
  anything. The ticket's "33 guided steps + 11 free-play actions" evidence line
  should be read as 33.
- **Dead knobs.** `analyzerGrant`, `context` and `effectDependent` are never
  varied by any scenario or toggle; `analyzerGrant` appears in the permissions
  card but never affects `assess()`.
- **`targetRequired`, `restoreRequired`, `generatorRequired` are inputs, not
  derived facts.** The hard question, *when* does a unit need target execution,
  restore, or generator output to have trustworthy compiler inputs, is exactly
  what is left outside the model. For the reference repository the answers are "always",
  "always", and "never" (all generated code is checked in).
- **The dependent/unrelated compiler-error split is a stipulated boolean.** Roslyn
  gives concrete evidence for it (`SymbolInfo.CandidateReason`,
  error-type symbols in signatures, `IsImplicitlyDeclared`); the model should
  name the rule so implementation cannot drift.
- **Scenarios missing that the reference repository makes mandatory:** referenced project output
  unavailable (unbuilt `ProjectReference`), other-language `ProjectReference`,
  checked-in generated code (`.designer.cs`, `Reference.cs`, `<auto-generated>`),
  `InternalsVisibleTo` (7 files in the reference repository), global/implicit using context
  (approved Q10 but not modeled), the same physical file active in two units at
  once, extension-method reduced form, explicit interface implementation.
- **Native mode can "confirm" with derived (generated) inputs** without any
  generator grant. Minor, but it contradicts Q11.

The research document `binding-semantic-host.md` is accurate and well cited. Its
one important omission is that `MSBuildWorkspace`'s BuildHost *is* the
"separately authorized out-of-process acquisition stage" the ticket asks tethys
to investigate; the document mentions it only to reject it as "reintroducing
MSBuild", which is also true of the plan's own acquisition stage.

---

## 3. Where the shipping C# path actually stands

Numbers from `tethys index --rebuild` on the reference repository with the worktree's release
build (3.4 s, 17.5 MB index) and from the fixture audit.

### 3.1 Reference-repository baseline

| Measure | Value |
|---|---|
| Symbols | 26,666 (property 9,273; method 6,473; field 5,946; class 1,931; namespace 1,637) |
| References | 70,933; resolved 22,813 (32 %) |
| Resolution by strategy | same_file 17,765; import_union 2,403; qualified_exact 2,116; unique_workspace 529 |
| `module_path` for C# symbols | empty for all 26,666 |
| `arch_packages` | empty; `coupling` prints "requires a Cargo workspace" |
| Qualified names | never include the namespace (`Constants`, not `Some.Namespace.Constants`) |
| Simple type names declared in more than one file | 223 of 1,976 (`Program`×7, `Constants`×7, `Settings`×6, a payment-source enum ×7) |
| References naming a colliding type | 1,961; 954 (49 %) decline |
| `Constants.X` references | 30, all unresolved (7-way collision plus consts not indexed) |
| `affected-tests` for one of the `Constants.cs` files | "No tests affected" (false clean) |
| Cross-project bindings | 2,956; 55 violate the `ProjectReference` graph (no path from referencing project to declaring project) |
| Call edges | 8,386; 35 violate the project graph |
| Declared csproj→csproj `ProjectReference` edges witnessed by at least one `file_dep` | 87 of 166 (52 %) |
| `same_file` binds in files with more than one same-named symbol (overload last-wins exposure) | 9,371 of 17,765 (53 %) |
| `qualified_exact` binds whose target qualified name is duplicated (first-row exposure, tethys-bvgb) | 293 |
| Generated files (`*.designer.cs`, `*.Designer.cs`, `Reference.cs`, `.g.cs`) | 36 files holding 30 % of symbols and 20 % of references; 102 files carry `<auto-generated>` |
| `dead-code` | 3,186 candidates, 354 Definite; the Definite property/method sample is entirely resx `Designer.cs` accessors and LINQ-to-SQL designer methods |
| C# references whose first segment is a VB-only type | 656, all unresolved |

Two concrete phantoms worth keeping as fixtures:

- `this.Invoke("…", …)` in an ASMX proxy `Reference.cs` inside a class
  library (a `SoapHttpClientProtocol` subclass) binds via `unique_workspace`
  to an OWIN middleware's `Invoke` in an unrelated web project — 28 call sites,
  wrong project, wrong type, no `ProjectReference` path. The extractor drops
  `this.`/`base.` receivers, so the name is bare and workspace-unique.
- `CurrentUser.MappingService = …` where `CurrentUser` is a *property of
  interface type* binds via `qualified_exact` to the member of the *class* that
  shares the property's name (13 sites). The same
  member reached through a locally named variable (7 sites) stays unresolved.

The modern sibling repository resolves 35 % of references; its unresolved
call/construct references with an in-index name number only 69, so on SDK-style
code the losses are almost entirely external/BCL, whereas on the reference repository the
collision and VB classes dominate.

### 3.2 Capability matrix (condensed; full version in the extractor report)

| Area | Extracted / bound today | Not extracted / wrong |
|---|---|---|
| Declarations | namespace (block and file-scoped), class, struct, interface, enum, record (as Class), delegate, method, ctor (as Method named like the type), property, event, field, const | enum members, indexers, operators, conversion operators, destructors, local functions, top-level statements (zero symbols), positional record properties, `record struct` (stored as Class), generic arity (`Box<T>` and `Box` share a name), all modifiers except `static` |
| References emitted | `call`, `construct`, `field_access`, `inherit` only | no `Type` references at all (params, locals, returns, generics, `typeof`/`is`/`as`/casts/`catch`/`where`), no implicit-this reads, no `?.`, no indexers, no object-initializer members, no `new()`, no bare generic invocation `Foo<int>()`, `nameof(X)` emits a bogus `call nameof` |
| Receivers | `obj.Foo()` stored as `obj::Foo` (never resolves cross-file) | `this.`/`base.` dropped → `base.Hello()` binds to the override itself (self-loop call edges) |
| Usings | plain `using Ns;` feeds `import_union` (types only) and `using static` member calls | alias stored but never consulted; `global using` never propagates; `static`/`global` flags and lexical scope lost; single-identifier alias target stored as empty string; dotted namespace aliases over-resolve as globs |
| Preprocessor | both `#if` branches indexed at namespace level | member-level `#if` drops the members but keeps their body references with NULL `in_symbol` |
| Resolution arms for C# | same_file, import_union, qualified_exact, unique_workspace | `explicit_import` never (all rows are `*`), `same_crate` never, `qualified_module_fallback` never → C# references can never reach the `high` band |
| Identity | file-local qualified name `Parent::Name` | no namespace, no project, no overload signature, no generic arity |
| Visibility | public / protected internal / private protected / internal / protected | interface members and explicit interface implementations stored `private` → dead-code false positives |
| Analyses | index, search, stats, callers, impact, cycles, reachable, affected-tests, deprecated-callers, untested-code, dead-code, hierarchy run | coupling, visibility-tightening, unused-imports are Rust-gated; panic-points inapplicable |

The audit also confirms all eleven cited gap tickets are still open and adds
twenty-two unticketed gaps (extractor report §8), of which the receiver-blind
Pass-1 binding, the missing `Type` reference kind, and the private-by-default
interface members most affect maintainer-visible outcomes.

---

## 4. Gaps in the approved contract, measured against the reference repository

Numbered so the ticket amendments in §7 can cite them.

**G1 — VB references block parity (07eh, 4015).** See §1. Verified with Roslyn
5.3: `CSharpCompilation` rejects a `VisualBasicCompilationReference`
(`ArgumentException`) but a metadata-only `Emit` of the VB compilation (3 KB
skeleton) binds cleanly, and `MSBuildWorkspace` does this automatically. The
minimum amendment: *VB projects are semantic inputs (metadata), not indexed
sources*. VB-scoped queries stay Indeterminate; C# bindings to VB members are
external semantic targets with VB-unit provenance.

**G2 — packages.config restore is Windows/Mono-only (rvr5).** The restore grant
cannot be honored on Linux for classic projects. The contract needs a third
outcome beside `restore-required`/`restore-failed`: `restore-unsupported-on-host`
with the guidance "commit or pre-populate `packages/`". Mono is end-of-life
(stewardship moved to WineHQ, 2024), so it should not be the answer.

**G3 — evaluation-only discovery yields no compiler references for classic
projects (chlt, 07eh Q15).** `dotnet msbuild -getItem:Reference` returns
`HintPath` for package references but nothing for framework assemblies. Either
target execution runs (`ResolveAssemblyReferences`) or tethys reimplements a
slice of RAR against a targeting pack. The Linux recipe that works without
Windows or Mono (verified end to end, including `dotnet build` of csproj and
vbproj): `VSToolsPath=` (blank) + `Microsoft.NETFramework.ReferenceAssemblies.net48`
targets via `CustomAfterMicrosoftCommonTargets` + `FrameworkPathOverride`. This
"classic on Linux" profile should be data in the adapter, not a qualified host.

**G4 — Linux CI is permanently Indeterminate for the reference repository under rvr5.** All 11
web projects (`Microsoft.WebApplication.targets` import) fail evaluation on
Linux with MSB4019; 61 of 72 projects evaluate as-is, 72 of 72 with
`-p:VSToolsPath=`. rvr5 says "a workspace-wide query is Indeterminate if any
applicable unit is not confirmed", so every workspace-wide query on the reference repository in
a Linux job would exit 2 forever. The contract needs an explicit, recorded
global-property/import-tolerance policy (the research already notes
`ProjectLoadSettings.IgnoreMissingImports`) rather than treating VS-only imports
as unqualifiable.

**G5 — no generated-code policy anywhere (07eh, lyl6, rduz).** 30 % of the reference repository's
symbols are in designer/proxy files and the Definite dead-code list is made of
them. Roslyn has `GeneratedCodeAttribute`, `<auto-generated>` headers, and
`GeneratedKind`; the model needs an origin tag on declarations and a default
exclusion for dead/untested/visibility findings, distinct from the derived
(in-memory generated) inputs the model already discusses.

**G6 — the oracle corpus contains no classic repository (umjq).** All four
pinned repositories (Avalonia, xUnit v3, Newtonsoft.Json, MSBuild) are SDK-style.
The day-one legacy gate has no oracle. legacy-shaped fixtures (classic csproj,
packages.config, vbproj reference, Web Forms, WCF proxies) must be added, and
the local Windows test VM is the natural place to run the VS-MSBuild leg.

**G7 — per-invocation reevaluation cost (chlt Q6).** Evaluating all 72 projects of the reference repository with `dotnet msbuild` took 11.9 s on this machine (0.16 s each) against
a 3.4 s full index. Acceptable for `index`, questionable for every `reindex`;
the plan should allow a recorded-context cache keyed on project-file and import
mtimes, with reevaluation on change.

**G8 — no canonical symbol identity was chosen (07eh Q20).** Documentation
comment IDs (`M:Ns.Type.Method(System.String)`) are spec-defined,
overload-disambiguating, cross-tool stable, and cover ctors, indexers,
operators, explicit interface implementations and generic arity. Verified
holes: locals return `null`, lambdas are nameless, and **local functions get an
ID that omits the containing method and collides** (round-trip returns 0
symbols). Recommendation: key symbols by `(evaluation_unit, doc_comment_id)`
and synthesize document-local IDs for lambdas, local functions, locals and
parameters.

**G9 — `qualified_exact` first-match (tethys-bvgb, P4) is a live phantom source
on C#.** 293 exposures on the reference repository; the property-named-like-its-class case above is one.
It belongs in the roadmap's must-close binding-correctness group next to
tethys-0aqj, not at P4.

**G10 — the roadmap grouping omits the extractor gaps that dominate the reference repository
outcomes.** Receiver dropping (`this.`/`base.`), the missing `Type` reference
kind, interface members stored `private`, explicit interface implementations
as duplicate private methods, and overload identity are unticketed. Under a
compiler-backed path most disappear, but the no-host path stays the default and
the scorecard's "Degraded" rows are driven by exactly these.

**G11 — mixed-language invariants (a75r) need a third language class.** The
fixture asserts Rust↔C# never bind. It should also assert C#→VB *does* bind
through metadata when the VB unit is present and declines with a VB-specific
reason when it is not, so the two rules cannot be confused later.

**G12 — the OmniSharp readiness ticket (sgq2) is now low value.** Both
OmniSharp and csharp-ls sit on the same `MSBuildWorkspace`; on Linux both hit
the same MSB4019/MSB3644 pair for classic projects and need Mono for the net472
host. Once tethys owns a BuildHost sidecar, an LSP-based legacy refinement host
adds a second, weaker copy of the same machinery.

---

## 5. What the approved plan would have to build, and what already exists

| Plan component | Approved plan | Already available |
|---|---|---|
| Discovery (evaluation-only, project × TFM, provenance) | tethys MSBuild adapter | `dotnet msbuild -getItem/-getProperty` (used by the chlt probes); keep |
| Compiler-input acquisition (design-time targets, host selection, SDK vs classic vs Mono, fallbacks) | tethys out-of-process adapter, host matrix qualification | Roslyn `Workspaces.MSBuild` BuildHost: out-of-proc, host kinds `NetCore`/`NetFramework`/`Mono`, fallback with logged warning, `ProvideCommandLineArgs` capture, per-TFM `Project`s, VB skeleton references |
| Zero-execution acquisition from an existing build | not considered | `complog create` from the repository's own binlog; rebuilds C# and VB `Compilation`s with references and generated files; no targets run by tethys |
| Semantic walker (declarations, selected targets, accessor effects, dispatch candidates, diagnostics) | must be built | must be built in every option — this is tethys's real product |
| Symbol identity | undecided | documentation-comment IDs + unit qualifier |
| Rust ingestion (`strategy=roslyn`, standing, provenance) | must be built | must be built in every option |
| Grants, no-silent-downgrade, reconciliation against discovery | must be built | must be built; cheaper with BuildHost because inputs are materialized objects, not parsed command lines |

Roughly 1.5–3 k lines of the riskiest, least differentiating C# (host matrix,
MSBuild API versioning, MSB error taxonomy) disappear under the sidecar route.
The security posture is identical: design-time targets are arbitrary project
code in either design, and BuildHost is a process boundary, not a sandbox.

---

## 6. Recommendation

1. **Replace the tethys-owned acquisition adapter with a pinned .NET sidecar on
   `Microsoft.CodeAnalysis.Workspaces.MSBuild`.** Start it only under the
   target-execution grant; pass the recorded evaluation context as global
   properties; strip analyzer references unless the generator grant is present;
   reconcile its projects, documents and metadata references against the
   evaluation-only discovery snapshot and surface mismatches as Indeterminate;
   map every `WorkspaceFailed` failure to a per-unit reason with the native
   MSB/NETSDK code and record host fallbacks as provenance. Emit JSONL keyed by
   `(unit_id, doc_comment_id)`.
2. **Add `complog` ingestion as the zero-execution path** for repositories that
   already build in CI. For a Windows-built Web Forms solution this is the only
   route with no host workarounds at all, and it delivers exact compiler inputs
   and provenance. One grant: "trust this build artifact".
3. **Define the classic-on-Linux profile as data** (`VSToolsPath=`,
   reference-assemblies targets, `FrameworkPathOverride`) and fail closed with
   `toolchain-unavailable`/`restore-required` when `packages/` is absent.
4. **Amend the VB rule** to "VB units are semantic inputs, not indexed sources".
5. **Adopt documentation-comment IDs** as canonical identity with synthesized
   local IDs.
6. **Add a generated-code origin tag** and default exclusions for
   dead/untested/visibility findings.
7. **Add a legacy-shaped fixture set to the oracle corpus** and run its
   VS-MSBuild leg on the Windows VM.
8. **Keep scip-dotnet only as an occurrence/implementation oracle**; drop the
   Roslyn LSIF generator (deleted upstream 2025-06) and CodeQL (licence) from
   consideration.
9. **Reprioritize tethys-bvgb** into the must-close binding group and file the
   three unticketed extractor phantoms (receiver dropping, private interface
   members, explicit-interface duplicates) so the no-host baseline stops
   producing wrong edges on legacy code.

What does not get easier: grants and their enforcement, evaluation-unit
identity, provenance and reconciliation, Confirmed/Indeterminate standing, the
semantic walker, and the Rust ingestion. Those are the same under every option.

---

## 7. Concrete ticket amendments

| Ticket | Amendment |
|---|---|
| tethys-4015 (map) | Move VB from "out of scope" to "semantic input only"; record G1–G12 as map notes. |
| tethys-07eh | Append: acquisition stage may be Roslyn BuildHost or complog; identity = `(unit, doc-comment id)`; generated-code origin tag; dependent-error rule named (`CandidateReason`, error types); VB metadata binding permitted. |
| tethys-rvr5 | Add `restore-unsupported-on-host`; add a recorded global-property/import-tolerance policy with `VSToolsPath=` as the first entry; state that packages.config restore is never performed by tethys on non-Windows hosts. |
| tethys-chlt | Allow a mtime-keyed evaluation cache; note framework references require target execution or a targeting-pack resolver. |
| tethys-umjq | Add a classic-Framework fixture cluster (csproj + packages.config + vbproj + Web Forms + WCF proxy) and a Windows-VM leg. |
| tethys-a75r | Add the C#→VB positive control and VB-absent decline reason. |
| tethys-jp80 | Sequence: (1) discovery snapshot + `--rebuild` cutover; (2) BuildHost sidecar behind grants, SDK repo first, classic-on-Linux profile second, Windows VS leg third; (3) complog path; (4) walker vocabulary; (5) analysis consumers. |
| tethys-sgq2 | Downgrade or close: superseded by owning a BuildHost sidecar. |
| tethys-bvgb | P4 → P2, group with tethys-0aqj. |
| new | Extractor phantoms: `this.`/`base.` receiver dropping; interface members stored private; explicit interface implementations as duplicate private methods; no `Type` reference kind. |

---

## 8. Probes still worth running

- **Done later the same day:** the Windows VM probe. See
  `2026-09-06-windows-vm-legacy-probe.md`. Summary: Build Tools installed in
  3 minutes; `MSBuildWorkspace` loaded all 65 projects under VS MSBuild in
  under a minute at 700 MB. With the private-feed packages restored, 50 of 65
  projects compile clean and Roslyn binds 97.9 % of invocations, 99.3 % of
  object creations and 99.9 % of member accesses, including 680 C# calls into
  VB projects. Cross-language references are all-or-nothing under skeleton
  emit: a 6-error C# project still cost its 7 VB consumers every type (new gap
  G13 for the contract). Residual errors are repository defects worth
  fixturing (VS-only MSTest assembly, missing references, ruleset-escalated
  CS8019).
- On Linux: run the same sidecar on the reference repository with the three-property profile
  and a pre-populated `packages/` (copied from a Windows restore) to measure
  how many of the 72 units reach 0 errors, and the wall-clock and RSS cost.
- Produce one `.complog` from a Windows `dotnet build -bl` / `msbuild -bl` of
  the reference repository and time the Linux ingestion.

Side effects of this session to be aware of: two global dotnet tools were
installed by the research agent (`scip-dotnet` 0.2.14, `complog` 0.9.62 —
`dotnet tool uninstall -g <name>` to remove); the reference repository's `.rivets/index`
was rebuilt; probe artifacts live under `/tmp/legacyprobe`, `/tmp/csaudit-fixtures`,
`/tmp/reference-probe`. No repository was modified beyond these three new docs.
