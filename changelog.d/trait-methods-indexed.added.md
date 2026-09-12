- Rust traits now contribute their declared methods to the index. `trait Repo { fn
  find(&self) -> Result<String, String>; }` produces a `find` method linked to
  `Repo`, alongside the trait symbol.
- Both declaration forms are covered: default methods with a body and required
  methods ending in `;`, which carry no body but do carry their return type.
- This brings Rust traits to parity with C# interfaces, whose members were
  already indexed.
