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
# commit for Python MAGI v5.3.0 — THE VERDICT SENTINEL. The prompts now carry
# the `<MAGI_VERDICT>` / `</MAGI_VERDICT>` marker block, the worked example
# moved OUTSIDE it, and the empty-slot placeholder took its place inside. The
# 7 top-level keys are unchanged.
#
# Delta from the previous pin (v3.0.0, 62cf5801) is 24+/4-, identical in all
# three files, and the 4 removed lines ARE the residual this milestone closes:
# "Respond with ONLY a JSON object...", "Example structure:", the inline
# fabricable 7-key example, and the old IMPORTANT block.
MAGI_REF_SHA = "9a762fa40b4ca366ce59127e68df160b87cae329"

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
# EMPTY since MS3 (2026-07-29): the reference itself now ships
# `"verdict": "conditional"` in the worked example, so the F0 divergence has
# nothing left to apply — its `expected_occurrences = 1` would find 0 and
# `apply_divergences` would fail loudly (verified: it did, before this entry was
# removed). The Rust prompts_md files are now the reference blobs VERBATIM, with
# no local delta at all.
DIVERGENCES: list[tuple[bytes, bytes, int]] = []

# Comment block written into the fixture so a reader of the .sha256 file sees
# the divergence without opening the scripts. Keep in sync with DIVERGENCES.
# Kept (not deleted) even with DIVERGENCES empty: a reader of the .sha256 file
# needs to be told EXPLICITLY that there is no local delta. Silence is ambiguous
# — it does not distinguish "no divergences" from "nobody documented the ones
# there are".
DIVERGENCE_BLOCK = [
    "# No local divergences (MS3, 2026-07-29): the hashes below are of the",
    "# pinned reference blobs VERBATIM — Python MAGI v5.3.0, the verdict",
    "# sentinel. The F0 divergence was retired because the reference itself now",
    '# ships "verdict": "conditional" in the worked example, so there was',
    "# nothing left to apply. The Rust prompts_md files must match these hashes",
    "# byte-for-byte.",
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
