# Review decisions

| finding-id | finding | reviewer | evidence-state | evidence | decision | fix | note |
|---|---|---|---|---|---|---|---|
| F1 | Ordinary incompatible-schema open enables WAL before checking currency; check before persistent pragmas. | RevisionReview | Verified | DELETE-mode byte/sidecar fence failed before the fix (artifact 106), passed afterward; final full suite and CLI smoke passed. | Accept | S1: unconfigured open, then currency check, then WAL configuration; explicit rebuild configures independently. | S1 checkpoint PASS: 1131 nextest tests and 18 doctests passed; batch/stream 15,124-file rollback/reindex/rebuild smoke passed. Reviewer confirmed the repair. Final integration N/A — later plan slices remain. |
