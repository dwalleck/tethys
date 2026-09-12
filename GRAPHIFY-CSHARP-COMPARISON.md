# Tethys vs. graphify-csharp — Comparison & Learnings

A side-by-side comparison of [`tethys`](./README.md) (this crate) and
[`graphify-csharp`](https://github.com/zachsaw/graphify-csharp), a headless
Roslyn/MSBuild semantic indexer for C# that emits a standalone JSON graph for
coding agents. Companion to [KIROGRAPH-COMPARISON.md](./KIROGRAPH-COMPARISON.md)
and [CODE-REVIEW-GRAPH-COMPARISON.md](./CODE-REVIEW-GRAPH-COMPARISON.md); those
two compare *tree-sitter* peers, so this is the first comparison against a
**compiler-backed** peer.

> Date of comparison: 2026-09-11
> graphify-csharp revision reviewed: `66f3c03` (`docs: update README`), v0.1, MIT
> Size: 13.3 kLOC C# (5.2 kLOC in `src/Graphify.CSharp/Roslyn/`), 8.2 kLOC tests
> Tethys state reviewed: `feat/tethys-82a6-corpus` @ `bab1b55`

All line citations below were read from the pinned revision, not inferred.
Path prefix `G/` = `graphify-csharp/src/Graphify.CSharp/`.

## TL;DR

- **It is a working implementation of the route tethys already chose.** Tethys
  made the decision in **tethys-yjok** ("tethys-roslyn: first-party Roslyn
  semantic provider shipped as a dotnet tool") after the 2026-09-06 research
  round. graphify-csharp is that tool, shipped, MIT-licensed, with a
  maintainable 5.2 kLOC core — field evidence the route is buildable at that
  size, and a source we can read instead of re-deriving Roslyn idioms.
- **The highest-leverage borrow is the *catalogue of pattern-member shapes*,
  and the discipline of asking Roslyn per shape.** `foreach`, `await`, `using`,
  deconstruction, `fixed`, and `with` select members the compiler never declares,
  and tethys-yjok's record contract does not name this family at all — a walker
  built on `IInvocationOperation` alone misses real call targets. graphify gets
  most of them right, from three different sources: authoritative operation
  properties (`IWithOperation.CloneMethod`, pattern symbols, argument→formal
  parameter), speculative binding (which correctly handles extension methods),
  and name matching for protocol continuations. The name-matching stage has one
  confirmed defect — a two-way `Dispose` resolves to the public method where the
  compiler calls the explicit interface implementation (section 3, verified by
  running graphify's own walker). **Copy the shape list; use
  `GetForEachStatementInfo`/`GetAwaitExpressionInfo` and the interface-conversion
  rule for the two protocols that have dedicated APIs.**
- **Second: the outermost-`IOperation`-root guard.** Walking syntax and asking
  `GetOperation` at every node duplicates edges; graphify's one-bit ancestor
  check (`G/Roslyn/SemanticSyntaxWalker.cs:40-64`) is the fix.
- **Third: two-key identity.** graphify separates a *canonical* key (project-
  scoped, hashed into the node id) from a *reference* key built under a
  synthetic `__reference__.csproj` project. Documentation-comment ids are
  **not** assembly/unit-qualified, so tethys needs exactly this second key.
- **What it does not solve**: possible-dispatch edges (the `DispatchesTo`
  enum member is never produced), typed reference kinds, unit standing,
  grants, reconciliation against an evaluation snapshot, source generators,
  and any **classic-on-Linux profile**: no
  `VSToolsPath`/`FrameworkPathOverride`/reference-assemblies handling appears
  anywhere (`grep` is empty across `src/` and `docs/`), and the tool delegates
  loading to whatever MSBuild the host provides
  (`docs/COMPATIBILITY.md:26-36`). That is a *gap in the untested envelope, not
  a categorical failure* — a Windows host with VS Build Tools supplies
  `NetFramework` BuildHost support, and the source contains no evidence either
  way about classic projects there. tethys-cmlc's recorded three-property
  profile remains the thing graphify does not have.
- **Three of its identity choices would be actively wrong for tethys**:
  span-bearing discriminators inside the persisted key (edit-unstable across
  runs), a single `References` relation for type usage / reads / writes /
  nameof, and hard-failing the whole load on a multi-targeted project.

## What graphify-csharp is

A .NET global tool. `MSBuildWorkspace` opens the `.sln`/`.slnx`/`.csproj`, each
C# project's `Compilation` is walked with `IOperation` plus a syntax walker, and
one JSON document of `nodes`/`edges` comes out. There is no database, no query
language, no policy layer — by explicit design ("The extractor supplies the
facts. Your agent or downstream consumer decides what those facts mean",
`README.md`).

Tethys is the opposite shape (SQLite cache, kind-gated resolution, consumed
provenance bands, 16 CLI analyses). That inversion is the interesting part:
graphify is a pure "evidence layer" implementing precisely the acquisition +
binding stage tethys has already decided to buy rather than build.

`G/Roslyn/RoslynWorkspaceLoader.cs:16-28` is the whole acquisition contract:

```csharp
var workspaceProperties = new Dictionary<string, string>(StringComparer.Ordinal)
{
    ["Configuration"] = request.Configuration,
    ["DesignTimeBuild"] = "true",
    ["BuildingProject"] = "false",
};
...
var workspace = MSBuildWorkspace.Create(workspaceProperties);
workspace.RegisterWorkspaceFailedHandler(args => diagnostics.Enqueue(...));
```

then `GetCompilationAsync` per project (`:45`), and a language filter that
**skips every non-C# project** (`:40-43`). That filter is a working precedent
for tethys-015w's open amendment: *VB units are semantic inputs, not indexed
sources* — MSBuildWorkspace still supplies the VB skeleton references
automatically, so C#→VB binds while VB source is never walked.

## The same route, independently arrived at

Tethys's 2026-09-06 research concluded that Roslyn's BuildHost already *is* the
out-of-process, host-selecting acquisition stage tethys-07eh proposed to build,
and filed tethys-yjok to ship a sidecar on it. graphify-csharp confirms the
route in a shipped artifact, and fills in parts of the contract that were left
as "must be built":

| tethys-yjok contract item | graphify's equivalent | Verdict |
|---|---|---|
| unit: project, framework, host, parse options, reconciliation | `ProjectIdentity.Key = project=<relpath>\|tfm=<tfm>` (`G/Domain/ProjectIdentity.cs:15`), per-project fingerprint | **Thinner** — no host/standing/reconciliation |
| declaration: `(unit, doc-comment id)` identity | hand-rolled canonical string + SHA-256 node id (`G/Domain/NodeId.cs:14-20`) | **Weaker identity, better cross-unit matching** (see below) |
| reference: bound target, syntactic kind, substitutions | bound target yes; **one `References` relation for every kind** | **Weaker** |
| direct call vs possible-dispatch, never merged | direct only; `DispatchesTo` declared, never emitted | **Absent** |
| using directive with form + used flag | absent | **Absent** |
| generated-code origin tag | absent | **Absent** |
| diagnostic + unit standing | per-document diagnostic strings; `WorkspaceFailed` captured but not classified into reasons | **Weaker** |

So graphify is *not* a substitute for tethys-yjok's contract. It is a reference
implementation of the walk underneath it.

## Identity: where graphify fills a real hole

tethys-yjok keys declarations by `(evaluation unit, documentation-comment id)`.
The 2026-09-06 research already established the trap: doc-comment ids are
**not** assembly- or unit-qualified, so two units can legitimately declare the
same `T:Foo.Bar`. graphify solves the matching half of that problem with a
second key:

- **`CanonicalKey`** = `"csharp/v1" | <project key> | <reference key>`
  (`G/Domain/SymbolIdentity.cs:189-196`) → hashed to the node id
  (`G/Domain/NodeId.cs:18-19`, `"cs_" + lowercase hex`). Project-scoped, so it
  separates identical declarations in two units.
- **`ReferenceKey`** (`G/Domain/SymbolIdentity.cs:198-215`) is the same string
  *without* the project component, and `G/Roslyn/SymbolReferenceKey.cs:9`
  rebuilds it under a synthetic project identity
  (`new ProjectIdentity("__reference__.csproj", "__reference__")`). That is what
  lets a reference in project A match a declaration in project B without
  merging the two.

The reference key is a hand-rolled documentation-comment id: kind, namespace,
containing-type path with arity, containing-member path, name, arity, canonical
parameter list (type + `ref:`/`out:`/`in:`/`this:`/`params:` modifier), return
type, discriminator (`G/Domain/SymbolIdentity.cs:198-215`; construction in
`G/Roslyn/RoslynSymbolIdentityFactory.cs:13-34, 48-98, 286-306`; modifier tokens
at `G/Domain/SymbolIdentity.cs:74-88`). Doc-comment ids remain the better
*identity* — spec-defined, overload-disambiguating, cross-tool — but the two-key
split is the mechanism tethys's contract is missing, and it needs it regardless
of which id form wins.

Two more identity details worth copying:

- **Partial canonicalization** (`G/Roslyn/PartialSymbolHelper.cs`): a symbol
  collapses to its `PartialDefinitionPart` for identity, while the node's
  locations span **all** parts via `Parts()` — so `partial void M()` plus its
  implementation are one node with two locations, not two nodes. Tethys's
  tree-sitter path emits N symbols per partial part with the members' parent
  linkage nulled (`docs/2026-09-06-csharp-extractor-capability-matrix.md` §1);
  this is the correct shape.
- **Unique-match-only lookup** (`G/Roslyn/DeclarationCatalog.cs:94-141`): the
  catalog holds three indices — by `ISymbol` (`SymbolEqualityComparer`), by
  reference key, by source span — and both fallback lookups **decline unless
  exactly one match exists** (`matches.Length == 1` at `:115` and `:135`). That
  is tethys's unique-or-decline epistemics expressed in a declaration catalog,
  and it is the fix shape for the open `qualified_exact`-binds-first-match bug
  (**tethys-bvgb**). The catalog constructor *throws* on a canonical-key
  collision (`:39`), but the builder *skips with a diagnostic* first
  (`G/Roslyn/DeclarationCatalogBuilder.cs:98-102`) — two phases, so a collision
  never surfaces as a crash mid-walk.

## The walk: `IOperation` first, syntax second

Three mechanisms here are non-obvious and each is a trap tethys would hit.

### 1. Outermost-operation-root dedup

`G/Roslyn/SemanticSyntaxWalker.cs:22-64`: an operation is visited only if **no
ancestor syntax node has already been visited as an operation root**, tracked in
a `HashSet<SyntaxNode>` using `ReferenceEqualityComparer.Instance` (`:11-12`),
with `ShouldVisitOperationRoot` (`:40-51`) and `HasVisitedOperationRoot`
(`:53-64`) walking parents both ways. Without it, `GetOperation` at every node
yields the same invocation several times and edges multiply. Tethys has no
equivalent concern today (tree-sitter walks are positional), so this is new
information, not a re-derivation.

### 2. Two walkers, one edge set, one suppression rule

The syntax walker supplies what `IOperation` does not: `VisitAttribute`,
`VisitGenericName`, `VisitQualifiedName`, `VisitAliasQualifiedName`,
`VisitIdentifierName` (`:66-94`) each resolve a symbol and go through `VisitSymbol`
(`:96-120`), which calls `GetSymbolInfo` and emits a `References` edge — plus a
separate edge for a using alias via `GetAliasInfo` (`:115-119`). **This is how
type usage gets indexed at all** — a type named only in a parameter annotation,
a generic argument, or an attribute still produces an edge, and the test asserts
it (`tests/Graphify.CSharp.Tests/Roslyn/SemanticReferenceExtractorTests.cs:28`
against `:24-30`). Tethys's tree-sitter C# path emits **zero** `Type` references
(filed as **tethys-vb28**)
(capability matrix §1.4), so a type used only in annotations has no inbound
evidence. This is the cheap half of that fix.

The suppression rule is the one that keeps this honest: `IsInvocationTarget`
(`:122-135`) skips identifier names that *are* an invocation's callee
(`invocation.Expression == node`, and the `memberAccess.Name` / `memberBinding.Name`
variants), because the operation walker already emitted that call. Two walkers
writing into one edge set need exactly this kind of explicit ownership boundary.

### 3. Pattern members: take the shape list, reject the lookup

C# has no declaration to point at for `foreach`'s `GetEnumerator`, `await`'s
`GetAwaiter`/`IsCompleted`/`GetResult`, `using`'s `Dispose`, deconstruction's
`Deconstruct`, `fixed`'s `GetPinnableReference`, or `with`'s clone method —
these are members the compiler *selects* without the source naming a callee.
The **shape list is the valuable part**, and tethys-yjok's contract does not
mention the family at all. Missing them makes `GetEnumerator`/`Dispose`/
`GetAwaiter` implementations read as unreferenced in `dead-code` and absent
from `callers` — a real, maintainer-visible gap.

graphify's mechanism for finding them is mostly sound — and where it is sound,
that is the point worth recording, because *how* it gets each member differs per
shape. Three distinct sources are in play:

1. **Authoritative operation properties** — Roslyn hands these over directly and
   graphify uses them: the `with` clone method (`IWithOperation.CloneMethod`,
   `:159-164`), pattern deconstruction (`IRecursivePatternOperation.DeconstructSymbol`,
   `:165-170`), list/slice patterns (`IListPatternOperation.LengthSymbol`/`IndexerSymbol`,
   `ISlicePatternOperation.SliceSymbol`, `:171-183`), interpolated-string-handler
   construction (`IInterpolatedStringHandlerCreationOperation.HandlerCreation`,
   `:307-316`), event add/remove (`IEventAssignmentOperation.Adds` →
   `AddMethod`/`RemoveMethod`, `:237-251`), argument→formal-parameter
   (`IArgumentOperation.Parameter`, `:231-236`), and every operator method.
2. **Speculative binding** (`ResolveSpeculativeMethod`, `:708-717`, synthesizing
   `(receiver).Name(args)` + `GetSpeculativeSymbolInfo`) — used for `foreach`'s
   enumerator, `await`'s awaiter, deconstruction-assignment `Deconstruct`, and
   `fixed`'s `GetPinnableReference`. This is real binding, so it handles
   overloading and **extension methods** correctly.
3. **Name matching on the result** (`FindParameterlessMethod`,
   `FindReadableProperty`, `FindMembers`, `:718-810`) — used to walk the
   *continuation* of a protocol by name once the entry member is known.

Source 2 is better than I first credited and is not the defect. Source 3 is
where the error is, and it matters because it silently picks the wrong member
rather than declining.

**Wrong — `Dispose`.** `FindDisposalMethod` (`:734-763`) calls
`FindParameterlessMethod` *first* and only falls back to
`FindImplementationForInterfaceMember`. For

```csharp
sealed class TwoWay : IDisposable
{
    public void Dispose() => Console.WriteLine("public");
    void IDisposable.Dispose() => Console.WriteLine("explicit");
}
```

an ordinary `using` calls the **explicit interface implementation**, not the
public method. Verified two ways on .NET SDK 10.0.102 — a runtime probe prints
`using -> EXPLICIT IDisposable.Dispose`, and a Roslyn probe shows
`TwoWay.GetMembers("Dispose")` returns only `Dispose` (the explicit
implementation's `ISymbol.Name` is `System.IDisposable.Dispose`, so a
name lookup cannot see it) while
`FindImplementationForInterfaceMember` returns the explicit implementation.
graphify therefore emits a **false edge**. Confirmed by running graphify's own
walker on the fixture above (built from `66f3c03`; full decode below): it emits

```
calls  Gfix.Use.UsesTwoWay  ->  Gfix.TwoWay.Dispose            (public)
```

while the runtime calls `Gfix.TwoWay.System.IDisposable.Dispose`. Both nodes
exist in its catalog — it picked the wrong one. So its `callers` for the public
`Dispose` reports a call that never happens, and its `callers` for the explicit
implementation misses a call that does. (The one-way case where only the
explicit implementation exists happens to be saved by the fallback — which is
why the ordering bug is easy to miss.)

**Not a defect — extension `GetEnumerator`.** I initially recorded this as a
graphify miss and it is not one: `AddForEachPatternReferences` (`:546-584`)
obtains the enumerator through `ResolveSpeculativeMethod`, not `FindMembers`, and
speculative binding resolves extension methods. Confirmed by building graphify
from source at `66f3c03` (.NET SDK 10.0.102, `-p:TargetFrameworks=net10.0`) and
running its own walker on a fixture using
`static IEnumerator GetEnumerator(this Ext e)`; it emits

```
calls  Gfix.Use.UsesExt  ->  Gfix.ExtEnum.GetEnumerator(Gfix.Ext)
```

correctly. `FindMembers` name-matching applies only to the protocol
*continuation* (`MoveNext`, `Current`, `Dispose`) on the type the entry member
returns.

**For `foreach` and `await`, Roslyn states the answer outright.** These are
preferable to speculative binding for the *entry* member — same result on the
extension case, one call instead of a synthesized-and-rebound expression, and no
reliance on `FindMembers` for the continuation. Both were compiled and executed
against `Microsoft.CodeAnalysis.CSharp` 5.9.0:

| API | members it returns |
|---|---|
| `SemanticModel.GetForEachStatementInfo(CommonForEachStatementSyntax)` | `GetEnumeratorMethod`, `MoveNextMethod`, `CurrentProperty`, `DisposeMethod` |
| `SemanticModel.GetAwaitExpressionInfo(AwaitExpressionSyntax)` | `GetAwaiterMethod`, `IsCompletedProperty`, `GetResultMethod` |

Both handle extension and interface cases correctly because they report what the
compiler actually bound. For `using`, the rule to implement is the compiler's
own — a type convertible to `IDisposable`/`IAsyncDisposable` disposes **through
the interface**, so `FindImplementationForInterfaceMember` must be the *primary*
path, not the fallback; only a ref struct with no such conversion uses a pattern
`Dispose`. Do **not** reach for speculative binding where Roslyn already supplies
the member: the `with` clone is `IWithOperation.CloneMethod` (`:159-164`, which
graphify itself uses) and pattern deconstruction is
`IRecursivePatternOperation.DeconstructSymbol` (`:165-170`). Speculative binding
is genuinely needed for just two shapes — `fixed`'s `GetPinnableReference`
(`:638-649`) and deconstruction-*assignment*'s `Deconstruct` (`:526-545`), which
`IDeconstructionAssignmentOperation` does not expose.

**Consequence for tethys's priorities.** This is not a "~120-line correctness
win" to port. A name-matching reimplementation emits *false edges*, and for a
tool whose contract is precision — `--exclude-speculative`, kind-gated binding,
dead-code's inverted band posture — a false positive is strictly worse than a
miss: it makes `callers` lie and hides dead code. The pattern family belongs in
the contract with authoritative lookups and **negative fixtures** (two-way
`Dispose`, explicit-only `Dispose`, `DisposeAsync` preference under `await using`)
plus the extension `GetEnumerator` positive control. It is a correctness
requirement with real design surface, not a cheap add-on.

### 4. The compiler-selected operand catalogue

`G/Roslyn/SemanticOperationWalker.cs` is best read as a checklist of every
operation whose *target member* is compiler-chosen and therefore invisible to a
syntax-only extractor. Every row is one `OperationWalker` override:

| Operation override | Target it reveals | Line |
|---|---|---|
| `VisitInvocation` | `TargetMethod`, `ConstrainedToType` | :67-73 |
| `VisitObjectCreation` | `Constructor` | :74-79 |
| `VisitCollectionExpression` | `ConstructMethod` (collection builder) | :80-85 |
| `VisitCompoundAssignment` / `VisitIncrementOrDecrement` | `OperatorMethod` | :86-99 |
| `VisitBinaryOperator` / `VisitUnaryOperator` / `VisitConversion` | `OperatorMethod` (user-defined operators, conversions) | :100-120 |
| `VisitDeconstructionAssignment` | `Deconstruct` | :121-126 |
| `VisitForEachLoop` | enumerator protocol | :127-132 |
| `VisitAwait` | awaiter protocol | :133-138 |
| `VisitUsing` / `VisitUsingDeclaration` | `Dispose`/`DisposeAsync` | :139-158 |
| `VisitWith` | `CloneMethod` | :159-164 |
| `VisitRecursivePattern` | `DeconstructSymbol` | :165-170 |
| `VisitListPattern` / `VisitSlicePattern` | `LengthSymbol`, `IndexerSymbol`, `SliceSymbol` | :171-183 |
| `VisitPropertySubpattern` | pattern member reference | :184-193 |
| `VisitMethodReference` / `VisitFieldReference` / `VisitEventReference` | member (method group, field, event) | :194-214 |
| `VisitLocalReference` / `VisitParameterReference` / `VisitArgument` | local, parameter, **formal parameter of the call** | :215-236 |
| `VisitEventAssignment` | add/remove accessor | :237-251 |
| `VisitBranch` | label target | :252-257 |
| `VisitImplicitIndexerReference` | `IndexerSymbol`, `LengthSymbol` | :258-264 |
| `VisitTypeOf` / `VisitSizeOf` / `VisitIsType` | type operand | :265-282 |
| `VisitFunctionPointerInvocation` | function-pointer target | :283-298 |
| `VisitPropertyReference` | property + accessor methods by effect | :299-306 |
| `VisitInterpolatedStringHandlerCreation` / `Append` | handler ctor + `AppendFormatted` | :307-326 |
| `Visit` (base) | `IRangeOperation.Method`, `case` clause label, `fixed` pattern | :44-65 |

Tethys's missing shapes are almost exactly the complement: `?.`, indexers,
object initializers, `new()`, implicit-this reads, collection expressions,
patterns, `await`, `foreach`, `using`, operators, deconstruction. Every one of
them is a single operation override here — and the fixture proves it, including
`host?.Value = "updated";` and `values?[0] = 1;`
(`tests/Fixtures/CSharp14Fixture/ModernFeatures.cs:22-23`), which are literally
tethys-5uqz's open items. `VisitArgument` (`:231-236`) is the other one worth
calling out: it binds each argument to the **formal parameter symbol**, which is
tethys-yjok's "invocation and constructor arguments bound to source formal
parameters" requirement — one override, no signature parsing.

### 5. Accessor effect derivation

`AddPropertyAccessorEdges` (`:483-507`) answers "was this property read, written,
or both?" by climbing from the reference to its parent operation —
`IIncrementOrDecrementOperation`/`ICompoundAssignmentOperation` → ReadAndWrite,
`IAssignmentOperation` where the reference is the target → Write, else Read —
through `IConversionOperation`/`IParenthesizedOperation` wrappers
(`UnwrapAssignmentTarget`, `:508-518`). It then emits **calls to the accessor
methods** (`operation.Property.GetMethod` / `SetMethod`), not just a reference to
the property. That is what makes "who calls `get_X`" answerable, and it is the
concrete algorithm behind tethys-yjok's "read, write, init" reference kinds.

## Declaration catalogue: enumerate symbol interfaces, not syntax kinds

graphify never maintains a syntax→symbol-kind table. Every declaration comes
from `ISymbol` (`G/Roslyn/RoslynSymbolIdentityFactory.cs:13-34`'s switch), and
`declaration_kind` is derived from the *symbol* — including
`method.MethodKind.ToString().ToLowerInvariant()`
(`G/Roslyn/DeclarationCatalogBuilder.cs:375-404`), which yields `destructor`,
`conversion`, `userdefinedoperator`, `eventadd`, `eventremove`, `indexer`,
`record_struct`, `enum_member`, `local_function`, `local_constant`, and `alias`
for free. The test asserts all ten
(`tests/Graphify.CSharp.Tests/Roslyn/SemanticReferenceExtractorTests.cs:144-150`).

Contrast tethys's `node_kinds` string table and per-declaration arms
(`src/languages/csharp.rs:21-71, 796-937`), whose `_ => {}` at `:878` silently
drops destructors, operators, conversions, and indexers. **A symbol-driven
catalogue cannot silently drop a declaration family** — that is the structural
win, and it is independent of whether tethys ships graphify's walker.

The split between the two collection passes is also worth noting:

- `DeclarationCatalogBuilder.VisitNamespace`/`VisitType` (`:189-254`) walk the
  compilation's symbol tree for namespaces, types, and their members, filtered
  by **source membership** — only symbols whose location is in
  `project.Documents` (`IsSourceDeclaration`, `:406-420`).
- `SourceDeclarationCollector.cs` covers what the symbol tree does not expose
  conveniently: locals, range variables, parameters, type parameters, labels,
  aliases, tuple elements, designations, `case` labels, query clauses
  (`IsDeclarationCandidate`, `:204-227`, dispatching to `GetDeclaredSymbol` per
  syntax shape in `DeclaredSymbols`, `:229-401`). Its `FindRangeVariable`
  (`:403-442`) is a telling
  piece of engineering — it probes several positions plus every node in the
  enclosing query expression to work around `LookupSymbols`'s position
  sensitivity. Honest workaround, documented, not a guess.

Also note `G/Roslyn/RoslynDocumentKeyPolicy.cs:7-22`: work-queue keys are
canonical absolute paths, and a **duplicate document gets an occurrence suffix**
(`\u001f{duplicateIndex:D8}`) because, in the source's own words, "Roslyn can
expose one physical generated file more than once when an explicit broad include
overlaps SDK-generated inputs" (`:14-17`). Tethys already hit the tree-sitter
analogue (**tethys-hyoq**: duplicate `Compile` items violated a PK and rolled
back the whole revision). The Roslyn side has the same hazard.

## Robustness: the exception taxonomy and determinism

graphify's recoverable-failure posture is narrower and better-informed than
"catch everything", and it is the piece tethys's per-unit standing vocabulary
needs from the Roslyn side:

- `IsRecoverableSemanticException` names **`ArgumentException`,
  `NotSupportedException`, `NotImplementedException`**
  (`G/Roslyn/SemanticReferenceExtractor.cs:282-283`) — Roslyn's family for "you
  asked for something this symbol shape does not support" — caught **per
  document**, with the project and relative file path recorded in the diagnostic
  string (`:285-303`). The dispatcher is `ExtractBatchesAsync` (`:147-...`),
  which parallelizes per project with a bounded degree and keeps a per-batch
  diagnostic set. That taxonomy — which exceptions mean *unsupported shape* and
  which mean *infrastructure* — is exactly the mapping tethys-yjok needs for
  `WorkspaceFailed` → rvr5 reasons.
- `SourceSymbolKey.TryCreate` also catches `NotSupportedException` around
  `symbol.Locations` (`G/Roslyn/SourceSymbolKey.cs:21-24`), because some
  implementation symbols throw there. That is the same class of guard tethys's
  own extractor applies at every failure path.
- Identity failures do not abort: `IsRecoverableIdentityException`
  (`G/Roslyn/DeclarationCatalogBuilder.cs:364-365`) → an
  `Identity: skipped <kind> '<display>' at <file>:<line>: <message>` diagnostic
  (`:356-362`).
- An auxiliary probe failure never crashes a loaded project: input discovery
  catches everything, sets `isComplete = false`, and records a diagnostic
  (`G/Roslyn/RoslynProjectInputDiscovery.cs:125-133`), with the in-source
  rationale "an auxiliary dependency probe must not turn a usable project into a
  crash".
- Determinism is enforced by discipline, everywhere: ordinal ordering on
  projects, declarations, edges, and diagnostics; `GraphEdge` sorts its source
  locations by path/line/column and exposes `DeduplicationKey`
  (`G/Domain/GraphEdge.cs:64`); the accumulator merges rather than appends
  (`G/Roslyn/GraphEdgeAccumulator.cs:12-29`), with a single-location fast path
  that only upgrades to a `HashSet` on the second distinct location (`:89-121`).
  tethys-yjok's "deterministic output across 3 fresh runs" acceptance criterion
  has a concrete pattern to copy.

One honest divergence: the README claims "Unsupported semantic shapes are
reported as diagnostics instead of silently disappearing", but an unresolved
symbol produces **no edge and no diagnostic** (`AddSymbolEdge` returns silently
when `FindDeclaration` misses, `:426-458`). The diagnostics cover thrown
exceptions, not misses. A second divergence: the incremental docs describe
cold-start cache reuse, but the warm watcher session only ever calls
`SaveAsync` — reuse-from-manifest lives solely in the standalone refresh engine.
Both are reminders that a README is not evidence.

## Two fingerprinting ideas worth stealing

`G/Roslyn/IncrementalProjectFingerprintBuilder.cs:24-58` keys a project's
staleness on:

- the project file, every source document (`:28-35`), **the input-discovery
  dependency files** (imports, item-rule globs, analyzer/`AdditionalFiles`, and
  the `obj/project.assets.json` path even when absent —
  `G/Roslyn/RoslynProjectInputDiscovery.cs:77-82`), the project-reference key set
  with aliases (`:43-46, 74-86`), and an `IsComplete` flag (`:56`), plus
  a **compilation-options key** (`:97-167`): `PreprocessorSymbolNames`,
  `LanguageVersion`, `NullableContextOptions`, `AllowUnsafeBlocks`, implicit
  `Usings`, `SpecificDiagnosticOptions`, `CheckOverflow`, `Deterministic`,
  warning level, and the provider type names.

The second one is the transferable insight. `DefineConstants`, `LangVersion`,
`Nullable`, and `AllowUnsafeBlocks` change what a reference **binds to** without
changing any source byte. Tethys's reindex keys on file mtimes, and its
evaluation cache keys on project-file and import mtimes; a semantic sidecar's
freshness must additionally cover these properties. tethys-82a6 already
captures all four in its evaluated property list
(`tools/tethys-msbuild-evaluate/Evaluation.cs:17-25`) — this is the field
evidence for why they belong in the fingerprint and not merely in provenance.

The first is the same idea applied to *uncertainty*: `DependencyDiscoveryComplete`
is folded into the fingerprint's canonical key (`G/Incremental/ProjectFingerprint.cs:138`)
**and** forces a `Different` verdict from either side
(`:82-83`), so an incomplete discovery can never be mistaken for a clean cache.
That is tethys's own "a partial evaluation must not be reused as if whole"
posture, made mechanical, and worth stating as an explicit reindex invariant.

One cautionary note: `SourceFingerprint` carries a content-hash channel and a
three-valued comparison (`Different` > `MetadataMatch` > `ContentMatch`), but
**no production call site ever requests a hash** — every caller passes
`includeContentHash: false`, so mtime+length is the only content check. Tethys
has the same dormant-feature pattern today (`content_hash` is written as `None`);
the code-review-graph comparison flagged it as tethys's biggest lead. Two
independent tools building the same unused channel is evidence it needs a
consumer decision, not a third implementation.

## Where graphify diverges — do not copy these

1. **Span-bearing discriminators inside the persisted key.** File-local types,
   locals, parameters, type parameters, labels, and aliases get
   `source=<relpath>@<start>:<length>` in their **canonical** key
   (`G/Roslyn/RoslynSymbolIdentityFactory.cs:260-284`, applied via
   `CreateScoped` `:125-145`), so their node ids change whenever anything above
   them in the file is edited. That is fine for graphify: it emits a fresh
   snapshot every run and consumers re-read it. Tethys persists identity across
   runs and computes deltas (dead-code candidates, reindex invalidation, ref
   repair). **Synthesized document-local ids must not contain spans** — key them
   by enclosing doc-id plus a stable ordinal, and keep the span as a location
   only. tethys-yjok's synthesis rule needs this constraint spelled out.
2. **One `References` relation for everything.** Type usage, field/property
   reads and writes, `nameof`, method groups, base-list entries, label and
   range-variable use — all `GraphRelation.References`
   (`G/Domain/GraphEdge.cs:5-13`). tethys-yjok asks for syntactic kinds (`call`,
   `construct`, `read`, `write`, `init`, `add`, `remove`, `nameof`, `typeof`,
   `type usage`, `base list`, `attribute`). graphify's coarser relation is enough
   for "does anything reference this?" and not enough for read/write-specific
   analyses.
3. **No possible-dispatch edges.** `GraphRelation.DispatchesTo`
   (`G/Domain/GraphEdge.cs:12`) is declared and never emitted;
   `EvidenceKind.Inferred`/`Ambiguous` (`:18-19`) likewise; every edge is
   `Extracted` with `confidence: 1.0` at all three construction sites. The
   confidence fields are write-only — the same failure mode ADR-0003 was written
   to avoid, and the same one observed in code-review-graph. A call to an
   interface member is emitted as a plain `Calls` edge; nothing distinguishes
   "the compiler selected this" from "this may dispatch to any implementation".
   tethys-yjok's direct-vs-possible split is **not** satisfied by this design.
   (The one place `FindImplementationForInterfaceMember` is used is *implicit
   interface implementation* edges in `SemanticDeclarationRelationshipExtractor.cs`
   and disposal lookup — not dispatch.)
4. **Multi-targeting is refused, not fanned out.** `TargetFrameworkResolver`
   MSBuild-*evaluates* the project (`G/Roslyn/TargetFrameworkResolver.cs:33`) and
   throws `InvalidOperationException` when `TargetFrameworks` has more than one
   entry unless `--target-framework` is passed (`:36-41`). Since an evaluation
   unit is project × TFM, tethys must fan out per unit; graphify's
   one-TFM-per-run model would either fail the load or force one TFM onto every
   project in the solution. (graphify's own docs acknowledge the identity risk:
   "the symbol key cannot silently combine different compilations".)
5. **No classic .NET Framework support.** No `VSToolsPath`,
   `FrameworkPathOverride`, `CustomAfterMicrosoftCommonTargets`,
   `Microsoft.NETFramework.ReferenceAssemblies`, `packages.config`, or
   `EnableWindowsTargeting` anywhere in `src/` or `docs/`. graphify's
   compatibility doc states the requirement plainly: "MSBuild must be able to
   evaluate the supplied input on the host." On the legacy reference repository
   that is false for 11 of 72 projects without a blank `VSToolsPath`, and
   `packages.config` is unrestorable by the dotnet CLI at all (tethys-cmlc).
   Tethys's three-property Linux profile is strictly ahead here — adopt the
   *data-driven profile* framing, not graphify's "install the SDKs" posture.
6. **No generator policy.** `Project.GetCompilationAsync` will run source
   generators from the project's analyzer references, and generated documents
   are not in `Project.Documents` so the source-membership filter excludes them
   from the catalogue — but there is no analyzer-stripping switch, no generator
   grant, and no `GeneratedCodeAttribute`/`<auto-generated>` origin tag.
   tethys-yjok's `--no-analyzers` and **tethys-lev7**'s generated-origin tag
   both remain tethys work.
7. **Hard failure where tethys wants standing.** `DeclarationCatalog`'s
   constructor throws on a canonical identity collision (`:39`); the multi-TFM
   resolver throws; the loader throws when no C# project loads
   (`G/Roslyn/RoslynWorkspaceLoader.cs:75`) and rethrows after disposing the
   workspace (`:89-91`). Tethys's contract is fail-closed per *unit* with a
   recorded reason (rvr5), not a process exit.
8. **Name matching for protocol continuation, and for `Dispose`.** Two separate
   uses. (a) Walking a continuation like `MoveNext`/`Current`/`Dispose` on the
   type the entry member returned, by name and position/order (`FindMembers`,
   `:774-810`) — acceptable, since the continuation's receiver type is already
   known. (b) Choosing the disposal target (`FindDisposalMethod`, `:734-763`):
   it tries a parameterless `Dispose` *before* the interface implementation,
   which is wrong whenever a type declares both, because the compiler disposes
   **through the interface**. Verified against graphify's own output — see
   section 3. Make `FindImplementationForInterfaceMember` the primary path and
   use `GetForEachStatementInfo`/`GetAwaitExpressionInfo` for the two protocols
   that have dedicated APIs; do not port (b).

## Side-by-side

| | Tethys (current C# path) | graphify-csharp |
|---|---|---|
| **Extraction engine** | tree-sitter-c-sharp 0.23.1, hand-rolled node-kind dispatch | Roslyn `IOperation` + syntax walker on `MSBuildWorkspace` |
| **Declarations** | 12 kinds, 8 domain `SymbolKind`s; no enum members, indexers, operators, conversions, destructors, local functions, top-level statements, positional record properties | all `ISymbol` kinds incl. destructor, conversion, user-defined operator, event add/remove, enum member, indexer, record/record_struct, local function, local, range variable, label, alias, parameter, type parameter |
| **Reference kinds** | 4 (`call`, `construct`, `field_access`, `inherit`); **no `Type` kind at all** | 1 relation (`References`) + `Calls`; covers type usage, reads, writes, method groups, labels |
| **Receiver handling** | dropped (`this.`/`base.`/literal/invocation receivers → bare name); phantom binds + self-loops (tethys-qghe, tethys-53iv) | compiler-resolved by construction |
| **Overloads / generics** | no overload identity; three inconsistent generic spellings; `Foo<int>()` emits no ref (tethys-l38h) | full parameter-type identity; `OriginalDefinition` fallback for constructed generics |
| **Member-read shapes** | plain `member_access_expression` only (tethys-5uqz: `?.`, indexers, initializers, implicit-this missing) | every `IOperation` shape incl. `?.` writes and null-conditional indexer writes |
| **Pattern members** | none | `foreach`/`await`/`using`/deconstruction/`fixed`/`with` shapes covered; entry members via operation properties or speculative binding (extension `GetEnumerator` correct), but a two-way `Dispose` resolves to the public method |
| **Type usage** | none | syntax walker → `References` edges |
| **Preprocessor** | both `#if` branches indexed; member-level `#if` drops members | compilation's real `ParseOptions` — no branch ambiguity |
| **Usings** | `static`/`global`/scope lost; alias never consulted (tethys-alus, tethys-glus) | not modelled |
| **Hierarchy** | `inherit` only — base class and interfaces indistinguishable, unanchored name match | `Inherits` vs `Implements` split; `Overrides`; explicit + implicit interface implementation |
| **Identity** | `parent::name` file-local name; no namespace, no project, no overload signature | project-scoped canonical key + project-independent reference key + doc-id-shaped reference key |
| **Partial declarations** | N rows, no merge; same-file parts null every member's parent | one identity, all locations |
| **Resolution provenance** | 9 strategies, 3 bands in a view, consumed at query time (`--exclude-speculative`) | none — all edges `Extracted`, confidence 1.0 |
| **Project/TFM identity** | evaluation units via the 82a6 MSBuild adapter (implemented), not wired into C# extraction | `project=<relpath>\|tfm=<tfm>`, one TFM per run (refuses multi-target) |
| **Classic-on-Linux profile** | specified in tethys-cmlc and implemented in the evaluation adapter | none present; loading is delegated to the host MSBuild, so classic-on-Windows-with-VS is untested rather than known-broken |
| **Trust / grants / standing** | `DiscoveryGrants`, rvr5 failure reasons, per-unit standing | none |
| **Output** | SQLite + 16 CLI analyses | one JSON document for an agent/`jq` |
| **Incremental** | mtime staleness + evaluation cache | project/TFM metadata fingerprint incl. a compilation-options key (content hashing present but unused) |
| **Determinism** | not a stated contract for C# output today | ordinal ordering everywhere; tests assert it |

## Concrete suggestions for tethys

Ordered by value-to-effort. Items 1–4 belong in tethys-yjok's contract and
implementation; 5–7 are test-and-extractor work that stands on its own.

### 1. Add the pattern-member family to tethys-yjok — *with authoritative lookups*

Enumerate the shapes (`foreach`, `await`, `using`/`await using`, deconstruction,
`fixed`, `with`), add the record kind, and resolve each with the best available
source of truth rather than a name match:

1. `SemanticModel.GetForEachStatementInfo` and `GetAwaitExpressionInfo` for the
   enumerator and awaiter protocols — the compiler's own answer.
2. The interface-conversion rule for `using`/`await using`:
   `FindImplementationForInterfaceMember` as the **primary** path when the type
   converts to `IDisposable`/`IAsyncDisposable`, with a pattern `Dispose` only
   for a ref struct that has no such conversion.
3. Speculative binding for the two shapes with no dedicated API —
   `GetPinnableReference` and deconstruction-assignment `Deconstruct` — and
   *only* those. Everything else has a direct source: `IWithOperation.CloneMethod`,
   `IRecursivePatternOperation.DeconstructSymbol`, the list/slice pattern
   symbols, `IArgumentOperation.Parameter`, and every operator property.

**Positive and negative fixtures are both the acceptance gate.** Positive: the
extension `GetEnumerator` case must bind (graphify gets this right; it is a
regression risk if the entry member is looked up by name instead). Negative: a
two-way type with both a public `Dispose` and an explicit `IDisposable.Dispose`,
an explicit-only implementation, and `DisposeAsync` preference under
`await using`. Each negative is a case where a name-matching implementation
silently produces the *wrong* edge (section 3), and a wrong edge is worse than a
missing one for a precision-contract tool.

Scope note: this is a correctness requirement with real design surface, not a
small add-on. Do not size it from graphify's code — graphify's name-matching
stage is the counter-example, and its speculative stage is not the problem.

### 2. Specify the walk shape and the dedup rule in the contract

Statement to record: *an operation is visited once, at its outermost syntax
root; identifiers that are an invocation's callee are owned by the operation
walker and excluded from the syntax walker's reference emission.* Both rules
exist to prevent edge multiplication, and both are easy to get wrong
independently.

### 3. Add the reference-key half of identity, and forbid spans in persisted ids

Keep `(unit, doc_comment_id)` as the identity; add a project-independent
matching key for cross-unit resolution. State explicitly that synthesized
document-local ids for lambdas, local functions, locals, parameters, labels,
and file-local types **must not embed source offsets** — tethys persists
identity and computes deltas across runs, and graphify's span-bearing
discriminators are edit-unstable. (This is a correction to graphify's design,
not a borrow.)

### 4. Add accessor-effect derivation and a possible-dispatch record

The accessor algorithm is ~35 lines (`SemanticOperationWalker.cs:483-518`) and
is what makes `get_X`/`set_X` callers answerable. The dispatch record is not in
graphify at all: `FindImplementationsAsync`/`FindOverridesAsync` (or the
synchronous `FindImplementationForInterfaceMember` sweep over `AllInterfaces`)
must produce a **separate** edge set that never merges with direct calls.
tethys-yjok's acceptance criterion already demands a fixture proving neither
leaks into the other; graphify is a negative example of the requirement being
easy to lose.

### 5. Take the operation-override catalogue as the extractor's gap checklist

The 23-row table above is a shape-by-shape list of what a compiler-selected
target reveals. Used against the tree-sitter path, it is also the honest
enumeration of tethys's C# blind spots, and it cross-checks the 22 un-ticketed
gaps from the capability-matrix audit. Worth filing once, as a checklist, so the
no-host path and the sidecar path converge on the same shape vocabulary.

### 6. Two cheap borrows for the tree-sitter path if the sidecar is delayed

- **Emit `Type` references.** tethys has none, so a type used only in
  annotations has zero inbound evidence. graphify gets these from the syntax
  walker, not the operation walker — meaning the cheapest version of this fix
  is available to tree-sitter too (`typeof`/`is`/`as`/cast/`catch`/`where`/
  parameter/local/return/generic-argument nodes).
- **`OriginalDefinition`-style fallback for generic identity.** graphify's
  `FindDeclaration` (`:811-824`) tries the symbol, then the source-span key,
  then `symbol.OriginalDefinition` against both — the concrete mechanism for
  "a constructed generic target resolves to the source declaration". Tethys's
  three inconsistent generic spellings (`List`, `List<T>`, `Echo<int>`) and
  missing arity are the same problem — verified and filed as **tethys-l38h**, with
  **tethys-itez** covering the symbol-metadata half.

### 7. Fixture shape: one surface file, one assertion per edge

`tests/Fixtures/LanguageSurfaceFixture/Surface.cs` (347 lines) declares every
construct tethys cares about, and
`LanguageSurfaceFeatureTests.cs` asserts the resulting edges one at a time
(`AssertCalls`/`AssertReferences` helpers). `ReferenceFixture` (5 files, 276
lines) covers declaration kinds, scoped declarations, file-local same-name
pairs, and cross-project calls — including two file-local types with the same
name asserted to get **distinct** canonical keys
(`SemanticReferenceExtractorTests.cs:155-169`). Tethys's C# fences are 863 lines
across 5 integration files with no fixture for the shape families above
(`tests/csharp_*` cover usings, cross-dir deps, coupling, L2 file deps). The
surface-fixture pattern is a better match for an oracle corpus than per-bug
fixtures, and it pairs directly with **tethys-umjq**/**tethys-dsh1**.

## What graphify could learn from tethys

Recorded for symmetry, and because two of these are the reason graphify's
output needs a consumer policy layer.

- **Consumed provenance.** Strategy stamping with high/medium/speculative bands
  derived in one view, wired to `--exclude-speculative` and to dead-code's
  inverted posture. graphify's `confidence`/`EvidenceKind` are write-only — the
  exact failure ADR-0003 was written to prevent, reproduced independently.
- **Direct vs possible dispatch as a first-class distinction.** graphify has the
  enum member and no producer; tethys's `CallerMode` makes the choice explicit
  at the query seam and rejects contradictory CLI flag combinations.
- **Kind-gated binding.** A `construct` ref that must not bind a property; a
  `Call` that must not bind a `StructField`. graphify's `FindDeclaration` will
  happily target any symbol kind.
- **Standing vocabulary.** Indeterminate-with-a-reason beats a thrown
  `InvalidOperationException` when a workspace-wide query must still answer.
- **Referential integrity at write time.** graphify validates duplicate node
  ids and dangling edge endpoints before publishing
  (`G/Incremental/IncrementalOutputPublisher.cs:36-53`), because its string-keyed
  graph can dangle. Tethys's FK schema makes that impossible by construction —
  and contribution-at-a-time merging is exactly the design that would need the
  check if the FKs ever went away.
- **The classic-on-Linux profile as data.** `VSToolsPath=` +
  `Microsoft.NETFramework.ReferenceAssemblies` targets +
  `FrameworkPathOverride` is a recorded profile with provenance, not a host
  qualification exercise.

## Things to not borrow

- **Span-in-identity for anything tethys persists** (see divergence 1).
- **One relation for every reference kind**, and numeric/boolean confidence that
  nothing consumes (see divergences 2 and 3).
- **Refusing multi-targeted inputs** instead of fanning out per evaluation unit
  (divergence 4).
- **`hyperedges: []`, `input_tokens: 0`, `output_tokens: 0`, `weight: 1.0`** —
  foreign-consumer schema fields carried with no producer
  (`G/Graphify/GraphifyJsonSerializer.cs:24-26`). Tethys's `--json` contract
  should not grow empty scaffolding for another tool's shape.
- **A hand-rolled reference key as the primary identity.** It works, but
  documentation-comment ids are spec-defined, cross-tool, and stable by
  specification; graphify's string is a re-implementation of the same idea with
  less coverage and no round-trip. Take the two-key *structure*, not the string.
- **No SDK/targeting license review.** graphify ships a .NET tool with
  `Microsoft.CodeAnalysis.Workspaces.MSBuild`, `Microsoft.Build`, and
  `Newtonsoft.Json`; tethys's `deny.toml` allow-list and the managed
  companion's NuGet lockfile check (AGENTS.md) mean the same dependency set has
  to pass a policy graphify does not have.

## Bottom line

graphify-csharp is the strongest single piece of evidence for tethys's
semantic-route decision: the Roslyn walk that tethys-yjok specifies is ~5.2
kLOC of C# in a shipped, MIT-licensed tool, and several parts tethys had not yet
specified — the walk dedup rule, the pattern-member shape list, accessor-effect
derivation, the two-key identity split, the recoverable-exception taxonomy, the
compilation-options fingerprint — are readable and concrete.

The caution is that readable is not the same as correct. graphify's
pattern-member handling is the demonstration: it is right in three different
ways (operation properties, speculative binding, name-matched continuations) and
still wrong on `using` — against a two-way `Dispose` it emits a call the
compiler never makes while missing the one it does. Read it for the *shape
inventory* and for which source each shape needs; take the binding *rules* from
Roslyn, and check every borrowed mechanism against a fixture that would fail if
it were wrong.

It is also a clean illustration of where tethys's contract is genuinely harder:
graphify ships *facts* and pushes meaning to the consumer, so it can afford
write-only confidence, one relation kind, no dispatch distinction, no standing,
no grants, and no classic-on-Linux profile. Tethys ships queries, so it cannot.
The borrows are the walk and the shape vocabulary; the divergences are the
discipline — and every borrowed mechanism needs a negative fixture before it is
trusted.
