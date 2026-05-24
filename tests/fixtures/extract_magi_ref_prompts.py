#!/usr/bin/env python3
"""Extract MAGI Python reference prompts to src/prompts_md/.

Uses `git show <ref>:<path>` to read blobs without mutating the reference
repo. Writes raw bytes directly to avoid Windows CRLF conversion that
shell redirection (`>`) would introduce.

Usage:
    python tests/fixtures/extract_magi_ref_prompts.py
"""
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

MAGI_PATH = Path(os.environ.get("MAGI_PATH", r"D:\jbolivarg\PythonProjects\MAGI-Claude"))
MAGI_REF_SHA = "62cf58019aeab822cd55cbb02e1b8f34a3fd5d81"
AGENTS = ("melchior", "balthasar", "caspar")
DEST_DIR = Path(__file__).resolve().parents[2] / "src" / "prompts_md"


def read_blob(repo: Path, ref: str, rel_path: str) -> bytes:
    result = subprocess.run(
        ["git", "-C", str(repo), "show", f"{ref}:{rel_path}"],
        check=True,
        capture_output=True,
    )
    return result.stdout


def main() -> int:
    DEST_DIR.mkdir(parents=True, exist_ok=True)
    for agent in AGENTS:
        blob = read_blob(MAGI_PATH, MAGI_REF_SHA, f"skills/magi/agents/{agent}.md")
        # Normalize CRLF to LF in case `git show` emitted CRLF on Windows.
        blob = blob.replace(b"\r\n", b"\n")
        out = DEST_DIR / f"{agent}.md"
        out.write_bytes(blob)
        print(f"wrote {out} ({len(blob)} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
