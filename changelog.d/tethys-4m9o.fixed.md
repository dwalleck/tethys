- Shortest dependency-chain queries (`get_dependency_chain` in the library
  API) previously never terminated on workspaces whose file dependency
  graph contains cycles — which real workspaces do. The traversal now
  answers in milliseconds on cyclic graphs.
- Chain results are unchanged for workspaces where the query worked
  before: directed shortest path by edge count including both endpoints,
  a one-file path for equal endpoints, no path (rather than an error) for
  disconnected files, and a not-found error for unindexed endpoints.
- The selected chain's files are now fetched in one batched query instead
  of one lookup per path member.
