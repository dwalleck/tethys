# tethys-aay4 — tracker prior art (prove-it-prototype step 0)

- **tethys-j2r1** (open, P3) — type hierarchy; BLOCKED BY this issue (its
  method-to-method override walk needs parent linkage). The reason aay4 is
  being shipped now.
- **tethys-dl7l** (P2, FILED BY THIS PROBE) — find_impl_type records the
  TRAIT as parent/qualified prefix for trait-impl methods; the refs side
  (impl_type_base_name, tethys-53iv) uses the correct 'type' field. aay4
  must fix this first or parent_symbol_id inherits trait-as-parent.
- **tethys-53iv** (closed) — receiver-typed method resolution; its
  impl_type_base_name is the correct-side precedent for dl7l's fix.
- **tethys-0nar** (open) — macro-defined symbols not indexed; their
  parents are out of any universe until then.
- **tethys-mpth** (open, P4) — ModulePath newtype / module structure;
  adjacent but not gating (parent here is symbol-level, not module-level).
- **tethys-xov3** (closed) — C# nested type extraction; C# members already
  carry parent_name (extract_data_member/extract_method).
