# Feature: Batch and canonicalize file dependency cycles

## What this is

The Tethys library will detect directed cycles in the indexed file-dependency graph and project each cycle to indexed workspace-relative paths. Detection will bulk-load the file map and dependency edges, preserve stored edge direction, canonicalize rotations, and avoid database reads while converting cycle members.

## Users

- **Library integrator**: Calls `Tethys::detect_cycles()` for an indexed workspace and consumes stable `Cycle { files }` values for dependency diagnostics or automation.

## Behavior

### Empty and acyclic indexes
- **Given**: An empty workspace or an indexed workspace whose `file_deps` graph has no directed cycle.
- **When**: A library integrator calls `Tethys::detect_cycles()`.
- **Then**: The call succeeds with `Ok(Vec::new())`.

### Directed cycle projection
- **Given**: Indexed files with edges `A -> B`, `B -> C`, and `C -> A`, where an edge means the source file depends on the target file.
- **When**: A library integrator calls `Tethys::detect_cycles()`.
- **Then**: The result contains `[A, B, C]` in stored edge direction, with the first file omitted from the end.

### Self-loop projection
- **Given**: An indexed file with a `file_deps` edge from itself to itself.
- **When**: A library integrator calls `Tethys::detect_cycles()`.
- **Then**: The result contains one cycle whose `files` value contains that path exactly once.

### Overlapping directed cycles
- **Given**: One strongly connected component contains multiple distinct simple directed cycles.
- **When**: A library integrator calls `Tethys::detect_cycles()`.
- **Then**: The result contains every simple directed cycle, with duplicate database edges impossible under the `file_deps` primary key and no artificial cycle-length or cycle-count cap.

### Rotation identity and direction
- **Given**: The graph contains several rotations of the same directed cycle, and may also contain the reverse directed edges.
- **When**: A library integrator calls `Tethys::detect_cycles()`.
- **Then**: Rotations are returned once, rotated so the lexicographically smallest workspace-relative path is first; a reverse-direction cycle remains distinct when its directed edge sequence differs.

### Stable result ordering
- **Given**: Multiple canonical cycles exist.
- **When**: A library integrator calls `Tethys::detect_cycles()` repeatedly on the unchanged index.
- **Then**: The returned `Vec<Cycle>` is ordered deterministically by each cycle's canonical relative-path sequence.

### Path projection and integrity
- **Given**: Cycle members are indexed workspace-relative paths, including paths with spaces or Unicode.
- **When**: A library integrator calls `Tethys::detect_cycles()`.
- **Then**: The returned `PathBuf` values preserve the stored relative spelling exactly; no absolute or non-indexed path is emitted. If an edge endpoint has no indexed `files` row, the call returns `Error::NotFound` rather than emitting an invalid cycle.

### Set-oriented hydration
- **Given**: Any indexed workspace, including empty and acyclic workspaces.
- **When**: A library integrator calls `Tethys::detect_cycles()`.
- **Then**: the implementation performs one set-valued read for all indexed files and one set-valued read for all dependency edges, and performs zero database lookups while iterating cycle members.

### Consistent read snapshot
- **Given**: Indexing writes the same SQLite database while cycle detection runs.
- **When**: A library integrator calls `Tethys::detect_cycles()`.
- **Then**: both bulk reads observe one SQLite read snapshot; no cycle is assembled from files and edges belonging to different database states.

## Success criteria

- **Empty/acyclic result**: `0` cycles for `2` fixtures (empty and acyclic), measured by Tethys integration tests.
- **Basic cycle coverage**: `1` exact two-file cycle and `1` exact four-file cycle, measured by integration assertions on canonical relative paths and edge direction.
- **Overlapping/directional coverage**: `2` distinct cycles for a three-file graph containing both directed orientations, plus an overlapping shorter cycle when present, measured by exact canonical cycle-set comparison.
- **Rotation deduplication**: `1` result for `3` equivalent rotations of one directed cycle, measured by exact result count and canonical first path.
- **Self-loop coverage**: `1` one-file cycle for `1` self-loop edge, measured by an integration assertion that the path occurs once.
- **Path integrity**: `100%` of returned members are indexed, workspace-relative, stored spellings; measured by fixture assertions including Unicode and spaces and by the dangling-edge error test.
- **Hydration query count**: `2` set-valued `SELECT` statements per `detect_cycles()` call, including empty and acyclic calls, and `0` scalar file lookups per cycle member, measured with the SQLite trace hook and a scalar-lookup canary.
- **Repeatability**: `3/3` repeated calls on an unchanged index return byte-for-byte-equivalent cycle path sequences, measured by an integration test.
- **Snapshot consistency**: `1` database read snapshot per call, measured by a transaction-level concurrency test that cannot observe mixed file/edge states.

## Edge cases and decisions

| Edge | Decision | Rationale |
|---|---|---|
| Empty workspace | Return success with no cycles. | Explicit ticket acceptance criterion. |
| Acyclic workspace | Return success with no cycles. | Explicit ticket acceptance criterion. |
| One-file self-loop | Report a one-file cycle. | A self-edge is a directed cycle and must not be silently discarded. |
| Multiple simple cycles in one SCC | Report every simple directed cycle. | The feature is cycle detection, not merely SCC detection; distinct paths carry distinct direction information. |
| Equivalent rotations | Deduplicate. | Explicit ticket acceptance criterion. |
| Reverse direction | Keep distinct when the directed sequence differs. | Explicit ticket acceptance criterion. |
| Cycle orientation | Follow `from_file_id -> to_file_id`. | `file_deps` stores the source file's dependency direction. |
| First file repeated at end | Never repeat it in `Cycle.files`. | Explicit ticket acceptance criterion; the CLI may append it for display only. |
| Multiple result cycles | Sort by canonical relative-path sequence. | Stable library and CLI output. |
| Canonical first file | Lexicographically smallest workspace-relative path. | Stable across index ID assignment and equivalent rotations. |
| Paths with spaces or Unicode | Preserve stored relative spelling. | Avoid lossy or absolute projection at the API boundary. |
| Duplicate dependency rows | Treat as one edge. | `file_deps` primary key is `(from_file_id, to_file_id)`. |
| Dangling edge endpoint | Return `Error::NotFound` for any dangling `file_deps` endpoint, whether or not it lies on a cycle. | Do not emit a path that is not indexed. NOTE (review, 2026-08-02): this is deliberately *wider* than the behavior it replaced — `ids_to_cycle` errored only on a dangling cycle member, so an acyclic index holding one corrupt edge now returns `Err` where it previously returned `Ok(vec![])`. It is also stricter than the sibling `file_deps` readers (`get_file_dependency_paths`, `get_file_dependent_paths`), which skip and count such rows without erroring. Accepted knowingly: validating up front is what lets hydration project from the snapshot with no per-member lookup. |
| Concurrent indexing | Read one consistent SQLite snapshot. | Prevent mixing file rows and edge rows from different index states. |
| Long cycles | Do not silently cap cycle length or count. | The requester selected complete simple-cycle enumeration. |
| Missing filesystem source after indexing | Use the indexed database path. | Cycle detection consumes the index; it does not rescan the workspace. |
| Retry after a read error | Propagate the database error; no internal retry. | Read-only query semantics do not add retry policy. |
| Permissions/authentication | Not applicable; the local SQLite index is the access boundary. | No remote or per-record authorization layer exists. |
| Multi-tenancy | Not applicable; one Tethys instance owns one workspace/index. | Workspace isolation is established by `Tethys::new`. |
| Time zones/DST | Not applicable; cycle output depends only on graph rows and paths. | No time-based ordering or filtering is involved. |
| Cache invalidation | Not applicable; no cycle-result cache is introduced. | Each call reads the current index snapshot. |

## Out of scope

This change does NOT include:

- Changing Rust or C# import extraction, reference resolution, or `file_deps` edge construction.
- Detecting symbol-call cycles, type-hierarchy cycles, package cycles, or cycles in files outside the indexed workspace.
- Changing the public `Cycle` shape or the CLI's human-readable closure marker; the CLI may continue appending the first file only when displaying a cycle.
- Adding an API-visible depth, count, or time limit to cycle enumeration.
- Scanning the filesystem to repair missing or stale indexed rows.

## Constraints

| Dimension | Limit | How measured |
|---|---|---|
| Database reads | Exactly `2` set-valued `SELECT`s per call, independent of cycle/member count. | SQLite trace hook over empty, acyclic, and cyclic fixtures. |
| Per-member lookup | `0` scalar file lookups during conversion. | Trace canary rejects `FROM files WHERE id =` after cycle discovery begins. |
| Path form | `100%` workspace-relative indexed paths. | Integration fixture with nested, Unicode, and spaced names plus invalid-edge test. |
| Result identity | `1` result per canonical directed cycle; reverse sequences remain separate. | Exact cycle-set assertions. |
| Snapshot | `1` SQLite read snapshot per call. | Transaction-level concurrent-write test. |

## Decisions log

| # | Question | Decision | Why |
|---|---|---|---|
| 1 | Who is the primary audience? | Library integrator. | The behavior is exposed behind `Tethys::detect_cycles()`. |
| 2 | Should a self-edge count? | Report one-file cycle. | It is a directed cycle and is observable in the indexed graph. |
| 3 | How should multiple results be ordered? | Deterministic canonical order. | Stable consumer output is required. |
| 4 | Which rotation is canonical? | Lexicographically smallest relative path first. | It is independent of database insertion IDs. |
| 5 | Which direction is dependency order? | Follow stored `from -> to` edges. | Matches `file_deps` semantics. |
| 6 | How should overlapping cycles be handled? | Return every simple directed cycle. | Preserve distinct graph paths and direction. |
| 7 | What is the hydration query budget? | Two set-valued queries. | One file map plus one edge map; no per-member lookup. |
| 8 | How are unusual relative paths projected? | Preserve stored spelling. | Avoid lossy or absolute output. |
| 9 | Is there a length/count cap? | No artificial cap. | Do not silently omit indexed directed cycles. |
| 10 | What if indexing writes concurrently? | Use one consistent read snapshot. | Prevent mixed-state results. |
| 11 | What if an edge endpoint is missing? | Return `Error::NotFound`. | Invalid paths must not appear in results. |
| 12 | Does the query budget apply to empty/acyclic calls? | Yes, fixed two-query read. | The implementation's bulk-loading contract is unconditional. |

## Sign-off

The library-integrator contract is: enumerate complete simple directed file-dependency cycles, including self-loops; preserve edge direction and relative path spelling; canonicalize and deterministically order results; and hydrate through two set reads inside one snapshot with no per-member lookups.

The requester agreed: "Agree"

Date: 2026-08-02
