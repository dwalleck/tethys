# Raw request

Requester (dwalleck), verbatim, 2026-06-07:

> Merge it, then start the usgf loop

Tracker issue tethys-usgf, verbatim body:

> Deferred from the jwf9 loop boundary (see .csharp-ns/spec.md decision #5).
> Plain `using Namespace;` directives resolve through CSharpModuleResolver;
> three forms still decline: (1) `using static Type;` — bare member
> (method/const) resolution; (2) `using Alias = Namespace.Type;` —
> alias-form resolution (Import.alias storage exists; extractor support to
> be verified); (3) C# 10 `global using` — cross-file propagation of
> usings, which the per-file resolution model cannot express today. Each
> form should get its own probe before design; (1) and (2) may surface
> parser gaps adjacent to tethys-itez/tethys-z45p.

The tracker bundles three distinct forms with three distinct mechanisms.
Decomposition trigger — this interrogation must pin WHICH form(s).
