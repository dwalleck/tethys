#!/usr/bin/env python3
"""C0: independently check the proposed SQLite publication protocol.

This is a design experiment, not proof that tethys implements the protocol.
The observer owns a separate connection and uses literal expected revisions.
"""
import argparse
import sqlite3
import tempfile
from pathlib import Path


def observe(connection):
    return connection.execute(
        "SELECT r.value, f.value, a.value FROM revision r, files f, architecture a"
    ).fetchone()


def run(mutation):
    with tempfile.TemporaryDirectory(prefix="tethys-82a6-c1-") as directory:
        path = Path(directory) / "index.db"
        writer = sqlite3.connect(path, isolation_level=None)
        observer = sqlite3.connect(path, isolation_level=None)
        contender = sqlite3.connect(path, isolation_level=None, timeout=0)
        try:
            writer.execute("PRAGMA journal_mode=WAL")
            for table in ("revision", "files", "architecture"):
                writer.execute(f"CREATE TABLE {table}(value INTEGER NOT NULL)")
                writer.execute(f"INSERT INTO {table} VALUES(1)")
            assert observe(observer) == (1, 1, 1), "C0 baseline observer failed"
            writer.execute("BEGIN IMMEDIATE")
            writer.execute("SAVEPOINT file_write")
            writer.execute("UPDATE files SET value=2")
            writer.execute("RELEASE file_write")
            if mutation:
                writer.execute("COMMIT")
                writer.execute("BEGIN IMMEDIATE")
            assert observe(observer) == (1, 1, 1), "C0 mixed revision visible before publish"
            try:
                contender.execute("BEGIN IMMEDIATE")
            except sqlite3.OperationalError as error:
                assert error.sqlite_errorcode == sqlite3.SQLITE_BUSY, (
                    "C0 contender failed for a reason other than writer exclusion"
                )
            else:
                contender.execute("ROLLBACK")
                raise AssertionError("C0 second writer entered during revision")
            writer.execute("SAVEPOINT architecture_write")
            writer.execute("UPDATE architecture SET value=2")
            writer.execute("RELEASE architecture_write")
            writer.execute("UPDATE revision SET value=2")
            # A fatal interruption rolls back released savepoints too.
            writer.execute("ROLLBACK")
            assert observe(observer) == (1, 1, 1), "C0 failure damaged published revision"
            # Positive control: the contender can write after the original releases.
            contender.execute("BEGIN IMMEDIATE")
            contender.execute("ROLLBACK")
            # Pin a reader before publication; it must see the old complete revision.
            observer.execute("BEGIN")
            assert observe(observer) == (1, 1, 1)
            writer.execute("BEGIN IMMEDIATE")
            for table in ("files", "architecture", "revision"):
                writer.execute(f"UPDATE {table} SET value=3")
            writer.execute("COMMIT")
            assert observe(observer) == (1, 1, 1), "C0 pinned reader changed revisions"
            observer.execute("COMMIT")
            assert observe(observer) == (3, 3, 3), "C0 positive publication was not observed"
            # Close/reopen is an independent visibility/durability observation.
            with sqlite3.connect(path) as reopened:
                assert observe(reopened) == (3, 3, 3), "C0 reopened revision is incomplete"
            print("C0 PASS: released savepoints remain unpublished; rollback preserves old rows;")
            print("writer exclusion and reader pinning hold; commit/reopen observes all new rows.")
            print(f"SQLite {sqlite3.sqlite_version}; protocol experiment, not the production revision fence.")
        finally:
            writer.close()
            observer.close()
            contender.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--mutate-early-commit", action="store_true")
    run(parser.parse_args().mutate_early_commit)
