# Migration guide: magi-core 0.4.x → 0.5.0

## Summary

v0.5.0 adds **caller-supplied complexity gate** for cost control + closes
the per-variant breaking-change pattern by marking `MagiError` as
`#[non_exhaustive]`.

1. New `MagiBuilder::with_complexity_gate(F)` — predicate
   `Fn(&str, &Mode) -> bool + Send + Sync + 'static` runs after
   `max_input_len` validation but before LLM dispatch. When it returns
   `false`, `analyze` short-circuits with
   `MagiError::SkippedByComplexityGate` and **zero LLM calls are made**.
2. New `MagiError::SkippedByComplexityGate { reason: String }` variant
   (marked `#[non_exhaustive]` itself, so future structured fields are
   non-breaking).
3. `MagiError` is now `#[non_exhaustive]`. **All future variants will be
   non-breaking** — but you must add a `_ => ...` arm to existing
   exhaustive matchers (one-time cost).

## API compatibility

- **Additive for callers that don't pattern-match `MagiError`
  exhaustively.** Calling `analyze` without setting a gate preserves
  v0.4.x behavior exactly — verified by a dedicated test using the
  `Magi::new` default-builder path.
- **Breaking for callers that match `MagiError` exhaustively without
  a catch-all.** You will get a compile error like:

  ```
  error[E0004]: non-exhaustive patterns: `_` not covered
  ```

  Fix: add a `_ => ...` arm. The library will not add new variants
  silently from v0.6 onwards thanks to the `#[non_exhaustive]` attribute
  added in this release.

## Common upgrade pattern

```rust
// v0.4.x (no longer compiles in v0.5.x)
match err {
    MagiError::Validation(s) => log::warn!("validation: {s}"),
    MagiError::Provider(e) => log::error!("provider: {e}"),
    MagiError::InsufficientAgents { succeeded, required } => {
        log::error!("only {succeeded}/{required} agents responded");
    }
    MagiError::Deserialization(s) => log::warn!("deserialization: {s}"),
    MagiError::InputTooLarge { size, max } => {
        log::error!("input {size}B exceeds {max}B");
    }
    MagiError::InvalidInput { reason } => log::warn!("invalid input: {reason}"),
    MagiError::Io(e) => log::error!("io: {e}"),
}

// v0.5.x (compiles)
match err {
    MagiError::Validation(s) => log::warn!("validation: {s}"),
    MagiError::Provider(e) => log::error!("provider: {e}"),
    MagiError::InsufficientAgents { succeeded, required } => {
        log::error!("only {succeeded}/{required} agents responded");
    }
    MagiError::Deserialization(s) => log::warn!("deserialization: {s}"),
    MagiError::InputTooLarge { size, max } => {
        log::error!("input {size}B exceeds {max}B");
    }
    MagiError::InvalidInput { reason } => log::warn!("invalid input: {reason}"),
    MagiError::Io(e) => log::error!("io: {e}"),
    // NEW in v0.5.x
    MagiError::SkippedByComplexityGate { reason, .. } => {
        log::info!("complexity gate skip: {reason}");
    }
    // catch-all required by #[non_exhaustive]
    _ => log::error!("unknown MagiError variant: {err:?}"),
}
```

The `..` rest pattern on `SkippedByComplexityGate` is required because
the variant is itself `#[non_exhaustive]` — future fields like
structured `content_len` or `mode` can be added without breaking your
match.

## Behavior changes

**None for default usage.** If you do not call `with_complexity_gate`,
`analyze` behaves identically to v0.4.x.

## When to use the complexity gate

The gate is **opt-in cost control**. Use it when:

- You call `analyze` in a loop and many inputs are obviously trivial
  (single function body, empty file, etc.).
- You want a per-tenant rate limit on Claude usage.
- You have a cheap pre-classifier (length heuristic, regex sniffer)
  that can reject the input faster than 3× Claude opus calls.

Do NOT use the gate when:

- You want different ANALYSIS QUALITY for trivial vs complex inputs
  (the gate is binary — accept or skip; consider a different model
  alias via `default_model_for_mode` instead).
- The predicate would itself need to call an LLM. Use a cheap model
  (haiku) via a `pollster::block_on` wrapper, but understand this
  blocks the executor for the duration.

## Performance impact

- **No gate set (default):** zero overhead, identical to v0.4.x.
- **Gate returns `true`:** the predicate cost is added to `analyze`'s
  critical path. Microseconds typically; the rest of `analyze`
  dominates.
- **Gate returns `false`:** `analyze` returns immediately after the
  predicate. No agent factory, no nonce, no LLM calls. The cost is the
  predicate plus one `format!` allocation for the synthesized
  `reason`.

## Predicate contract checklist

- Signature: `Fn(&str, &Mode) -> bool + Send + Sync + 'static`.
- **Must be cheap** — microseconds, not milliseconds. Runs
  synchronously on the calling task's executor.
- **Must not panic.** Predicate panics propagate uncaught through
  `analyze`. Wrap defensively if inputs are adversarial.
- **`Mode` is by reference.** v0.5.0 chose `&Mode` over `Mode`
  (by value) to future-proof against `Mode` growing non-`Copy`
  variants. Today `Mode: Copy`, so the choice has zero runtime cost.

## Reason field format

The `reason` string in `SkippedByComplexityGate` is **library-synthesized**,
not caller-supplied. Current format:

```
complexity gate rejected: mode={mode}, content_len={N}
```

where `content_len` is the BYTE length (not UTF-8 char count) and
`{mode}` is the Display form (e.g., `code-review`, `design`, `analysis`).

**This format is NOT part of the SemVer contract.** Future versions
may add diagnostic context or restructure the string. For structured
logging, count occurrences of the `SkippedByComplexityGate` variant
itself; do not parse the reason. Future versions may expose
`content_len` / `mode` as structured fields on this variant (the
`#[non_exhaustive]` attribute on the variant permits this).

## Verification

```bash
cargo nextest run --features test-utils
cargo clippy --tests --all-features -- -D warnings
cargo fmt --check
cargo doc --no-deps --all-features
cargo audit
```

All commands must pass clean.

## Test count

`cargo nextest run --features test-utils` runs **377 tests** (up from
370 in v0.4.0). 7 new tests cover the gate's allow/block paths, the
content+mode propagation, the default no-gate v0.4.x backward compat,
the stateful rate-limiter use case, the synthesized reason format,
and the validate-first invariant.
