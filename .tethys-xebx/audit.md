# tethys-xebx corpus audit (checkpointed-build slice 11, 2026-07-05)

One-shot measurements against the real Tethys.Results corpus (31 indexed C#
files), diffed against expectations that were written BEFORE the feature
existed (`findings.md` prediction, `probe1-output.txt`, and the three
committed pre-feature baselines). Permanent forms of these measurements are
the named CI fences in `design.md`'s Falsification table.

| Claim | Expectation (pre-written) | Measured | Verdict |
|---|---|---|---|
| C1 | 42 `property` symbols (probe1 independent walk) | 42 | PASS |
| C2 | 2 `struct_field` symbols (`_value`, `_error`) | 2 | PASS |
| C5 | `Data` → `qualified_name Result::Data` (method convention) | `Result::Data` | PASS |
| C6 | 881 `field_access` refs (probe1, per-level) | 881 | PASS |
| C7 | decoy reads bind `same_file` to local `ApiResponse.Data` (×2) | both bound `same_file` | PASS |
| C8 | `result::Data` / `dataResult::Data` unresolved-qualified | both, strategy NULL | PASS |
| C9 | JSON: 1 symbol (kind `property`, note parsed, error null), exactly 2 Maybe sites (`BasicTests.cs:77`, `test-package.cs:23`), decoys absent | exact match, callers attributed (`Main`, `GenericResult_Value_ShouldReturnSameAsData`) | PASS |
| C10 | `call_edges` diff vs `baseline-call-edges.txt` empty | empty (300 = 300) | PASS (after D10) |
| C11 | call/construct refs diff vs baseline empty; `in_symbol` gains only in member bodies | refs diff EMPTY; exactly 2 in_symbol gains (TypedResult `Value`/`Error` getters) | PASS (after D10) |
| C12 | full suite green, unmodified Rust behavior | 886 tests pass every slice | PASS |
| C13 | no-op reindex: member symbol/ref dumps identical | identical (42+2 rows, 881 refs) | PASS |
| C14 | `search --kind property` and `stats` work on new kinds | 3 `Data` properties listed with type signatures; stats renders `Properties: 42`, `Struct Fields: 2` | PASS |

## The drift event (slice 7) and its resolution

The C11 audit initially FAILED: 2 fabricated call edges appeared —
`new Success<T>(value)` (Result.cs:117) and `new Exception("timeout")`
(TypedResultImplicitConversionTests.cs:182) Pass-1-bound by bare name to the
newly extracted same-file properties `Result.Success` and
`StepFailed.Exception`. Per the STOP-on-drift rule the build halted; dwalleck
approved amendment **D10** (data-member Pass-1 map consulted only by
`field_access` refs, mirroring `macro_name_to_id`; Pass-2
`ref_binds_to_symbol_kind` refuses Call/Construct → property/event/field).
Post-fix, both baseline diffs are bit-identical. Fences:
`member_reads_produce_no_call_edges`,
`construct_ref_does_not_bind_same_file_property` (embeds the exact drift
shape). The design-time collision check had measured the wrong join (member
names × existing symbols, not × same-file unresolved ref names) — recorded in
tethys-0aqj, which keeps the general kind-aware binding work.

## Fence inventory (all green at slice 11)

Unit (`src/languages/csharp.rs`): `extracts_property_auto_and_accessor_block`,
`extracts_property_expression_bodied_with_attribute`,
`extracts_property_in_interface_and_struct`,
`extracts_property_in_nested_class_with_enclosing_parent`,
`same_named_properties_in_two_classes_stay_distinct`,
`extracts_field_declarators_each_with_attributes`,
`extracts_event_field_and_accessor_forms`,
`extracts_delegate_at_namespace_and_class_level`,
`member_read_simple_emits_field_access`,
`member_read_chained_emits_one_ref_per_level`,
`member_read_skips_invocation_callee_spine`,
`member_read_on_invocation_result_and_in_arguments`,
`member_read_on_assignment_lhs_is_emitted`,
`no_member_access_means_no_field_access_refs`,
`accessor_body_refs_attribute_to_member_span`; `types.rs` roundtrip +
proptest extended.

Integration: `tests/graph.rs::member_reads_produce_no_call_edges`,
`tests/graph.rs::construct_ref_does_not_bind_same_file_property`,
`tests/member_reads.rs::member_read_cross_file_stays_qualified`,
`tests/member_reads.rs::call_resolution_with_member_symbol_declines_ambiguous`,
`tests/deprecated_callers.rs::csharp_obsolete_property_reader_sites`,
`tests/deprecated_callers.rs::csharp_obsolete_static_property_definite_site`,
`tests/attributes.rs::csharp_member_attribute_rows_match_source_and_survive_reindex`.
