# tethys-j2r1 — slice 5 audit (oracle closure)

## Probe ⇄ binary (C1/C2/C8), self-index

| measure | probe | binary |
|---|---|---|
| type-level Implements edges | 27 | 27 (24 anchored + 3 cross-file NULL) |
| supertrait edges | 10 (grep-exact) | 10, all anchored to their trait |
| subtype same-file anchoring | 24/27 | 24/27 (**exact**) |
| unresolved retained | 21 impl + 10 supertrait external | 45 total (both granularities) |
| name distribution | Display 8, From 5, Send/Sync 5+5 | identical |

Method markers: 55 (= methods inside the 27 trait impls). `tethys
hierarchy ModuleResolver` smoke: up {Send, Sync external}, down
{RustModuleResolver, CSharpModuleResolver} — matches the probe rows.

## C8 — analyses isolation (delete-inherit-rows, same binary/tree)

unused-imports, visibility-tightening, deprecated-callers, panic-points,
untested-code: **5/5 IDENTICAL** with vs without the 92 inherit rows.
(untested-code identical confirms markers/edges don't perturb the refs-BFS
closure: inherit in_symbols are methods/types already reachable or roots'
targets — no reachability change measured.)

## C4 — call_edges

0 inherit-sourced rows (fence F-H5 + slice-2 SQL check).

## Fences

Unit: trait_impl_emits_type_edge_and_method_markers,
supertrait_bounds_emit_anchored_edges, base_list_emits_anchored_inherit
_edges (C#). E2E (`tests/type_hierarchy.rs`): F-H1/F-H4+C9 suppression
join, F-H3 retention + Path-B non-pollution, F-H5 call_edges, F-H2/F-H7
transitive walks w/ depths + NotFound, F-H6 C# both directions, F-H8
binary JSON seam. Suite 982/982.
