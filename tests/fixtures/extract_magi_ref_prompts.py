#!/usr/bin/env python3
"""Extract MAGI Python reference prompts to src/prompts_md/.

Uses `git show <ref>:<path>` to read blobs without mutating the reference
repo. Writes raw bytes directly to avoid Windows CRLF conversion that
shell redirection (`>`) would introduce.

Usage:
    python tests/fixtures/extract_magi_ref_prompts.py
"""
from __future__ import annotations

import sys
from pathlib import Path

from _magi_ref import AGENTS, MAGI_PATH, MAGI_REF_SHA, apply_divergences, read_blob

# `MAGI_REF_SHA`/`MAGI_PATH`/`AGENTS`/`read_blob`/`apply_divergences` live in
# `_magi_ref.py` (single source of truth — re-pin there). The declared local
# divergences are applied here too, so the extracted files always match what
# `gen_magi_ref_prompts.py` hashes into the fixture.
DEST_DIR = Path(__file__).resolve().parents[2] / "src" / "prompts_md"


def main() -> int:
    DEST_DIR.mkdir(parents=True, exist_ok=True)
    for agent in AGENTS:
        rel_path = f"skills/magi/agents/{agent}.md"
        blob = read_blob(MAGI_PATH, MAGI_REF_SHA, rel_path)
        # Normalize CRLF to LF in case `git show` emitted CRLF on Windows.
        blob = blob.replace(b"\r\n", b"\n")
        blob = apply_divergences(blob, rel_path)
        if blob is None:
            return 1
        out = DEST_DIR / f"{agent}.md"
        out.write_bytes(blob)
        print(f"wrote {out} ({len(blob)} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
