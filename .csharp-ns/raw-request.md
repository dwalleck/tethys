# Raw request

Requester (dwalleck), verbatim, 2026-06-06:

> Merge it, then start the jwf9 loop

Tracker issue tethys-jwf9, verbatim body:

> resolve_import() in csharp.rs:109-111 returns empty vec, meaning C# using
> statements don't resolve to files.
>
> This was marked "Task 6" in the original TODO, suggesting it was planned.
> Need to implement namespace-to-file resolution for C# projects.

Note: the issue predates the ModuleResolver seam (merged in PR #1); the
csharp.rs:109-111 reference is likely stale — probe must verify what exists
there today. The declining stub now lives in
src/languages/module_resolver.rs::CSharpModuleResolver.
