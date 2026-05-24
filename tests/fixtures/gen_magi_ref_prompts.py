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

MAGI_PATH = Path(os.environ.get("MAGI_PATH", r"D:\jbolivarg\PythonProjects\MAGI-Claude"))
# Pinned to commit SHA (MAGI R2 I4) — tags may move, commits don't.
# This is the release commit for Python MAGI v3.0.0. The v3.0.0 prompt
# update added "Finding calibration" sections (per-mode guidance on
# finding severity thresholds) and structured file/line/category fields
# in the finding schema. The seven top-level keys are unchanged from v2.1.4.
# Pre-write SHA existence check (added v0.4.0, MAGI R1 W4) errors if the
# pinned commit is missing from the local MAGI checkout. See
# docs/migration-v0.4.md.
MAGI_REF_SHA = "62cf58019aeab822cd55cbb02e1b8f34a3fd5d81"
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


def verify_sha_exists(repo: Path, ref: str) -> bool:
    """MAGI R1 W4: pre-write check that the pinned SHA exists in the repo
    before regenerating the fixture. Avoids producing a stale fixture
    against a missing commit (would cause silent drift downstream)."""
    result = subprocess.run(
        ["git", "-C", str(repo), "cat-file", "-e", f"{ref}^{{commit}}"],
        capture_output=True,
    )
    return result.returncode == 0


def main() -> int:
    if not MAGI_PATH.is_dir():
        print(f"error: MAGI_PATH does not exist: {MAGI_PATH}", file=sys.stderr)
        return 1

    if not verify_sha_exists(MAGI_PATH, MAGI_REF_SHA):
        print(
            f"error: pinned SHA {MAGI_REF_SHA} does not exist in {MAGI_PATH}. "
            f"Run `git -C '{MAGI_PATH}' fetch --all` or update MAGI_REF_SHA "
            f"before regenerating the fixture.",
            file=sys.stderr,
        )
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
