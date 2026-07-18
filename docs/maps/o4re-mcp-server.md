# Wayfinder map — tethys-o4re: MCP server for tethys tools

Goal: a shippable implementation plan for `tethys-mcp`. Charted 2026-07-17
from a grilling session; the issue (rivets tethys-o4re) carries the pi-lens
reference mechanisms this map repeatedly cites.

## Charter (decided in the charting session — do not re-litigate)

1. **Home**: workspace conversion — `crates/tethys` (lib+cli) +
   `crates/tethys-mcp` (bin). rmcp/tokio never enter the core lib.
2. **Surface**: all shipped analytics (~18 tools), two waves — wave 1
   navigation core (status, symbol, search, callers, callees, impact,
   file_impact, reachable, cycles, affected_tests, panic_points); wave 2
   analytics pack (coupling, hierarchy, dead_code, untested_code,
   deprecated_callers, unused_imports, visibility_tightening). Unbuilt
   features (path, hotspots, surprising, diff, FTS5) stay out.
3. **Lifecycle**: read-only tools + honest staleness in every reply +
   explicit `tethys_reindex` tool. No daemon/watcher in v1.
4. **Envelope**: uniform from day one — `generated_at`, staleness,
   per-section provenance, `standing: confirmed|indeterminate`. Types live
   in the core crate; tethys-09wx sequenced independently but must reuse
   them (read its decided contract as a constraint).
5. **Transport**: standalone stdio binary, rmcp 0.8 (rivets-mcp is the
   reference implementation at ~/repos/rivets/crates/rivets-mcp).
6. **Root**: fixed per server instance at startup (`--workspace`, default
   cwd); startup discovery audited against the four pi-lens failure modes
   (marker walk-up, VCS-boundary stop, home ceiling, null-not-cwd).
7. **Approval**: read-only tools auto-approved + `readOnlyHint`
   annotations; `tethys_reindex` requires approval. LSP warm-keeping out
   of v1. Testing per repo gates (in-process rmcp client + envelope fences).

## seam-fence: Land tethys-r4j2 (CLI seam fence)

Blocked by: —
Status: open
Type: Task

### Question

o4re's first AC extends the r4j2 fence to the mcp module — but r4j2 (a
test-only fence: src/cli must not import crate::db / hold &Index, style of
tests/seam_lint.rs) is itself still open. Land it via the ship pipeline so
the seam is guarded before the second adapter exists.

### Answer

## rmcp-survey: rmcp 0.8 server API + rivets-mcp reference read

Blocked by: —
Status: open
Type: Research

### Question

What does rmcp 0.8 give us and what does rivets-mcp already solve? Cover:
tool registration/schema derivation (schemars?), tool annotations
(readOnlyHint availability), error mapping surface (tethys::Error incl.
PackageNotFound → MCP error shape), long-running tool support (for
reindex), in-process test client patterns, tokio runtime setup for a
stdio server, and cargo-deny/license posture of the new dep tree. Asset:
summary markdown with file-level pointers into rivets-mcp.

### Answer

## envelope-design: Reply envelope + staleness semantics + vocabulary

Blocked by: —
Status: open
Type: Grilling

### Question

Concrete schema for the uniform envelope (charter #4): field names and
types; how staleness is computed per query without blowing the budget
(mtime walk is O(files) syscalls — cached? threshold? pi-lens used a
10-minute built-at cutoff); the standing vocabulary against tethys-09wx's
already-decided CLI contract; per-section provenance granularity (bands,
resolved-refs, lsp); where the shared types live in the core crate.
Consult /domain-modeling; new terms (envelope, standing, provenance,
staleness) enter CONTEXT.md as part of the resolution.

### Answer

## conversion-spike: Workspace conversion breakage survey

Blocked by: —
Status: open
Type: Prototype

### Question

Throwaway branch: mechanically convert the repo to a workspace
(crates/tethys) and record everything that breaks — CI workflows,
tarpaulin, release/changelog scripts, seam_lint include_str paths,
benches, cargo-husky hooks, committed .tethys-* artifact references, docs.
Asset: the breakage list (the branch is discarded). This prices the
conversion ticket and prevents the real one from discovering surprises
mid-flight.

### Answer

## tool-schemas: Per-tool input/output schema design

Blocked by: envelope-design, rmcp-survey
Status: open
Type: Prototype

### Question

For all wave-1+2 tools: parameter conventions (qualified_name vs file
path shapes), result truncation/pagination for unbounded outputs (impact
and reachable can be huge — what's the token-budget posture?), and one
worked example reply per tool (envelope included) to react to. Asset:
schema stubs + example replies.

### Answer

## reindex-tool: tethys_reindex contract

Blocked by: rmcp-survey, envelope-design
Status: open
Type: Grilling

### Question

Incremental vs full rebuild; SQLite locking behavior when queries arrive
mid-reindex (WAL? busy timeout? refuse-with-standing?); progress/result
reporting shape; what the envelope's staleness field reads during and
after. The concurrent-access question likely needs a small probe.

### Answer

## agent-ux-spike: Minimal live server driven from Claude Code

Blocked by: tool-schemas
Status: open
Type: Prototype

### Question

Rough 2-3 tool server (search, callers, status) wired into a real Claude
Code session against this repo: does the envelope read well in-model, do
auto-approve annotations behave, does the staleness hint actually steer
the agent to reindex? Asset: transcript + findings. Throwaway code, not
the production scaffold.

### Answer

## rollout-plan: Implementation sequencing

Blocked by: seam-fence, conversion-spike, tool-schemas, reindex-tool, agent-ux-spike
Status: open
Type: Task

### Question

Assemble the final slice sequence and file the rivets issues (workspace
conversion; scaffold + envelope + wave 1; wave 2; reindex tool; docs +
client config), each sized for the ship pipeline. Update tethys-o4re with
the plan; the map closes.

### Answer

## Notes

Domain: Rust engineering on tethys (read AGENTS.md first; CONTEXT.md is
the glossary). Implementation tickets go through the ship pipeline
(/ship), not resolved inside map sessions. Consult /rust-best-practices
for any code, /domain-modeling when terms are coined. The rivets issue
tethys-o4re is the canonical requirements record; this map is the route
to its plan.
