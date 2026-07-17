# `src/prompts_md/` — Embedded prompt data

The three `.md` files here (`melchior.md`, `balthasar.md`, `caspar.md`) are
copies of the Python MAGI reference at
`MAGI@v3.0.0/skills/magi/agents/*.md` (commit `62cf58019aeab822cd55cbb02e1b8f34a3fd5d81`)
with **one documented local divergence** (see below). They are embedded into
the crate at compile time via `include_str!` in `src/prompts.rs`.

## Local divergence from the pinned reference (F0, 2026-07-16)

The worked example inside each prompt carries `"verdict": "conditional"`
instead of the reference's `"approve"` — fabrication-echo hardening: a model
that echoes the example verbatim must not fabricate a clean `approve` in the
adversarial seat (an echoed `conditional` surfaces as `GO WITH CAVEATS`,
visible). This matches the Python MAGI plugin's own prompts from v5.1.0
onward. The delta is declared once in `tests/fixtures/_magi_ref.py`
(`DIVERGENCES`), applied automatically by both the extractor and the hash
generator, and pinned by
`prompts::tests::test_worked_examples_do_not_ship_an_approve_verdict`.
Everything else remains byte-identical to the reference.

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
2. RNF-04 mandates byte-for-byte parity with
   the Python reference; any project header would break parity and change
   the embedded SHA-256 that `test_prompts_match_python_reference_sha256`
   verifies in CI.
3. Authorship of the prompt content belongs to the upstream Python MAGI
   project.

## Regeneration

When the upstream Python MAGI prompts change:

1. Bump `MAGI_REF_SHA` in `tests/fixtures/_magi_ref.py` (single source of
   truth — both scripts import it). If the new reference already carries a
   declared divergence (Python v5.1.0+ carries the `conditional` example),
   remove the corresponding `DIVERGENCES` entry there.
2. Run `python tests/fixtures/extract_magi_ref_prompts.py` to re-extract
   the three files (writes raw bytes, normalizes CRLF→LF, **applies the
   declared divergences automatically**).
3. Run `python tests/fixtures/gen_magi_ref_prompts.py` to regenerate the
   hash fixture (hashes reference-blobs-plus-divergences and emits the
   divergence comment block automatically — no manual fixture editing).
4. Both scripts fail loudly if a divergence's occurrence count no longer
   matches the reference (re-audit `DIVERGENCES` in `_magi_ref.py`); the
   property test `test_worked_examples_do_not_ship_an_approve_verdict`
   independently fails the build if an `approve` example slips through.
5. Commit the files together as a dedicated re-pin commit (RE-04):
   `feat: re-pin agent prompts to MAGI v<version> (<summary>)`.
