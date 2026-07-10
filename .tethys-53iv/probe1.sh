#!/usr/bin/env bash
# probe1 (tethys-53iv): reproduce the ticket's exact repro on current main.
# Smallest question: does x.unwrap() on an Option bind to the in-crate
# Thing::unwrap (fabricated edge + caller over-attribution), and does
# panic-points then report 0 (false negative)? Raw SQL + CLI only.
set -euo pipefail
WS="$1"
TETHYS=(cargo run --quiet --manifest-path /home/dwalleck/repos/tethys/Cargo.toml --)

rm -rf "$WS" && mkdir -p "$WS/src"
printf '[package]\nname = "repro53iv"\nversion = "0.0.0"\nedition = "2021"\n' > "$WS/Cargo.toml"
cat > "$WS/src/lib.rs" <<'EOF'
pub struct Thing;
impl Thing {
    pub fn unwrap(&self) {}
}
pub fn use_external() {
    let x: Option<i32> = Some(1);
    x.unwrap();
}
pub fn use_internal() {
    let t = Thing;
    t.unwrap();
}
EOF

"${TETHYS[@]}" index -w "$WS" --rebuild >/dev/null
DB="$WS/.rivets/index/tethys.db"

echo "== A. all refs named unwrap (line, kind, strategy, bound target) =="
sqlite3 -header "$DB" "SELECT r.line, r.kind, COALESCE(r.strategy,'unresolved') AS strategy,
    COALESCE(ts.name || ' (' || ts.kind || ' @' || ts.line || ')', r.reference_name) AS target
  FROM refs r LEFT JOIN symbols ts ON ts.id = r.symbol_id
  WHERE ts.name = 'unwrap' OR r.reference_name LIKE '%unwrap%'
  ORDER BY r.line;"

echo "== B. call edges into Thing::unwrap (expect fabricated use_external edge) =="
sqlite3 -header "$DB" "SELECT cs.name AS caller, ts.name AS callee, ce.call_count
  FROM call_edges ce
  JOIN symbols cs ON cs.id = ce.caller_symbol_id
  JOIN symbols ts ON ts.id = ce.callee_symbol_id
  WHERE ts.name = 'unwrap' ORDER BY cs.name;"

echo "== C. panic-points (expect the x.unwrap() at src/lib.rs:7 or a false negative) =="
"${TETHYS[@]}" panic-points -w "$WS" 2>/dev/null | head -12

echo "== D. callers of Thing::unwrap (expect over-attribution) =="
"${TETHYS[@]}" callers Thing::unwrap -w "$WS" 2>/dev/null | head -12
