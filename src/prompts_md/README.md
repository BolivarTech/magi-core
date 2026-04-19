# `src/prompts_md/` — Embedded prompt data

The three `.md` files here (`melchior.md`, `balthasar.md`, `caspar.md`) are
**byte-for-byte copies** of the Python MAGI reference at
`MAGI@v2.1.3/skills/magi/agents/*.md`. They are embedded into the crate at
compile time via `include_str!` in `src/prompts.rs`.

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

If `MAGI@v2.1.3/skills/magi/agents/*.md` changes upstream:

1. Bump `MAGI_REF_SHA` in `tests/fixtures/gen_magi_ref_prompts.py`.
2. Re-extract the three files using `git show` (Task 02 step 1).
3. Run `python tests/fixtures/gen_magi_ref_prompts.py` to regenerate the
   hash fixture.
4. Commit as `chore: bump MAGI reference prompts to <new-sha>`.
