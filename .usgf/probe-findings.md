# Probe findings (prove-it-prototype, 2026-06-07)

Probe: `.usgf/probe.sh` — real binary, fixture with colliding + unique
members across methods, consts, static fields, enum members; static + plain
usings. Oracles: source grep, `tethys search` CLI (different query path),
direct csharp.rs read.

## What I learned (one sentence)

The C# extractor only emits CALLABLE members as symbols (kind
`function`/`method`, qualified_name `Type::member`, no parent link) and only
CALL-shaped references as refs — so `using static` disambiguation is reachable
for **static method calls only**; consts, static fields, and enum members are
neither indexed as symbols nor extracted as references, making B2's "any
member kind" largely a fantasy.

## Per-question results

| Q | Result |
|---|---|
| CORE | `Assist()` (collides: Helper::Assist + Other::Assist) → **UNRESOLVED today** (the gap). `Configure()` (unique) → **resolved** to Tools::Configure via the fallback (monotone baseline). Oracle: `tethys search Assist` → 2 symbols; grep → 2 defs. ✓ |
| **B2 FALSIFIED** | `var n = MaxRetries;` and `var p = Pi;` produced **NO refs** in App.cs (only the two method calls appear). `MaxRetries`/`Pi`/`Red` are **NOT in the symbols table** (`search` → "No symbols found" ×3, grep confirms the source has them). Const, static-field, enum-member members are invisible to tethys end to end. |
| Q1 | `using static My.Models.Helper;` → `*\|My.Models.Helper`; plain `using My.Models;` → `*\|My.Models`. is_static dropped — BUT the two are distinguishable by shape: the static form's `source_module` names a TYPE (last segment is a class), the plain form names a namespace. |
| Q2 | Static members stored kind `function`, qualified_name `Helper::Assist`, **parent_symbol_id = None**. The type-scoping handle is the `qualified_name` prefix (`Helper::`), not a parent link. |
| Q4 | `source_module` = full `namespace.Type` (`My.Models.Helper`), NOT just `Helper`. The namespace map (keyed `My.Models`) misses `My.Models.Helper`, which is exactly why `Assist` is unresolved today. |
| Q5 | const / static-field / enum-member symbols: **not extracted at all** (parser gap, adjacent to tethys-itez/778r). Enum TYPE is extracted; its members are not. |

## Implications for the spec (return to interrogation)

1. **B2 must narrow to static methods only.** Consts/static-fields/enum
   members are out — not by choice but because the substrate doesn't index
   them (symbols absent, refs absent). The const/field/enum-member
   extraction gap is filed as a dependency (tethys-cfme), NOT in this loop.
2. **Decision #4 (is_static storage) is likely unnecessary.** The static
   using is distinguishable from a plain namespace using by type-detection
   (`source_module` names a type whose namespace-prefix is in the map),
   with no schema change. B5 ("the static arm fires for the right rows") is
   satisfiable this way. Re-decide at sign-off: keep the storage option or
   adopt type-detection.
3. **Type-scoping handle is qualified_name** (`Type::member`), not
   parent_symbol_id — a design constraint for the member-lookup query.
4. Core premise HOLDS: colliding method names stay UNRESOLVED (the gap),
   unique ones already resolve (monotone baseline). The loop's value
   survives; only its breadth shrank.
