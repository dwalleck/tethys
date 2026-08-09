# Raw request

Requester invocation:

> /ship tethys-7a6a use subagents for any research or out of band work

Rivets issue text (verbatim):

> Replace the separate forward and backward reachability methods with one Tethys
> operation parameterized by reachability direction. Execute reachability behind
> the Tethys seam using bulk graph traversal and predecessor reconstruction
> rather than querying once per visited symbol and cloning each partial path
> during traversal.
>
> Preserve the documented BFS discovery order, shortest-depth uniqueness, path
> invariants, depth behavior, and termination on cyclic call graphs. The CLI
> continues to accept forward and backward directions and maps them to the
> unified operation.
>
> Acceptance criteria:
>
> - One direction-parameterized Tethys operation replaces the forward/backward method pair.
> - Forward direction follows callees and backward direction follows callers.
> - Each reachable symbol appears once at its shortest depth.
> - Every path excludes the source, includes the target, and has length equal to depth.
> - Results preserve BFS discovery ordering rather than global alphabetical ordering.
> - Cyclic call graphs terminate and do not return the source as reachable from itself.
> - Depth zero, one, omitted/default, finite, and oversized values follow the shared traversal-depth contract.
> - Traversal does not issue one database query per visited symbol or clone every growing path during search.
> - CLI and Tethys integration tests cover both directions and path invariants.
