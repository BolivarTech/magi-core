#!/usr/bin/env python3
"""Single source of truth for the MAGI reference prompt fixtures.

Holds the pinned MAGI commit (`MAGI_REF_SHA`), the reference repo path
(`MAGI_PATH`), the agent set, and the `git show` blob reader. Imported by
`gen_magi_ref_prompts.py` (hashes the blobs into `magi_ref_prompts.sha256`)
and `extract_magi_ref_prompts.py` (writes the blobs to `src/prompts_md/`).

**Re-pin here only** — bumping `MAGI_REF_SHA` in this one file keeps the two
scripts consistent (previously each carried its own copy, which could drift).
"""
from __future__ import annotations

import os
import subprocess
from pathlib import Path

# Pinned to a commit SHA (tags can move, commits don't). This is the release
# commit for Python MAGI v3.0.0 — finding calibration + structured
# file/line/category finding fields; the 7 top-level keys are unchanged.
MAGI_REF_SHA = "62cf58019aeab822cd55cbb02e1b8f34a3fd5d81"

# Reference repo checkout. Override with the MAGI_PATH env var.
MAGI_PATH = Path(os.environ.get("MAGI_PATH", r"D:\jbolivarg\PythonProjects\MAGI-Claude"))

AGENTS = ("melchior", "balthasar", "caspar")


def read_blob(repo: Path, ref: str, rel_path: str) -> bytes:
    """Read a file's bytes at a specific ref via `git show`, no checkout."""
    result = subprocess.run(
        ["git", "-C", str(repo), "show", f"{ref}:{rel_path}"],
        check=True,
        capture_output=True,
    )
    return result.stdout
