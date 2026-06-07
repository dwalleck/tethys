# Probe findings (prove-it-prototype, 2026-06-06)

Probe: `.csharp-ns/probe.sh` — real binary, fixture covering 4 namespace
declaration styles, 5 using-forms, used + unused usings. Slice 2 added a
name collision. Oracles: source grep (ref counts), `tethys search` CLI
(different query path), hand-read ground truth, direct csharp.rs read.

## What I learned (one sentence)

Unqualified C# type/member refs ALREADY resolve whenever their simple name
is workspace-unique (the unscoped unique-name fallback fires long before
any using-directive matters), so the using-arm's new value is strictly
**collision disambiguation** — and the signed spec's B1 ("was NULL before")
and B2 ("bare members stay unresolved") are both factually wrong as written.

## Per-question results

| Q | Result |
|---|---|
| Q-core | `new Widget()` under `using My.Models;` → **resolved TODAY** to Widget@Models.cs (unique fallback). Bare `Assist()` → **resolved TODAY** to Helper::Assist (same arm, no kind filter). |
| Slice 2 (collision) | Second `Widget` in `Dupe.Ns` → App.cs's Widget ref **UNRESOLVED** (unique-decline confirmed; oracle: search CLI shows exactly 2 candidates). THE feature gap. |
| Q1 | jwf9's csharp.rs:109-111 reference is stale — that region is now `UsingDirective`; the old empty-vec `resolve_import` no longer exists. |
| Q2 | Block-scoped dotted (`My.Models`) and file-scoped (`My.Scoped`) namespaces store as single dotted module symbols matching `source_module` strings exactly. **Nested block namespaces store as SEPARATE symbols (`Outer1`, `Inner1`) — never `Outer1.Inner1`** — so the symbols table alone cannot key the namespace map for nested declarations (the post-pass's map builder must reconstruct nesting; design constraint). |
| Q3 | Ref kinds observed: `construct`, `call`. Symbol kinds (class/module/method/function) support a types-only filter. (Type-annotation refs not exercised — fixture used `var`; extractor's Type kind exists per common.rs.) |
| Q4 | L1 baseline confirmed: unused `using Other.Stuff;` → `App.cs → Other.cs (1)` edge exists today and is exactly what decision #2 (L2) removes. NOTE: file_deps also receives call-edge-derived rows (App→Models ref_count 4 includes them) — the B5 delta enumeration must separate import-edges from call-edges. |
| Q5 | `using static My.Models.Helper;` → glob row `*|My.Models.Helper` (**is_static dropped** at csharp.rs:118 `to_import_statement`); `using W = My.Models.Widget;` → `*|My.Models.Widget|W`; `global using` → plain glob row in its own file only (no propagation). Type-level dotted paths decline namespace lookup naturally (no namespace bears those names). |

## Spec contradictions → returned to interrogation

1. B1's "Then: was NULL before this change" — false for unique names; only
   collision cases gain resolution.
2. B2's "bare `Hash(...)` stays unresolved" — false today for unique names
   (fallback resolves them, kind-blind); under B8 monotone-stability they
   must KEEP resolving. Types-only (decision #3) constrains the NEW
   using-arm only.
3. The feature's symbol-resolution half is namespace-scoped DISAMBIGUATION,
   not first-time namespace resolution.
