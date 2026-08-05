#!/usr/bin/env python3
"""Probe for tethys-09wx: per-changed-file query standing.

For each workspace-relative path given as an argument, classify:
  UNINDEXED - no row in the files table            -> indeterminate trigger (a)
  STALE     - disk mtime_ns/size differ from row   -> indeterminate trigger (b)
  CURRENT   - row matches disk                     -> index can stand behind it
This is the smallest slice of the feature's output: the standing of a single
changed file. No tethys code is reused; raw SQL + os.stat only.
"""
import os
import sqlite3
import sys

db = sqlite3.connect(".rivets/index/tethys.db")
for rel in sys.argv[1:]:
    row = db.execute(
        "SELECT mtime_ns, size_bytes FROM files WHERE path = ?", (rel,)
    ).fetchone()
    if row is None:
        print(f"{rel}\tUNINDEXED")
        continue
    mtime_ns, size = row
    try:
        st = os.stat(rel)
    except FileNotFoundError:
        print(f"{rel}\tSTALE\t(deleted on disk)")
        continue
    if st.st_mtime_ns != mtime_ns or st.st_size != size:
        print(f"{rel}\tSTALE\tdb=({mtime_ns},{size}) disk=({st.st_mtime_ns},{st.st_size})")
    else:
        print(f"{rel}\tCURRENT")
db.close()
