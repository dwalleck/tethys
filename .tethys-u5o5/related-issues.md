# Related issues

Tracker search (2026-08-02) matched the following prior art:

- **tethys-v21s** — original `detect_cycles()` implementation ticket; it predates the concrete `Index` graph cutover and is superseded in shape by this issue, but documents the prior cycle API and behavior.
- **tethys-4m9o** — batched shortest dependency-chain queries; adjacent file-graph hydration and path-order semantics.
- **tethys-n8pu** — batch direct file-dependency/dependent queries; adjacent set-oriented file hydration pattern.
- **tethys-vwrn** — dependency-chain traversal can enumerate unbounded walks on cyclic indexes; relevant negative-space context, but this issue is limited to cycle detection.
- **tethys-r77e** — file dependency edges can be misattributed by module resolution; relevant to interpreting indexed edges, not part of this change.

No additional open issue matched the exact cycle batching/canonicalization request.
