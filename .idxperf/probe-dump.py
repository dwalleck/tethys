#!/usr/bin/env python3
"""Canonical dump of a tethys index database (idxperf probe + oracle tool).

Emits one line per row of every table, sorted, with integer ids replaced by
natural keys and volatile columns (indexed_at, mtime_ns) excluded. Two
semantically identical indexes must produce byte-identical output regardless
of rowid assignment or insertion order. Duplicates are preserved.
"""
import sqlite3
import sys


def esc(v):
    """Escape newlines and pipes in free-text fields so each row is one line."""
    if v is None:
        return ""
    return str(v).replace("\\", "\\\\").replace("\n", "\\n").replace("|", "\\p")


def natkey_maps(c):
    files = {r[0]: r[1] for r in c.execute("SELECT id, path FROM files")}
    syms = {}
    for sid, fid, line, col, name in c.execute(
        "SELECT id, file_id, line, column, name FROM symbols"
    ):
        syms[sid] = f"{files.get(fid, '?')}:{line}:{col}:{name}"
    return files, syms


def main(db_path):
    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    c = conn.cursor()
    files, syms = natkey_maps(c)
    out = []

    for fid, path in files.items():
        lang, size = c.execute(
            "SELECT language, size_bytes FROM files WHERE id = ?", (fid,)
        ).fetchone()
        out.append(f"file|{path}|{lang}|{size}")

    for row in c.execute(
        "SELECT id, file_id, name, module_path, qualified_name, kind, line, column,"
        " end_line, end_column, signature, visibility, parent_symbol_id, is_test"
        " FROM symbols"
    ):
        (sid, fid, name, mp, qn, kind, line, col, el, ec, sig, vis, parent, test) = row
        parent_k = syms.get(parent, "") if parent is not None else ""
        out.append(
            f"sym|{files.get(fid, '?')}|{line}|{col}|{name}|{mp}|{qn}|{kind}"
            f"|{el}|{ec}|{esc(sig)}|{vis}|{parent_k}|{test}"
        )

    for row in c.execute(
        "SELECT file_id, kind, line, column, symbol_id, in_symbol_id, reference_name"
        " FROM refs"
    ):
        fid, kind, line, col, sid, in_sid, rname = row
        target = syms.get(sid, f"DANGLING:{sid}") if sid is not None else ""
        in_k = syms.get(in_sid, f"DANGLING:{in_sid}") if in_sid is not None else ""
        out.append(
            f"ref|{files.get(fid, '?')}|{line}|{col}|{kind}|{rname or ''}|{target}|{in_k}"
        )

    for fid, sname, smod, alias in c.execute(
        "SELECT file_id, symbol_name, source_module, alias FROM imports"
    ):
        out.append(f"imp|{files.get(fid, '?')}|{sname}|{smod}|{alias or ''}")

    for f, t, n in c.execute(
        "SELECT from_file_id, to_file_id, ref_count FROM file_deps"
    ):
        out.append(f"dep|{files.get(f, '?')}|{files.get(t, '?')}|{n}")

    for caller, callee, n in c.execute(
        "SELECT caller_symbol_id, callee_symbol_id, call_count FROM call_edges"
    ):
        out.append(f"edge|{syms.get(caller, '?')}|{syms.get(callee, '?')}|{n}")

    for sid, name, args, line in c.execute(
        "SELECT symbol_id, name, args, line FROM attributes"
    ):
        out.append(f"attr|{syms.get(sid, '?')}|{name}|{esc(args)}|{line}")

    pkgs = {r[0]: r[1] for r in c.execute("SELECT id, name FROM arch_packages")}
    for pid, name in pkgs.items():
        path, source = c.execute(
            "SELECT path, source FROM arch_packages WHERE id = ?", (pid,)
        ).fetchone()
        out.append(f"pkg|{name}|{path}|{source}")
    for fid, pid in c.execute("SELECT file_id, package_id FROM arch_file_packages"):
        out.append(f"fpkg|{files.get(fid, '?')}|{pkgs.get(pid, '?')}")
    for s, t, n in c.execute(
        "SELECT source_pkg, target_pkg, dep_count FROM arch_package_deps"
    ):
        out.append(f"pdep|{pkgs.get(s, '?')}|{pkgs.get(t, '?')}|{n}")

    out.sort()
    sys.stdout.write("\n".join(out) + "\n")


if __name__ == "__main__":
    main(sys.argv[1])
