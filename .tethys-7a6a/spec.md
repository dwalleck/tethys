# Feature: Unified directional reachability

## What this is

Tethys will expose one canonical direction-parameterized reachability operation. It will traverse one bulk call-graph snapshot with predecessor reconstruction while preserving the current observable reachability contract. The CLI will map its existing forward and backward values to that operation; the two legacy public wrappers will delegate until `tethys-71if` removes them.

## Users

- **Rust library consumer**: calls the `Tethys` facade and receives a `ReachabilityResult` with one shortest path per reachable symbol in BFS discovery order.
- **Tethys CLI operator**: runs `tethys reachable` with forward or backward direction and sees the same grouped reachability output and defaults as before.

## Behavior

### Canonical library traversal
- **Given**: an indexed source symbol, a `ReachabilityDirection`, and an optional maximum depth.
- **When**: a Rust library consumer invokes the canonical `Tethys` reachability operation.
- **Then**: Tethys returns a `ReachabilityResult` whose direction matches the request and whose entries contain each reachable symbol once at its minimum depth.

### Forward traversal
- **Given**: an indexed source symbol with outgoing call edges.
- **When**: the canonical operation receives `ReachabilityDirection::Forward`.
- **Then**: traversal follows caller-to-callee edges.

### Backward traversal
- **Given**: an indexed source symbol with incoming call edges.
- **When**: the canonical operation receives `ReachabilityDirection::Backward`.
- **Then**: traversal follows callee-to-caller edges.

### Path projection
- **Given**: a target first discovered at depth N.
- **When**: the result is projected after traversal.
- **Then**: its path contains exactly N symbols, excludes the source, ends with the target, and follows valid call edges in the requested direction.

### Discovery order
- **Given**: a graph with multiple targets at the same and different depths.
- **When**: reachability is computed.
- **Then**: entries remain in queue discovery order: lower depths first and deterministic qualified-name adjacency order within each dequeued predecessor; the library does not globally alphabetize the completed result.

### Depth handling
- **Given**: an indexed source and maximum depth zero, one, omitted, finite, or larger than `u32::MAX`.
- **When**: reachability is computed.
- **Then**: zero validates the source and returns no targets; one returns direct targets only; omitted uses 50; finite values bound traversal monotonically; oversized values saturate at `u32::MAX` and emit the shared warning.

### Cycles and duplicate routes
- **Given**: a cyclic graph, a self-loop, or multiple routes to the same target.
- **When**: reachability is computed.
- **Then**: traversal terminates, never returns the source as reachable from itself, and returns every other target once through the first shortest-depth predecessor.

### CLI mapping
- **Given**: `tethys reachable <symbol>` with `forward`, `f`, `backward`, or `b` and its existing maximum-depth option.
- **When**: a CLI operator runs the command.
- **Then**: the CLI maps the value to the corresponding direction, calls the canonical operation, retains CLI default depth 10, and rejects unsupported direction strings as configuration errors.

### Temporary compatibility wrappers
- **Given**: a Rust library consumer calls `get_forward_reachable` or `get_backward_reachable` before `tethys-71if` lands.
- **When**: either wrapper is invoked.
- **Then**: it delegates to the canonical operation and cannot execute a separate traversal implementation.

### Symbol projection correctness
- **Given**: a forward-reachable non-test symbol.
- **When**: the bulk graph snapshot is decoded.
- **Then**: the target retains `is_test == false`; call counts are never decoded as the `is_test` field.

## Success criteria

- **Canonical surface**: exactly 1 canonical direction-parameterized `Tethys` operation, measured by public-interface inspection; exactly 2 temporary wrappers delegate to it until `tethys-71if`.
- **Directional correctness**: 2/2 direction fixtures match an independently enumerated call graph, measured by Tethys integration tests.
- **Shortest-depth uniqueness**: 100% of returned symbol IDs are unique and each depth equals an independent BFS minimum, measured on a diamond-plus-cycle fixture.
- **Path invariants**: 100% of returned paths exclude the source, include the target last, have `path.len() == depth`, and contain only direction-valid adjacent pairs, measured by integration tests in both directions.
- **Ordering**: 100% of entries match an independent queue-order oracle on a fixture where BFS discovery order differs from global alphabetical order.
- **Depth contract**: 6/6 cases—zero, one, omitted, finite, `u32::MAX`, and larger than `u32::MAX`—match the shared contract, measured by integration tests and warning capture for saturation.
- **Cycle safety**: 2/2 directions terminate on a cycle with a self-loop and return the source 0 times, measured by integration tests.
- **Set-oriented database access**: increasing reachable targets from 1 to at least 100 adds 0 SQLite statements to one operation, measured by a canary-guarded trace callback.
- **Predecessor search state**: 0 growing `Vec<Symbol>` paths are cloned or queued during BFS; each discovered symbol stores at most 1 predecessor, measured by implementation audit and path-equivalence tests.
- **Projection correctness**: 1/1 forward-reachable non-test target retains `is_test == false`, measured by an integration fixture that would fail if `call_count` occupied the symbol projection's `is_test` column.
- **CLI integration**: 4/4 accepted direction spellings reach the matching canonical direction, and 1/1 unsupported spelling returns a configuration error, measured by CLI integration tests.

## Edge cases and decisions

| Edge | Decision | Rationale |
|---|---|---|
| Source symbol does not exist | Return `Error::NotFound`, including at depth zero. | Depth zero validates rather than bypassing lookup. |
| Multiple symbols share the requested qualified name | Preserve current first-row source lookup behavior. | The ambiguity and unique-or-decline fix are tracked by `tethys-bvgb`; this traversal change must not silently alter resolver semantics. |
| Source has no edges in the requested direction | Return an empty reachable list with the effective depth and requested direction. | Empty reachability is a valid result. |
| Maximum depth is zero | Validate the source and return no reachable targets. | Shared traversal-depth contract from `tethys-u1rs`. |
| Maximum depth is omitted | Use 50 in the library result. | Existing library and shared graph-query default. |
| CLI maximum depth is omitted | Pass 10 explicitly. | Existing documented CLI default remains unchanged. |
| Maximum depth exceeds `u32::MAX` | Saturate to `u32::MAX` and warn once. | Shared traversal-depth contract from `tethys-u1rs`. |
| Source has a self-loop | Do not return the source. | Source is seeded as visited before traversal. |
| A cycle returns to the source | Terminate and do not return the source. | Same visited invariant as a self-loop. |
| Target has multiple routes at different depths | Retain only the first shortest-depth route. | BFS discovery plus first-predecessor ownership. |
| Target has multiple routes at the same depth | Retain the route discovered first by deterministic adjacency order. | Makes path selection reproducible without global result sorting. |
| Reachability contains speculative call edges | Traverse all indexed call edges. | Provenance filtering is scoped to callers and impact by `tethys-6k6b`; legacy reachability has no exclusion mode. |
| Bulk query returns test and non-test symbols | Decode the complete symbol projection, including the real `is_test` column. | Avoids inheriting the projection defect tracked by `tethys-6bui`. |
| A call edge has a dangling endpoint | Preserve the current fail-fast database error posture. | A broader posture decision is tracked by `tethys-e3j1` for `tethys-71if`. |
| Legacy wrapper is called | Delegate to the canonical operation. | Requester retained wrappers until `tethys-71if` while requiring one traversal implementation. |

## Out of scope

This change does NOT include:

- Removing the two delegating public wrappers; `tethys-71if` owns that final cutover.
- Repairing existing `get_callees` or `get_transitive_callers` projection defects; `tethys-6bui` owns those methods. The new operation still decodes its own rows correctly.
- Changing dangling-edge error posture; `tethys-e3j1` owns that decision for `tethys-71if`.
- Changing duplicate-qualified-name source resolution; `tethys-bvgb` owns the unique-or-decline behavior change.
- Repairing resolver under-count classes tracked by `tethys-staf`, `tethys-qtq5`, and `tethys-z9mr`.
- Adding Petgraph, graph adapter traits, mocks, or a second graph seam; ADR-0002 and `tethys-mv36` require concrete `Index` graph operations behind `Tethys`.
- Changing CLI direction spellings, output layout, or the CLI default depth of 10.

## Constraints

| Dimension | Limit | How measured |
|---|---|---|
| Public traversal implementations | 1 canonical operation | Interface and callsite inspection |
| SQLite scaling | 0 additional statements when reachable targets grow from 1 to at least 100 | Canary-guarded SQLite trace test |
| Search path storage | 0 cloned partial paths during BFS; at most 1 predecessor per discovered symbol | Implementation audit plus path-equivalence fixture |
| Result uniqueness | 1 row per reachable symbol ID | Integration assertion |
| Path validity | `path.len() == depth`, source occurrences 0, target last 100% | Directional integration assertions |
| Depth default | Library 50; CLI 10 | Library and CLI integration tests |
| Backward compatibility | Existing CLI invocation and output posture unchanged; 2 legacy Rust wrappers remain delegating | CLI assertions and public callsite tests |
| Architecture | CLI → `Tethys` seam → concrete `Index` graph operation → SQLite | Seam lint and module inspection |

## Decisions log

| # | Question | Decision | Why |
|---|---|---|---|
| 1 | Which direction does each mode follow? | Forward follows callees; backward follows callers. | Explicit in `tethys-7a6a`, parent `tethys-6k6b`, and `CONTEXT.md`. |
| 2 | What depth contract applies? | Zero validates and returns empty; one is direct; omitted is 50; finite bounds are monotone; oversized saturates with a warning. | `tethys-7a6a` requires the shared contract established by closed blocker `tethys-u1rs`. |
| 3 | Does reachability expose speculative-edge filtering? | No; it traverses all indexed call edges. | Parent `tethys-6k6b` scopes filtering to callers and impact, matching legacy reachability. |
| 4 | How are equal-depth ties ordered? | Deterministic BFS queue discovery using qualified-name adjacency order, not global result sorting. | Preserves the current operation while satisfying the explicit BFS-order criterion. |
| 5 | Does this PR remove the two public wrappers? | No. They delegate until `tethys-71if` removes them. | Requester explicitly selected retention on 2026-08-08 to reconcile contradictory tracker wording. |
| 6 | How is the open `is_test` projection defect handled? | The new bulk query selects and decodes the real symbol columns and adds a non-test forward-reachability fence; adjacent methods remain tracked by `tethys-6bui`. | Prevents the new operation from inheriting the known defect without absorbing unrelated repair scope. |
| 7 | What happens when the source qualified name is duplicated? | Preserve the current first-row lookup and leave unique-or-decline semantics to `tethys-bvgb`. | The probe found 75 duplicate groups in the self-index, exposing a pre-existing resolver defect rather than traversal behavior. |

## Sign-off

Agent summary:

> One canonical `Tethys` operation will traverse forward or backward over a single call-graph snapshot, preserve deterministic BFS order, shortest-depth uniqueness, exact path and depth behavior, and cycle safety, and reconstruct paths from one predecessor per discovered symbol. The CLI keeps its current inputs and defaults. The two old public methods remain only as delegating wrappers until `tethys-71if`. The new query must decode complete symbol rows so non-test targets remain non-test. Duplicate-qualified-name source lookup remains unchanged and tracked by `tethys-bvgb`; unrelated graph-projection and resolver defects stay with their existing tracker issues.

The requester agreed: "Yes, approve amendment"

Date: 2026-08-08
