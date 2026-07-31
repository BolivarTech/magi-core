# Redaction-gate fixtures

Every rule in `ci/check_redaction.sh` has a **pair** here, and the pair is what keeps the rule
alive. Without one, neutering that rule leaves `--self-test` reporting success — which is the
failure this whole mechanism exists to prevent, one level up.

## Adding a pair

Two files, `<name>_bad.rs` and `<name>_good.rs`, each starting with metadata lines:

```rust
// TARGET: providers/subject.rs      // where the harness drops this file
// EXPECT: raw error interpolation   // a substring of the message the rule must produce (bad only)
```

`TARGET` is relative to the fake source tree the harness builds, and must be the file the rule
under test actually scans — `providers/subject.rs`, `providers/provider_url.rs`, `error.rs` or
`orchestrator.rs`. A fixture placed where its rule does not look is never scanned, and the harness
will report it as rejected **by the wrong rule** rather than silently passing.

`EXPECT` is what makes the harness discriminating. Exit status alone proves nothing: any unrelated
check can reject a fixture while the rule under test stays dead.

## One pair per ALTERNATIVE, not per rule

A rule with several alternatives — pattern 1 accepts four interpolation syntaxes — needs one pair
each. With a single fixture, deleting three of the four alternatives still passes, and the one
deleted is the one that mattered: the tracing sigil, in a crate that already uses `tracing`.

## Verifying a change

Running `--self-test` is not enough. After changing a rule:

1. Neuter its `fail` call and confirm the self-test names **this** fixture.
2. Inject the defect the rule guards into the **real tree** and confirm it still fails.
3. Re-count `grep -c 'fail "'` to be sure the probe was undone.

Step 2 is what catches a rule that now passes its fixtures and misses the real thing — which has
happened, in the direction that reports success.
