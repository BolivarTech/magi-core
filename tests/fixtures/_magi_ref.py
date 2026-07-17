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
MAGI_PATH = Path(
    os.environ.get(
        "MAGI_PATH", r"C:\Users\jbolivarg\Projects\PythonProjects\MAGI-Claude"
    )
)

AGENTS = ("melchior", "balthasar", "caspar")

# Declared local divergences from the pinned reference, applied to each
# reference blob by BOTH `extract_magi_ref_prompts.py` (before writing
# `src/prompts_md/`) and `gen_magi_ref_prompts.py` (before hashing into the
# fixture) — one definition, two consumers, so the local files and the
# fixture can never disagree about the divergence. The fixture stays a pure
# function of (reference, declared divergence): anti-drift preserved, and
# re-pinning is a single command with no manual restoration.
# Each entry: (old_bytes, new_bytes, expected_occurrences_per_prompt).
DIVERGENCES: list[tuple[bytes, bytes, int]] = [
    # F0 fabrication-echo hardening (2026-07-16): the worked example must not
    # carry an echo-fabricable `approve`. Matches Python MAGI v5.1.0+.
    (b'"verdict": "approve"', b'"verdict": "conditional"', 1),
]

# Comment block written into the fixture so a reader of the .sha256 file sees
# the divergence without opening the scripts. Keep in sync with DIVERGENCES.
DIVERGENCE_BLOCK = [
    "# Local divergence (F0, 2026-07-16): the worked example's verdict value is",
    '# "conditional" instead of the reference\'s "approve" — fabrication-echo',
    "# hardening (an echoed example must not fabricate a clean approve in the",
    "# adversarial seat). Matches Python MAGI v5.1.0+ prompts. Hashes below are",
    "# of the reference blobs with that single declared delta applied; the Rust",
    "# prompts_md files must match them byte-for-byte.",
]


def apply_divergences(blob: bytes, rel_path: str) -> bytes | None:
    """Apply every declared divergence to a reference blob, failing loudly.

    Returns the transformed blob, or ``None`` (after printing to stderr) if an
    occurrence count does not match — a miscount means the reference changed
    shape and ``DIVERGENCES`` must be re-audited, never silently skipped.
    """
    import sys

    for old, new, expected in DIVERGENCES:
        found = blob.count(old)
        if found != expected:
            print(
                f"error: {rel_path}: expected {expected} occurrence(s) of "
                f"{old!r}, found {found}. Re-audit DIVERGENCES before "
                f"regenerating (the reference prompt shape changed).",
                file=sys.stderr,
            )
            return None
        blob = blob.replace(old, new, expected)
    return blob


def read_blob(repo: Path, ref: str, rel_path: str) -> bytes:
    """Read a file's bytes at a specific ref via `git show`, no checkout."""
    result = subprocess.run(
        ["git", "-C", str(repo), "show", f"{ref}:{rel_path}"],
        check=True,
        capture_output=True,
    )
    return result.stdout
