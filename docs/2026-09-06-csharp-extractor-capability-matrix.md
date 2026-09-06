# Tethys C# capability matrix — tree-sitter path, no LSP

Audited worktree: `/tmp/tethys-07eh-binding-model` @ `138c6a6` (`docs(csharp): capture approved binding model…`).
Binary: `target/release/tethys` (pre-built). Grammar: `tree-sitter-c-sharp 0.23.1` (`Cargo.lock:1345-1346`).
All `file:line` cites are relative to the worktree. All fixture rows come from `sqlite3` against `.rivets/index/tethys.db` after `tethys index --rebuild`.

Fixtures (under `/tmp/csaudit-fixtures/`, no `Cargo.toml` unless stated):
- `ws1/src/{Decls,FileScoped,Program,Refs,Usings,Preproc}.cs` — every declaration kind, every expression shape, every using form, preprocessor. 6 files / 80 symbols / 36 refs.
- `ws2/{Lib,NsOne,NsTwo,Part,Nest,Ali,App,Tests,Globals,Dot,Dot2}/*.cs` — resolution cases (a)–(j) plus global-using, alias, static-style and test probes. 26 files.
- `ws3/src/{MemberIf,Receiver,Generic,TrailingDot}.cs` + `bin/`, `obj/` — member-level `#if`, receiver phantoms, generic arity, trailing-dot `using static`, build-dir exclusion.
- `ws4/` — mixed repo: root `[package]` Cargo.toml + `src/lib.rs` + `Cs/A`, `Cs/B` C# files.

Pipeline recap (for reading the tables): extractor (`src/languages/csharp.rs`) → Pass 1 insert-time same-file binding (`src/db/files.rs:183-529`) → Pass 2 cross-file arms (`src/resolve.rs:328-435`) → `call_edges` from resolved refs with both ends (`src/db/call_edges.rs:46-83`) → `file_deps` via K-hybrid filter (`src/db/call_edges.rs:116-216`) → `arch_*` only when Cargo crates exist (`src/indexing.rs:1260-1310`).

---

## 1. SYMBOLS

`qualified_name` is always `parent_name::name` or bare `name` (`src/indexing.rs:708-711`); the namespace is NEVER part of it (`extract_type_declaration` is called with `parent_name = None` for namespace-level types, `csharp.rs:711,719,726,734,740`). `module_path` is `""` for every C# symbol (`src/indexing.rs:712-718`; `lib.rs:225` is Cargo-based). `parent_symbol_id` is linked at insert time against same-file container symbols only, NULL on collision (`src/db/files.rs:299-339`).

| Declaration | Extracted? | SymbolKind | name / qualified_name / parent | Evidence |
|---|---|---|---|---|
| `namespace A.B { }` block | yes | `Module` | name = `"A.B"` (dotted text), qn `A.B`, parent none, vis Public, no attrs | `csharp.rs:746-762, 976-1003`; ws1 `Audit.Decls` row |
| file-scoped `namespace A.B;` | yes | `Module` | same as block | `csharp.rs:746` (`FILE_SCOPED_NAMESPACE_DECLARATION`); ws1 `Audit.Scoped` |
| nested block `namespace Outer { namespace Inner {…} }` | yes, as TWO un-dotted symbols | `Module`,`Module` | `Outer` and `Inner`, no parent chain, no `Outer.Inner` key | `csharp.rs:746-762` (recursion passes `Some(ns_name)` but `extract_namespace` ignores it: 997); ws2 `Nest/N.cs` rows `Outer`(33), `Inner`(34); doc `module_resolver.rs:38-41` |
| `class` | yes | `Class` | bare name; nested → `Outer::Inner` (immediate parent only, not chain: `Inner::InnerMethod`) | `csharp.rs:710-717, 832-843`; ws1 `Shape::Inner`, `Inner::InnerMethod` |
| `struct` (incl. `readonly struct`) | yes | `Struct` | bare | `csharp.rs:718-724`; ws1 `Point`, `RoStruct` |
| `interface` | yes | `Interface` | bare; members extracted with parent | `csharp.rs:725-732`; ws1 `IShape`, `IShape::Area` |
| `enum` | yes | `Enum` | bare | `csharp.rs:733-737` |
| enum member | **no** | — | `Color.Red/Green/Blue` produce no rows | `csharp.rs:733-737` pushes only the enum; `enum_member_declaration` not in `node_kinds` (21-71); ws1 `Color` has 0 children |
| `record Person(string Name, int Age)` | yes, as class | `Class` | bare; positional parameters produce NO property symbols | `csharp.rs:738-745`; ws1 `Person` has no `Name`/`Age` rows |
| `record class` with body | yes | `Class` | members extracted normally | ws1 `Employee`, `Employee::Dept` property |
| `record struct Coord(int X,int Y)` | yes, **as Class** (not Struct) | `Class` | `RECORD_DECLARATION` always maps to Class regardless of `struct` keyword | `csharp.rs:738-745`; ws1 row 77 `Coord class` |
| `delegate` (namespace-level) | yes | `Delegate` | bare, no parent; signature = return type | `csharp.rs:777-784, 1213-1243`; ws1 `Transform delegate sig int` |
| `delegate` (class-level) | yes | `Delegate` | parent = enclosing type | `csharp.rs:929-932` |
| method (instance) | yes | `Method` | `Type::Name`; signature `"<ret> Name(params)"` | `csharp.rs:763-771, 816-824, 1006-1039, 1400-1419` |
| method (`static`) | yes | `Function` | same qn; only the `static` modifier changes kind | `csharp.rs:766, 819` (`has_modifier(...,"static")`); ws1 `Ext::Twice function`, `Target::SMethod function` |
| constructor | yes | `Method` (no Ctor kind) | name = type name → qn `Widget::Widget`; signature `Widget(int x)` | `csharp.rs:1042-1074` (comment at 1063); ws2 rows 57/58 |
| static constructor `static Shape()` | yes | `Method` | same as ctor; visibility Private (no modifier) | ws1 row 48 `Shape::Shape private sig Shape()` |
| destructor `~Shape()` | **no** | — | `destructor_declaration` not matched | `csharp.rs:815-879` (`_ => {}` at 878); ws1 line 26 has no row |
| operator `operator +` | **no** | — | `operator_declaration` not matched | same; ws1 line 27 no row |
| conversion operator `implicit operator int` | **no** | — | `conversion_operator_declaration` not matched | same; ws1 line 28 no row |
| property (auto / accessor / expression-bodied) | yes | `Property` | `Type::Name`; signature = property type | `csharp.rs:903-907, 1082-1114`; ws1 `Shape::Sides int` |
| indexer `this[int i]` | **no** | — | `indexer_declaration` not matched (no simple name) | `csharp.rs:878`; ws1 line 22 no row; tracker 5uqz(4) |
| event (field form) | yes, one per declarator | `Event` | `Type::Name`; signature = type | `csharp.rs:916-923, 1121-1173`; ws1 `Shape::Changed EventHandler` |
| event (accessor form `{ add{} remove{} }`) | yes | `Event` | same | `csharp.rs:924-928, 1177-1207`; ws1 `Shape::Renamed` |
| field (incl. `int a, b;`) | yes, one per declarator | `StructField` | `Type::Name`; signature = type | `csharp.rs:908-915, 1121-1173`; ws1 `_sides`, `_edges` |
| `const` / `static readonly` field | yes | `StructField` | same (no Const/Static kind) | `csharp.rs:1116-1120` doc; ws1 `Shape::MaxSides int`, `Shape::Kind string` |
| local function | **no** | — | method bodies are never walked for symbols; `local_function_statement` unhandled | `csharp.rs:816-824` (method pushed, body not recursed); ws1 line 39 `Local` no row; call to it stays unresolved (ref rows 35/36) |
| lambda | **no** (never a symbol) | — | refs inside a lambda attribute to the enclosing member's span | `csharp.rs:241-248, 303-308`; ws1 line 40 |
| top-level statements / top-level `static void Helper()` | **no symbols at all** | — | `global_statement` falls into generic recursion; local functions not extracted | `csharp.rs:785-791`; ws1 `src/Program.cs` has ZERO symbol rows; its refs have `in_symbol NULL` |
| `static void Main()` inside a class | yes | `Function` | treated as an entry point by dead-code | `db/dead_code.rs:139-145` |
| partial declarations | yes, **one symbol per part**, no merge | `Class` ×N | same-file parts → parent linkage of ALL members becomes NULL (ambiguous container); cross-file parts → two independent classes | `db/files.rs:324-336`; ws1 `Shape`(40) & `Shape`(49), members 41-59 `pid NULL`; ws2 `Split` rows 44/47 |
| generic type parameters (`class Box<T>`, `T Generic<T>(T v)`) | name only, arity dropped | — | `Box<T>` and `Box` both store name `Box`; signature `T Generic(T v)`; `generics: None` | `csharp.rs:948-949` (name field = identifier), `1460`; ws3 rows 8/11 both `Box`; ws1 row 56 |
| nested types | yes | Class/Struct/Interface/Enum | `Outer::Inner` (one level); nested-type members `Inner::Member` (grandparent lost) | `csharp.rs:832-877`; ws1 rows 60-65 |
| explicit interface implementation `int IComparable<Shape>.CompareTo(...)` / `void IThing.Act()` | yes, **as a bare-named private method** | `Method` | name `CompareTo` (interface qualifier dropped), vis Private, duplicates the public impl (`Thing::Act` twice) | `csharp.rs:1011-1012` (`name` field only; `explicit_interface_specifier` ignored), `1335`; ws1 row 58; ws3 rows 17 & 19 |

**Modifiers.** Persisted nowhere except: `static` → kind `Function` vs `Method` (`csharp.rs:766`); `async` → `FunctionSignature.is_async` (`csharp.rs:1452`) which is inside `signature_details`, a field explicitly marked `dead_code` / never written to the DB (`common.rs:17-24`; `files.rs:264-266` inserts only `signature`). `abstract`, `virtual`, `override`, `sealed`, `new`, `partial`, `readonly`, `unsafe` are not captured; `is_unsafe` is hard-coded `false` (`csharp.rs:1458`); the `signature` string is `"<returns> <name><params>"` only (`csharp.rs:1412-1416`) — ws1 rows 51-54 (`abstract Area`, `virtual Draw`, `override ToString`, `unsafe Danger`) all show plain `void X()`.

**Visibility mapping** (`csharp.rs:1306-1337`): `public`→Public; `protected internal`→Crate; `private protected`→Module; `internal`→Crate; `protected`→Module; anything else→**Private**. Consequence: interface members (implicitly public in C#) are stored `private` (ws1 rows 64,71,72 `IInner::Do`, `IShape::Area`, `IShape::Prop`; ws3 rows 14,15) and explicit interface impls are `private` — both feed dead-code false positives (§7).

**Attributes** (`csharp.rs:1264-1302`): captured on types (971), methods (1037), ctors (1072), properties (1112), fields/events (1130 → cloned per declarator), event accessors (1205), delegates (1241); never on namespaces (1001). Stored as written (`Obsolete`, `System.Obsolete`), args = raw text minus one paren pair (ws1 attrs table: `Attr Obsolete nameof(Target)`, `Shape Serializable`). `is_test` only via method attributes `Test|TestMethod|Fact|Theory|TestCase` (`csharp.rs:1363-1397`), suffix `Attribute` and namespace prefix stripped; class-level `[TestClass]`/`[TestFixture]` are not consulted (ws2 `T::RunIsFine is_test=1`, `T::PingIsFine is_test=1`).

---

## 2. REFERENCES

The C# extractor emits exactly four kinds: `Call`, `Constructor`, `FieldAccess`, `Inherit` (grep of `csharp.rs`: 5/6/7/2 uses). It never emits `Type`, `Value`, `Method`, `Macro*`, `Reexport`. Because no `Method` kind is emitted, the tethys-53iv receiver gating at `files.rs:452-457` never applies to C#: Pass 1 binds C# calls by **bare name first**, then by the qualified name (`files.rs:463-468`), ignoring the receiver entirely.

Path/name storage: `parse_member_access` (`csharp.rs:496-535`) collects only `member_access_expression` and `identifier` nodes; any other receiver node (`this`, `base`, literals, invocation results, `generic_name`, array creation) is silently dropped (`533`). The name node's text is pushed verbatim, so a `generic_name` callee keeps `<T>`. `build_qualified_name` joins path + name with `::` (`files.rs:444`), which becomes `refs.reference_name` when unbound.

| Shape | Ref emitted? | Kind (extractor → db) | Stored name / path (`reference_name` if unbound) | `in_symbol` | Evidence |
|---|---|---|---|---|---|
| `Foo()` | yes | Call→`call` | `Foo`, no path | enclosing method/ctor/property/event span | `csharp.rs:373-391`; ws1 Refs.cs:29 |
| `obj.Foo()` | yes | Call | name `Foo`, path `[obj]` → `obj::Foo` | yes | `csharp.rs:392-403`; ws2 `App/A.cs:3 u::Ping`, `App/E.cs:3 x::Save` |
| `this.Foo()` | yes | Call | `this` dropped → bare `Foo` (indistinguishable from `Foo()`) | yes | `csharp.rs:533`; ws1 Refs.cs:28 bound `Hello` same_file |
| `base.Foo()` | yes | Call | `base` dropped → bare `Foo` → same-file last-wins binds to the **override itself** (self-loop) | yes | ws1 Refs.cs:27 bound `Derived::Hello`; call_edges `Hello→Hello ×3` |
| `Ns.Type.Static()` | yes | Call | `Static`, path `[Ns,Type]` → `Ns::Type::Static` (never resolves, see §5) | yes | ws2 `App/D.cs:3 Lib::Helper::Run` unresolved; ws1 `System::Threading::Tasks::Task::Yield` |
| `Type.Static()` | yes | Call | `Type::Static` → matches `symbols.qualified_name` | yes | ws2 `App/D.cs:3 Run qualified_exact` |
| generic invocation `Foo<int>()` | **no** | — | callee is `generic_name` → `_ => None` | — | `csharp.rs:404`; ws1 Refs.cs:48 no row; ws3 Generic.cs:10 no row |
| `obj.Foo<int>()` | yes | Call | **keeps generics**: `obj::Echo<int>` | yes | `csharp.rs:522-526`; ws3 Generic.cs:9 `obj::Echo<int>` |
| `new T(...)` | yes | Constructor→`construct` | `T` | yes | `csharp.rs:416-433`; ws1 Refs.cs:30 |
| `new T { X = 1 }` initializer | construct only | Constructor | initializer members emit **nothing** | yes | ws1 Refs.cs:31 single `construct Target`; tracker 5uqz(2) |
| `new()` target-typed | **no** | — | `implicit_object_creation_expression` unhandled | — | not in `node_kinds`; ws1 Refs.cs:32 no row |
| `new List<string>()` | yes | Constructor | strips to `List` | yes | `csharp.rs:445-457`; ws1 test 2465 |
| `new A.B.List<T>()` qualified generic | yes | Constructor | **keeps generics**: `System::Collections::Generic::List<Target>`; `M::Box<int>` | yes | `csharp.rs:434-443` (`name` field text incl. type args); ws1 Refs.cs:49; ws3 Generic.cs:12 |
| `T.Member` static read | yes | FieldAccess→`field_access` | `Member`, path `[T]` → `T::Member` | yes | `csharp.rs:336-354`; ws1 Refs.cs:34 `SProp` |
| `obj.Prop` read | yes | FieldAccess | `Prop`, path `[obj]` | yes | ws1 Refs.cs:35 |
| `obj.Prop = x` write | yes | FieldAccess | same as read (LHS emitted) | yes | test `csharp.rs:2051`; ws1 Refs.cs:36 |
| `obj.Field` | yes | FieldAccess | same | yes | ws1 Refs.cs:37 |
| implicit-this bare `Prop` | **no** | — | bare identifiers are never refs | — | ws1 Refs.cs:38 (`Own`) no row; tracker 5uqz(1) |
| `obj?.Prop` | **no** | — | `conditional_access_expression`/`member_binding_expression` unhandled | — | ws1 Refs.cs:39 no row; 5uqz(3) |
| indexer `a[i]` | **no** | — | `element_access_expression` unhandled | — | ws1 Refs.cs:40 no row |
| `nameof(X)` | yes, **bogus** | Call | `nameof` (unresolved noise); `X` emits nothing | yes | ws1 Refs.cs:41 & :64 `call nameof` |
| `typeof(T)` / `is T` / `as T` / `(T)x` / `default(T)` | **no** | — | no Type refs exist in C# | — | ws1 Refs.cs:42-45,54 no rows |
| type annotations (params, locals, returns, generic args) | **no** | — | — | — | `ExtractedReferenceKind::Type` never emitted; ws1 Refs.cs:65 no rows |
| base list `: Base, IFace<T>` | yes, one per entry | Inherit→`inherit` | bare name; generic stripped (`IComparable`); qualified keeps last segment; unresolved rows retained | anchored to the type's span | `csharp.rs:164-198`; ws1 rows `IDisposable`,`IComparable` (Shape), `Base` (Derived, same_file) |
| attribute usage `[Attr(args)]` | no ref for `Attr`; args are walked | — | e.g. `nameof` inside the attribute emits a Call attributed to the decorated method | yes | ws1 Refs.cs:64 |
| `using static` member call `Sqrt(4)` | yes | Call | bare `Sqrt` | yes | ws1 Refs.cs:55 unresolved (external); ws3 TrailingDot.cs:5 `Save import_union` |
| extension method `x.Twice()` / `3.Twice()` | yes | Call | identifier receiver → `x::Twice` (unresolvable); literal receiver dropped → `Twice` | yes | ws1 Refs.cs:56 `Twice unique_workspace` |
| delegate invocation `Act()` | yes | Call | `Act`; declines to bind the field (kind gate) | yes | `resolve.rs:52-54`; ws1 Refs.cs:52 unresolved |
| `Act.Invoke()` | yes | Call | `Act::Invoke` | yes | ws1 Refs.cs:53 |
| event `+=` / `-=` | FieldAccess on the event; handler method-group emits nothing | FieldAccess | `Ev` path `[t]` | yes | ws1 Refs.cs:50-51; `Handler` has no ref → dead-code Maybe |
| lambda | not a symbol; body refs attribute to enclosing member | — | — | enclosing | `csharp.rs:241-248` |
| LINQ query syntax | **no** | — | — | — | ws1 Refs.cs:57 no row |
| LINQ method syntax `.Select(x=>…)` | yes | Call | `Select` (receiver dropped when non-identifier) | yes | ws1 Refs.cs:58 |
| `await X.Y()` | yes (the call) | Call | `System::Threading::Tasks::Task::Delay` | yes | ws1 Refs.cs:63 |
| `catch (T e)` | **no** | — | — | — | ws1 Refs.cs:59 no row |
| `where T : U` | **no** | — | — | — | ws1 Refs.cs:61 no row |
| overloads `Get(1)` / `Get("s")` | yes | Call | both `Get` — no arity/type identity | yes | ws1 Refs.cs:46-47 both → row 21 (last overload) |

`in_symbol` (`containing_symbol_span`) is set for refs inside `method_declaration`, `constructor_declaration`, `property_declaration`, `event_declaration` bodies (`csharp.rs:241-248`, resolved via `span_to_id` at `files.rs:479-481`). It is NULL for field/property initializers (by design, `csharp.rs:239-240`), for top-level statements (ws1 Program.cs rows 27-30), and for refs inside members wrapped in a member-level `#if` (ws3 MemberIf.cs rows at lines 7/9 — the member symbol was never inserted so the span lookup misses).

---

## 3. IMPORTS

Storage path: `UsingDirective::to_import_statement` (`csharp.rs:127-138`) produces `ImportStatement { path, imported_names: [], is_glob: false, alias, is_reexport: false }` — `is_static` is **dropped** (`ImportStatement` has no such field, `common.rs:203-221`). At insert, `imported_names.is_empty()` forces `symbol_name = "*"` (`files.rs:507-513`), `source_module = path.join(".")` (`module_resolver.rs:344-346, 163-165`). `extract_using_directives_recursive` walks the entire tree (`csharp.rs:590-607`), so scope is lost.

| Directive | Stored row (`symbol_name` \| `source_module` \| `alias`) | What is lost | Evidence |
|---|---|---|---|
| `using System.Collections.Generic;` | `* \| System.Collections.Generic \| NULL` | — | ws1 imports table |
| `using static System.Math;` | `* \| System.Math \| NULL` | **static flag** — indistinguishable from a namespace using; re-derived at resolve time by splitting on the LAST dot and checking the prefix is a known namespace (`module_resolver.rs:390-406`) | `csharp.rs:616, 660-666` (identifier segments skipped when static, qualified kept) |
| `using static M.Repo.;` (trailing dot) | `* \| M.Repo. \| NULL` | trailing dot **kept** (missing identifier → empty segment) — answers nvcy question (1) | ws3 imports `M.Repo.` |
| `using Alias = Ns.Type;` | `* \| Ns.Type \| Alias` | alias stored but **never consulted**: `build_import_maps` routes every `"*"` row to `glob_imports` and only reads `alias` for non-`*` rows (`resolve.rs:298-303`); the target then behaves like `using Ns.Type;` | ws1 `Alias3`; ws2 `App/I.cs` `RT` unresolved |
| `using Alias = Ns.Sub;` (dotted namespace alias) | `* \| Ns.Sub \| Alias` | behaves as a plain glob of `Ns.Sub` → **over-resolves** bare names that C# would reject | ws2 `App/L2.cs` `new Dotted()` → `import_union` to `Dot/D.cs` |
| `using Alias = Ns;` (single-identifier target) | `* \| "" \| Alias` | **target lost entirely** (identifier segments are only pushed when `alias.is_none()`, `csharp.rs:660-666`; empty namespace + alias still returned, 675-677) | ws2 `App/L.cs` row `* \| (empty) \| X` |
| `using Alias = List<int>;` (generic target) | `* \| System.Collections.Generic.List<int> \| Alias1` | generic args kept in `source_module` | ws1 `Alias1` |
| `global using X;` | `* \| X \| NULL` in the declaring file only | `global` keyword dropped; **no propagation** | ws2 `Globals/Globals.cs` `* \| One`; `App/G.cs` `new Collide()` unresolved |
| `global using static X;` | `* \| X \| NULL` | both flags dropped | ws1 `System.Math` (from Usings.cs) |
| `extern alias Foo;` | **not stored** | `extern_alias_directive` is not a `using_directive` | ws1 imports has no `Foo` |
| `using` inside a namespace block | stored identically to file-level | scope flattened to the whole file | ws1 `System.IO`, `Audit.Decls\|Inner` from inside `namespace Audit.Usings` |

Consequence for resolution: `explicit_imports` is **always empty** for C# files (every row is `"*"`), so `ResolutionStrategy::ExplicitImport` (the only `high` band besides `lsp`, `schema.rs:91`) can never be assigned to a C# ref.

---

## 4. PREPROCESSOR

Grammar: `preproc_if`/`preproc_else`/`preproc_elif` wrap `declaration`/`type_declaration`/`statement` children; `preproc_region`/`preproc_endregion`/`preproc_pragma`/`preproc_define` are leaf-ish nodes (node-types.json of 0.23.1). The extractor has no arm for any `preproc_*` node; nothing evaluates conditions.

| Construct | Symbols | Refs | Evidence |
|---|---|---|---|
| `#if A … #else … #endif` at namespace/compilation-unit level | **both branches extracted** (falls into generic recursion `csharp.rs:785-791`) | both branches' refs extracted and bound | ws1 Preproc.cs: `IfBranch`(9), `ElseBranch`(11), `DebugOnly`(14) all present; call_edges `CallIf→OnlyIf`, `CallElse→OnlyElse` |
| `#if` wrapping **class members** | **members in either branch dropped** (`extract_class_members` matches only known member kinds, `_ => {}` at `csharp.rs:878`) | refs inside them still emitted (class-arm `_` recursion `csharp.rs:281-289` → `METHOD_DECLARATION` arm 241) but `in_symbol` NULL; calls to the dropped members stay unresolved | ws3 MemberIf.cs: only `Host::Always` extracted; refs at lines 7/9 bound `same_file` with `in_sym` empty; `Guarded` call unresolved |
| `#region` / `#endregion` | transparent | transparent | ws1 `Always` extracted inside a region |
| `#pragma`, `#define` | ignored | ignored | ws1 |

Both-branches indexing means mutually exclusive definitions (`#if NET48 class X {} #else class X {}`) become same-file duplicates, feeding the last-wins map and the ambiguous-parent NULLing.

---

## 5. RESOLUTION FOR C#

### 5a. Which arms can fire (order as in `resolve.rs:328-435`)

| Order | Strategy | Can fire for C#? | Condition / why not | Code |
|---|---|---|---|---|
| 0 (Pass 1) | `same_file` | yes | bare-name match in `name_to_id` (last-wins, kind-blind except data members & macros), then qualified; `FieldAccess` tries data members first then general map; `Inherit` via container map | `files.rs:341-401, 444-478` |
| 1 | `explicit_import` | **never** | `explicit_imports` empty (all C# rows are `*`) | `files.rs:507`, `resolve.rs:293-318, 337-348` |
| 2 | `glob_import` | **never** | C# policy is `UniqueAcrossAll`, not `FirstMatch` | `module_resolver.rs:374-388`; `resolve.rs:354-371` |
| 2' | `import_union` | yes, simple names only | union of (types named N in files of every plain-using namespace, kinds Class/Struct/Interface/Enum) ∪ (methods `Type::N` for every `using static Ns.Type` whose `Ns` is a known namespace); exactly one distinct symbol → bind, else decline | `resolve.rs:236-290, 375-385`; `symbols.rs:390-448` |
| 3 | `qualified_exact` | yes, for `A::B` names | `SELECT … WHERE qualified_name = ?` via `query_row` — **first row wins, no ambiguity check** | `resolve.rs:596-601`; `symbols.rs:111-122` |
| 4 | `same_crate` | practically no | requires `get_crate_for_file` to find a Cargo crate containing the `.cs` file; with a ROOT package the prefix is `""` and the lookup refuses | `resolve.rs:603-638`; `symbols.rs:262-264`; ws4 `Ping` resolved `unique_workspace`, not `same_crate` |
| 5 | `unique_workspace` | yes, simple names | exactly one symbol of that name in the whole index (any kind), then kind gate | `resolve.rs:641-657`; `symbols.rs:301-324` |
| 6 | `qualified_module_fallback` | **never** | `CSharpModuleResolver::qualified_splits` returns `[]` | `module_resolver.rs:408-410`; `resolve.rs:462-493` |
| 7 (Pass 3) | `lsp` | out of scope (a `CSharpLsProvider` exists, `src/lsp/mod.rs:43`) | — | — |

Kind gate on every Pass-2 arm: `ref_binds_to_symbol_kind` (`resolve.rs:46-60`) — Call/Construct never bind Property/Event/StructField; Inherit only binds containers; everything else unconstrained (module vs method, class vs ctor, etc.). Per-file memo by full `reference_name` (`resolve.rs:181-212`) means all same-named overload calls share one outcome.

Confidence bands (`schema.rs:89-95`): C# refs can only ever be `medium` (`same_file`, `import_union`, `qualified_exact`) or `speculative` (`unique_workspace`); never `high`.

### 5b. Concrete cases (ws2 rows: `path, line, kind, name, strategy, bound symbol, bound file`)

| Case | Fixture | Actual row(s) | Verdict |
|---|---|---|---|
| (a) workspace-unique simple name, no using | `App/A.cs` `new UniqueThing(); u.Ping();` | `A.cs 3 construct UniqueThing unique_workspace → UniqueThing class Lib/Unique.cs`; `A.cs 3 call u::Ping — unresolved` | type binds by uniqueness alone (speculative band); the instance call never resolves |
| (b) name colliding across 2 namespaces, one `using` | `App/B.cs` `using One; new Collide(); c.Save();` | `B.cs 4 construct Collide import_union → Collide class NsOne/Collide.cs`; `B.cs 4 call c::Save — unresolved` | using-arm disambiguates the TYPE; member call unresolved |
| (c) both `using`s | `App/C.cs` `using One; using Two; new Collide();` | `C.cs 5 construct Collide — unresolved (reference_name Collide)` | union has 2 candidates → decline |
| (d) qualified `Ns.Type.Method` | `App/D.cs` (no using) & `App/D2.cs` (`using Lib`) `Lib.Helper.Run(); Helper.Run();` | `D.cs 3 call Lib::Helper::Run — unresolved`; `D.cs 3 call Run qualified_exact → Helper::Run Lib/Helper.cs` (same in D2.cs) | namespace-qualified never resolves (no C# splits); `Type::Member` resolves regardless of usings |
| (d') cross-namespace `Type.Member` phantom | `App/K.cs` (no using) `Collide.Save();` with `Collide::Save` in NsOne AND NsTwo | `K.cs 3 call Save qualified_exact sid=57 → Collide::Save NsTwo/Collide.cs` (rows 57 & 60 both exist) | **phantom**: `qualified_exact` picks the first row, no decline |
| (e) `x.Save()` unknown receiver, two `Save` | `App/E.cs` | `E.cs 3 call x::Save — unresolved` | never resolves — the receiver is folded into the name; collision is irrelevant (see (a): even a unique method stays unresolved) |
| (e') same-file receiver-blind phantom | ws3 `Receiver.cs` `unrelated.Save(); self.Save(); this.Save(); Other.Save(); Repo.Save();` | all five rows `call Save same_file → Repo::Save` | Pass 1 ignores receivers entirely |
| (f) overloads `Get(int)` / `Get(string)` | `App/F.cs` `Helper.Get(1); Helper.Get("s");` and same-file `Get(1)+Get("x")` in `Lib/Helper.cs` | cross-file: both `qualified_exact sid=63 → Get(int)` (first row); same-file: both `same_file sid=64 → Get(string)` (last-wins) | no overload identity; cross-file picks FIRST overload, same-file picks LAST |
| (g) partial class across two files | `Part/P1.cs` `PartOne(){ PartTwo(); }`, `Part/P2.cs` `PartTwo(){ PartOne(); }` | `P1.cs 3 call PartTwo unique_workspace → Split::PartTwo P2.cs`; `P2.cs 3 call PartOne unique_workspace → Split::PartOne P1.cs`; two `Split` class rows (44, 47) | works only by workspace uniqueness; (first run with methods named `One`/`Two` colliding with namespaces `One`/`Two` stayed unresolved — kind-blind decline) |
| (h) type in nested namespace block | `App/H.cs` `using Outer.Inner; new Deep();` | `H.cs 4 construct Deep unique_workspace → Deep Nest/N.cs`; `file_deps` has NO `H.cs→N.cs` row | using is inert (map keys are `Outer`, `Inner`); resolves by uniqueness; K-hybrid drops the file dep |
| (i) `using Alias = …` | `App/I.cs` `using RT = Ali.RealType; new RT();` | `I.cs 4 construct RT — unresolved`; imports `* \| Ali.RealType \| RT` | alias never consulted |
| (j) `new Widget()` | `App/J.cs` (`using Lib`), ctors `Widget()`, `Widget(int)` in `Lib/Widget.cs` + same-file `new Widget()` in `Make()` | `J.cs 4 construct Widget import_union sid=56 → Widget CLASS` (both `new Widget()` and `new Widget(3)`); `Widget.cs 7 construct Widget same_file sid=58 → Widget::Widget METHOD (ctor `Widget(int x)`)` | cross-file binds the class (types-only kinds); same-file binds the LAST ctor overload. `callers Widget::Widget` → none; `callers Widget` → `J::M` |
| global using | `Globals/Globals.cs` `global using One;` + `App/G.cs` `new Collide();` | `G.cs 3 construct Collide — unresolved` | no propagation |
| static using member | ws3 `using static M.Repo; Save();` | `TrailingDot.cs 5 call Save import_union → Repo::Save` | static-member arm works when the type's namespace is in-workspace |
| mixed repo, same-namespace sibling | ws4 `Cs/B/Three.cs` `new Thing()` with `Thing` in NsA and NsB, no using | unresolved | implicit same-namespace visibility is not modeled |

---

## 6. PROJECT / CRATE ATTRIBUTION

- Crate discovery is Cargo-only: `cargo::discover_crates` returns `[]` when there is no `Cargo.toml` (`cargo.rs:26-39`). No `.csproj`/`.sln` reading exists anywhere; the only mention is a comment acknowledging "no `.csproj` discovery" (`db/call_edges.rs:232`).
- `file_crate_map` (`indexing.rs:593-618`): a file outside every crate gets pseudo-crate `orphan:<first path component>` (`orphan:App`, `orphan:Lib`; a root-level file gets `orphan:<filename>`; `ORPHAN_PSEUDO_CRATE_PREFIX` at `db/call_edges.rs:21`).
- Mixed repo trap (ws4, and `tests/csharp_cross_dir_deps.rs:1-13` header): with a root `[package]`, every `.cs` file under the root is attributed to that Rust crate (`CrateIndex` ancestor walk, `indexing.rs:1295-1300`); `arch_file_packages` lists `Cs/A/One.cs → mixed (manifest)`; `coupling` then shows one package `mixed` with C# files inside it.
- Effects:
  - **K-hybrid `file_deps`** (`db/call_edges.rs:116-216`): intra-bucket edges always kept; cross-bucket C# edges survive only via the namespace arm — caller's `*` usings ∩ callee file's `Module` symbol names (`244-287`, `174-181`). Namespace-qualified refs that resolved (`Helper::Run` from `App/D.cs`) still lose their file dep without a `using` (ws2: `D2.cs→Helper.cs` present, `D.cs→Helper.cs` absent); nested-block namespaces never corroborate (ws2 `H.cs`); refs with `in_symbol NULL` (top-level statements, member-level `#if`) never produce call edges, hence never file deps (ws1 `Program.cs→Decls.cs` absent although `Point` resolved).
  - **callers / reachable**: `call_edges` are bucket-agnostic — cross-bucket edges show (ws2 `callers Helper::Run` lists `D::M`, `D2::M`, `T::RunIsFine`).
  - **impact / cycles / affected-tests**: all read `file_deps` (`lib.rs:425-433`; `db/graph.rs:421-426, 490`; `lib.rs:790-830`), so they inherit the K-hybrid drops (ws2 `impact Lib/Helper.cs` lists `D2.cs`, `F.cs`, `T.cs` — not `D.cs`; `cycles` found `P1.cs→P2.cs→P1.cs` because both are in `orphan:Part`).
  - **coupling / arch phase**: `run_architecture_phase` returns zero stats when `crates` is empty (`indexing.rs:1269-1271`) and skips files outside any crate (`1299-1306`); CLI prints "No packages discovered … requires a Cargo workspace" (`cli/coupling.rs:262-266`). ws1/ws2: `arch_packages` empty.
  - **visibility-tightening**: keyed on `arch_packages` AND gated `f.language = 'rust'` (`db/visibility.rs:143-153`) — C# never participates even in the mixed repo.
  - **same_crate arm**: never fires for orphans (`resolve.rs:627-638` debug log); with a root package the prefix is `""` → refused (`symbols.rs:262-264`).
- Build output: `bin/` (unless under `src/`) and `obj/` are excluded from discovery (`indexing.rs:1249-1255`; ws3 `files` table has only the three `src/` files).

---

## 7. ANALYSES — the 16 CLI commands (`src/main.rs:276-360`)

| Command | C# status | Why (gating code) | Fixture result |
|---|---|---|---|
| `index` | works | language dispatch `types.rs:138-144`; C# extractor | ws1 "Indexed 6 files, 80 symbols, 36 references" |
| `search` | works | `LIKE` over `name`/`qualified_name`, language-neutral (`db/symbols.rs:73-90`) | ws1 `search Shape` → 20 rows incl. duplicates from partial/ctors |
| `stats` | works | per-language file counts (`cli/stats.rs:55-75`) | ws1 breakdown lists Properties/Events/Delegates |
| `callers` | works, degraded | `call_edges` (`db/graph.rs:43-60`); only refs with `in_symbol` AND a bound target; instance calls (`x.Foo()`) never bind cross-file; ctor symbols get no callers (construct→class) | ws2 `callers Helper::Run` 3 callers; `callers Widget::Widget` none |
| `impact` | works, degraded | `file_deps` transitive dependents (`lib.rs:425-433`); K-hybrid corroboration drops uncorroborated cross-dir edges (§6) | ws2 `impact Lib/Helper.cs` → D2, F, T (D.cs missing) |
| `coupling` | **Rust-only** | needs Cargo packages (`indexing.rs:1269`; `cli/coupling.rs:262-266`); C# files only appear inside a Rust crate's package in mixed repos | ws2 "No packages discovered"; ws4 shows package `mixed` |
| `cycles` | works, degraded | `file_deps` snapshot (`db/graph.rs:421-426, 490, 571`) — same K-hybrid caveat | ws2 found `P1→P2→P1` |
| `reachable` | works, degraded | `call_edges` BFS (`lib.rs:536-560`; `db/graph.rs:269`) — same call-edge caveats | ws1 `reachable Derived::Hello` → Twice, Target, Get, SMethod |
| `affected-tests` | works, degraded | reverse `file_deps` closure from changed files + `is_test` symbols in affected files (`lib.rs:790-830`); file-granular | ws2 `affected-tests Lib/Unique.cs` lists both tests (one doesn't touch Unique.cs) |
| `panic-points` | **Rust-only in practice** | predicate hard-codes `unwrap`/`expect` names (`db/panic_points.rs:25-28`) | ws1/ws2: 0 |
| `deprecated-callers` | works | `[Obsolete]` spellings (`db/deprecated.rs:119-120`), C# args parser (344+), same-language guard (258-261); ctor gap 9181 | ws1 lists `method Attr` as Clean |
| `visibility-tightening` | **Rust-only** | `AND f.language = 'rust'` (`db/visibility.rs:149`) + `arch_packages` join; doc `cli/visibility_tightening.rs:12-13` | ws1/ws2: none |
| `unused-imports` | **Rust-only** | files filtered to `Language::Rust` (`unused_imports.rs:95-103`, doc 89-90) | none |
| `untested-code` | works | language-neutral `refs` closure from `is_test` roots (`db/untested.rs:108-120`); C# roots via `[Fact]`/`[Test]`… (`csharp.rs:1363`) | ws2: 2 roots, 21 untested; ws1: indeterminate (0 roots) |
| `dead-code` | works, with C# false positives | C# candidate kinds `db/dead_code.rs:59-60`; `Main` entry point `139-145`; query `183-198`; type-level inherit only (`dead_code.rs:50-53`) | ws1 flags `IInner::Do` and explicit-impl `CompareTo` as DEFINITE, `IShape::Area/Prop` Maybe — artefacts of the Private default for interface members (§1) |
| `hierarchy` | works | `inherit` refs (`db/hierarchy.rs:83, 124-128, 191-196`); C# base lists `csharp.rs:164-198`; external supertypes shown as `?` | ws1 `hierarchy Derived` → Base; `hierarchy Shape` → `? IDisposable`, `? IComparable` |

---

## 8. KNOWN-GAP CROSS-CHECK

All eleven tracker items are `open` in `.rivets/issues.jsonl` and each is confirmed still present in code/data:

| Ticket | Still open? | Evidence |
|---|---|---|
| tethys-0aqj kind-blind binding | yes | Pass 1 `name_to_id` last-wins across kinds (`files.rs:387-394`); Pass 2 gate only excludes data members/macros/non-containers (`resolve.rs:46-60`). ws3 `Box<T>`/`Box` both `Box` → last wins; ws2 first run: methods `One`/`Two` declined against namespaces `One`/`Two` |
| tethys-5uqz member read shapes | yes | ws1 Refs.cs:38 (`Own`), :31 (initializer), :39 (`?.`), :40 (indexer) — no rows |
| tethys-cfme const/static-field/enum members | partially closed by xebx (const/static readonly → `struct_field`, ws1 rows 41/42); enum members still absent (ws1 `Color` has no members) | `csharp.rs:733-737` |
| tethys-3b06 interface-member suppression | yes | only type-level `inherit` (`csharp.rs:164-198`); `dead_code.rs:50-53`; ws1/ws3 impl methods carry no marker |
| tethys-alus alias using | yes | `resolve.rs:298-303`; ws2 `I.cs` `RT` unresolved |
| tethys-glus global using | yes | ws2 `Globals.cs` row is a plain per-file glob; `G.cs` unresolved |
| tethys-nnst nested namespaces | yes | ws2 `Outer`/`Inner` separate Module rows; `H.cs` bound by `unique_workspace`, file dep dropped |
| tethys-nvcy using-static trailing dot | yes (unfenced) | probe answers its question (1): `using static M.Repo.;` is stored as `M.Repo.` (dot kept, ws3 imports) — so the empty-`type_name` guard (`symbols.rs:438-440`) IS reachable for `using static Ns.;` where `Ns` is a known namespace |
| tethys-9181 obsolete constructors | yes | construct refs bind the class cross-file (ws2 `J.cs` sid 56); `callers Widget::Widget` → none |
| tethys-41lq visibility tightening C# | yes | `db/visibility.rs:149` |
| tethys-dsp1 display spelling | yes | CLI prints `Split::PartOne`, `Helper::Run`, `Shape::Shape` |

### Additional C# gaps found that no listed ticket covers

1. **Receiver-blind Pass-1 binding for C# calls.** The C# extractor never emits `ExtractedReferenceKind::Method`, so the 53iv receiver gate (`files.rs:452-457`) is Rust-only; every `anything.Save()` binds to a same-file `Save` by bare name (`files.rs:463-468`). ws3 Receiver.cs: 5/5 phantom `same_file` binds including `unrelated.Save()`. Same-file over-resolution is the mirror image of the cross-file under-resolution in (2).
2. **Instance-member calls never resolve cross-file.** `obj.Foo()` stores `obj::Foo`; the only qualified arm is `qualified_exact` on `Type::Member`, so it succeeds only when the receiver text literally equals a type name. ws2 `u::Ping`, `c::Save`, `x::Save`; ws1 `s::X`. Consequence: C# call graphs across files consist almost solely of constructs and static-style calls.
3. **`this.`/`base.` receivers are dropped** (`csharp.rs:533`), so `base.Hello()` binds to the override itself → `Hello→Hello` self-loop edges (ws1 call_edges `Hello→Hello ×3`) and the base implementation reads uncalled.
4. **`qualified_exact` has no ambiguity check** (`symbols.rs:111-122` `query_row`) — unlike every simple-name arm. ws2 `K.cs` `Collide.Save()` bound to NsTwo with no using while NsOne also defines `Collide::Save`.
5. **Overload identity absent**: cross-file `qualified_exact` picks the first overload row, same-file last-wins picks the last (ws2 F.cs sid 63 vs Helper.cs sid 64; ws1 `Get` → row 21). Deterministically wrong in one direction each.
6. **Generic arity/type args**: `class Box<T>` and `class Box` share name `Box`; bare generic invocation `Foo<int>()` emits **no ref** (`csharp.rs:404`); `obj.Foo<int>()` and `new A.B<int>()` keep `<int>` in the stored name (`obj::Echo<int>`, `M::Box<int>`) while `new B<int>()` strips it — three inconsistent spellings for the same symbol.
7. **Interface members default to `private`** (`csharp.rs:1335`) — dead-code reports them (`IInner::Do` DEFINITE). Not covered by 3b06 (which is about implementing classes).
8. **Explicit interface implementations**: extracted as a second bare-named `private` method (ws3 `Thing::Act` ×2, ws1 `CompareTo`), so they are dead-code DEFINITE candidates and collide with the public implementation in the last-wins map.
9. **Unextracted declarations**: destructors, operators, conversion operators, indexers, local functions, top-level statements/local functions (ws1 `Program.cs` has zero symbols), enum members, record positional properties (`Person(Name, Age)`); `record struct` stored as `class`.
10. **Modifiers unpersisted**: abstract/virtual/override/sealed/partial/unsafe/readonly/new absent; `async` computed but thrown away (`common.rs:17-24`); `is_unsafe` hard-coded false (`csharp.rs:1458`).
11. **Partial types**: N class rows, no merge; same-file parts NULL the parent linkage of every member (ws1 `Shape` members `pid` NULL vs ws3 `Repo` members linked).
12. **No `Type` references at all**: parameters, locals, returns, generic args, `typeof/is/as/cast/default/catch/where`, attribute names — a C# type used only in annotations has zero inbound evidence (dead-code Maybe via textual scan only); `unused`-style analyses can't see them.
13. **`nameof(X)` emits a bogus unresolved `call nameof`** (ws1 rows 14, 26) and `X` emits nothing.
14. **`new()` target-typed, `?.`, indexers, object initializers, LINQ query syntax, delegate `Invoke`, event handler method groups**: no refs (ws1 rows absent); `Act()` on a delegate field declines by kind gate (fine) but leaves no edge.
15. **Preprocessor**: both `#if` branches indexed at namespace level; member-level `#if` drops the members but keeps their body refs with NULL `in_symbol` (ws3 MemberIf).
16. **Using storage loses `static` and `global` flags and scope** (§3); single-identifier alias targets store an empty `source_module` (ws2 `L.cs`); dotted namespace aliases act as plain globs and over-resolve (ws2 `L2.cs` `import_union`).
17. **Same-namespace implicit visibility is not modeled**: siblings in the same namespace but different files resolve only by workspace uniqueness (ws4 `new Thing()` unresolved; ws2 `A.cs` speculative band).
18. **Qualified names omit the namespace and only the immediate parent** (`Inner::InnerMethod`), so `Type::Member` collides across namespaces and nesting depths (feeds 4).
19. **Top-level statements** produce refs with NULL `in_symbol` → no call edges, no file deps (ws1 `Program.cs`).
20. **Extension methods**: `x.Twice()` → `x::Twice` unresolvable; `3.Twice()` resolves only because the literal receiver is dropped and the name is workspace-unique.
21. **Test detection** ignores class-level `[TestClass]`/`[TestFixture]` and non-listed attributes (`csharp.rs:1363`); `affected-tests` is file-granular so unrelated tests in a dependent file are reported (ws2 `affected-tests Lib/Unique.cs`).
22. **Confidence ceiling**: C# refs can never reach the `high` band (`explicit_import` impossible, §3/§5a; `schema.rs:89-95`).

### Fenced today (for contrast)
Unit tests in `csharp.rs:1508-2604` fence: base-list inherit edges (incl. records), class/struct/interface/enum/record extraction, nested types (two namespace forms), compound visibilities, properties (auto/accessor/expression-bodied, in interface/struct/record/nested), fields per declarator with attributes, event field/accessor forms, delegates at both levels, member reads (simple, chained, callee-spine suppression, invocation-result/argument, assignment LHS, accessor-body attribution), static-method-as-function, ctor, namespaces, method signature/details, five using forms, bare/member calls, object creation (plain/generic), containing-symbol tracking, attribute stress. Integration fences: `tests/csharp_using_disambiguation.rs:89-121` (C3/C4/C5), `tests/csharp_using_static.rs:36-244` (6 cases), `tests/csharp_l2_file_deps.rs:33`, `tests/csharp_cross_dir_deps.rs:36`. Nothing fences overloads, generics, receivers, `this`/`base`, explicit interface impls, indexers/operators/destructors, local functions, top-level statements, preprocessor, aliases, global usings, or nested-namespace resolution.
