#!/usr/bin/env bash
# Probe for tethys-epmj: observe the REAL runtime behavior of the
# package-not-found path, against the real repo and real binary.
# Oracle: .tethys-epmj/oracle-expectations.md (static source derivation,
# pre-registered before this script first ran).
set -u
cd "$(dirname "$0")/.." || exit 1
BIN=./target/debug/tethys

echo "== build =="
cargo build --quiet 2>&1 | tail -3
echo "build exit: $?"

echo "== index the real repo =="
$BIN index > /dev/null 2>&1
echo "index exit: $?"

run_case() {
    local label="$1"; shift
    echo ""
    echo "== $label =="
    local out err code
    out=$("$@" 2>/tmp/probe-err.$$)
    code=$?
    err=$(cat /tmp/probe-err.$$); rm -f /tmp/probe-err.$$
    echo "exit code: $code"
    echo "stdout: [${out}]"
    echo "stderr: [${err}]"
}

run_case "A: text mode, nonexistent package" \
    $BIN coupling --package zzz-definitely-not-a-package
run_case "A2: json mode, nonexistent package" \
    $BIN coupling --package zzz-definitely-not-a-package --json
run_case "A3: substring of real package (suggestion path)" \
    $BIN coupling --package eth
run_case "control: existing package succeeds" \
    $BIN coupling --package tethys

echo ""
echo "== B-probe: tethys AST index — references to NotFound =="
$BIN search NotFound --json 2>&1 | head -20
echo ""
echo "== B-oracle: grep pipeline — construction sites by payload prefix =="
grep -rn "Error::NotFound(format!" src/ \
    | grep -v "^src/.*tests" \
    | sed -E 's/.*format!\("([^"{]*).*/\1/' \
    | sort | uniq -c | sort -rn
echo "-- raw sites (for eyeballing multiline cases):"
grep -rn "Error::NotFound(" src/ --include="*.rs" | grep -v "mod tests" | grep -c "format!"
