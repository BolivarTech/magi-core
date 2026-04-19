#!/usr/bin/env python3
"""Generate SHA-256 hashes of MAGI Python reference prompts.

Uses `git show <ref>:<path>` to read blob contents without mutating the
reference repo. Re-run only when MAGI_REF_SHA bumps. Output is committed.

Usage:
    python tests/fixtures/gen_magi_ref_prompts.py
"""
from __future__ import annotations

import hashlib
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

MAGI_PATH = Path(os.environ.get("MAGI_PATH", r"D:\jbolivarg\PythonProjects\MAGI"))
# Pinned to commit SHA (MAGI R2 I4) — tags may move, commits don't.
# This is the release commit for Python MAGI v2.1.3.
MAGI_REF_SHA = "668f0e5e8ba4cf6851c4dcf77e727d0174e7ca30"
AGENTS = ("melchior", "balthasar", "caspar")
OUT = Path(__file__).parent / "magi_ref_prompts.sha256"


def read_blob(repo: Path, ref: str, rel_path: str) -> bytes:
    """Read a file's bytes at a specific ref via `git show`, no checkout."""
    result = subprocess.run(
        ["git", "-C", str(repo), "show", f"{ref}:{rel_path}"],
        check=True,
        capture_output=True,
    )
    return result.stdout


def main() -> int:
    if not MAGI_PATH.is_dir():
        print(f"error: MAGI_PATH does not exist: {MAGI_PATH}", file=sys.stderr)
        return 1

    today = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    lines = [f"# Generated from MAGI@{MAGI_REF_SHA} on {today}"]
    for agent in AGENTS:
        rel_path = f"skills/magi/agents/{agent}.md"
        try:
            blob = read_blob(MAGI_PATH, MAGI_REF_SHA, rel_path)
        except subprocess.CalledProcessError as e:
            print(
                f"error reading {rel_path} at {MAGI_REF_SHA}: {e.stderr.decode()}",
                file=sys.stderr,
            )
            return 1
        digest = hashlib.sha256(blob).hexdigest()
        lines.append(f"{digest}  {agent}.md")

    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {OUT} ({len(AGENTS)} prompts, {MAGI_REF_SHA})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
