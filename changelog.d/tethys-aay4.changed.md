- Symbols now record their container: methods link to their type, struct
  fields to their struct, enum variants to their enum, C# members to their
  (innermost) class — the groundwork for type-hierarchy queries.
- Methods in `impl Trait for Type` blocks are now qualified by the
  implementing type (`Type::method`); they were previously misfiled under
  the trait's name, so `callers`/`search` lookups by the real type missed
  them and calls through typed receivers could fail to resolve.
- `untested-code` gains accuracy from the same fix: type-qualified
  constructor calls (for example `FileId::from(...)`) now resolve, so those
  methods no longer read as untested.
