# Raw request

Requester (dwalleck), verbatim, 2026-06-06:

> Lets do a gilfoyle loop on separator fix

Context the request points at (from preceding conversation, Claude's framing):

> Make the separator a `Language` method (`fn separator(&self) -> &str`) and thread it
> through `resolve.rs` — this fixes the live C# qualified-ref bug and is the forcing
> function that de-Rusts the resolution chain.

Bug claim being referenced: `resolve.rs` Pass-2 cross-file resolution hardcodes `"::"`
as the qualified-name separator (lines 168, 299, 310, 367, 379, 438), while C#
imports/qualified reference names are stored with `"."` (`batch_writer.rs:378-380`),
so C# qualified references never enter the qualified-resolution branch and silently
fall through to simple-name fallback search.
