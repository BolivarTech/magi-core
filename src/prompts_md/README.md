# `src/prompts_md/` — Embedded prompt data

The three `.md` files here (`melchior.md`, `balthasar.md`, `caspar.md`) are
**byte-for-byte copies** of the Python MAGI reference at
`MAGI@v3.0.0/skills/magi/agents/*.md` (commit `62cf58019aeab822cd55cbb02e1b8f34a3fd5d81`).
They are embedded into the crate at compile time via `include_str!` in
`src/prompts.rs`.

## Exemption from CLAUDE.local.md §0.2 file-header rule

CLAUDE.local.md §0.2 requires every new source file to begin with:

```
// Author: Julian Bolivar
// Version: 1.0.0
// Date: YYYY-MM-DD
```

The three prompt files in this directory are **exempt** from this rule.
Rationale:

1. They are **data**, not Rust source code.
2. RNF-04 in `sbtdd/spec-behavior.md` mandates byte-for-byte parity with
   the Python reference; any project header would break parity and change
   the embedded SHA-256 that `test_prompts_match_python_reference_sha256`
   verifies in CI.
3. Authorship of the prompt content belongs to the upstream Python MAGI
   project.

## Regeneration

When the upstream Python MAGI prompts change:

1. Bump `MAGI_REF_SHA` in both `tests/fixtures/gen_magi_ref_prompts.py`
   and `tests/fixtures/extract_magi_ref_prompts.py`.
2. Run `python tests/fixtures/extract_magi_ref_prompts.py` to re-extract
   the three files (writes raw bytes, normalizes CRLF→LF).
3. Run `python tests/fixtures/gen_magi_ref_prompts.py` to regenerate the
   hash fixture.
4. Commit the 6 files together as a dedicated re-pin commit (RE-04):
   `feat: re-pin agent prompts to MAGI v<version> (<summary>)`.
