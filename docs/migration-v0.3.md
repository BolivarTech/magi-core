# Migration Guide: magi-core 0.2.x → 0.3.0

v0.3.0 completes Python-MAGI parity by consolidating the prompt
architecture and hardening the `user_prompt` against injection. Two API
changes require consumer action; everything else is transparent.

## 1. `with_custom_prompt` → `with_custom_prompt_for_mode`

**Before (v0.2.x):**

```rust
let magi = MagiBuilder::new(provider)
    .with_custom_prompt(AgentName::Melchior, Mode::CodeReview, "...".into())
    .build();
```

**After (v0.3.0):**

```rust
let magi = MagiBuilder::new(provider)
    .with_custom_prompt_for_mode(AgentName::Melchior, Mode::CodeReview, "...".into())
    .build();
```

The legacy method is retained with `#[deprecated]` through v0.3.x. It
emits a compile-time warning but continues to work identically. Migrate
at your convenience; the shim is removed in v0.4.0.

## 2. New: `with_custom_prompt_all_modes` for mode-agnostic overrides

```rust
let magi = MagiBuilder::new(provider)
    .with_custom_prompt_all_modes(AgentName::Caspar, "custom for all modes".into())
    .build();
```

Lookup order (see `sbtdd/spec-behavior.md` §4.4):

1. Per-mode override (`with_custom_prompt_for_mode`).
2. Mode-agnostic override (`with_custom_prompt_all_modes`).
3. Embedded default from `src/prompts_md/*.md`.

## 3. Prompt directory layout changed

v0.2.x had 9 files under `src/prompts_md/` named `{agent}_{mode}.md`.
v0.3.0 has 3 files: `{agent}.md`, byte-for-byte copies of Python MAGI
v2.1.3. Consumers that read these files directly (e.g., for testing)
must update paths:

| v0.2.x path | v0.3.0 path |
|---|---|
| `src/prompts_md/melchior_code_review.md` | `src/prompts_md/melchior.md` |
| `src/prompts_md/melchior_design.md` | `src/prompts_md/melchior.md` |
| `src/prompts_md/melchior_analysis.md` | `src/prompts_md/melchior.md` |
| `src/prompts_md/balthasar_code_review.md` | `src/prompts_md/balthasar.md` |
| `src/prompts_md/balthasar_design.md` | `src/prompts_md/balthasar.md` |
| `src/prompts_md/balthasar_analysis.md` | `src/prompts_md/balthasar.md` |
| `src/prompts_md/caspar_code_review.md` | `src/prompts_md/caspar.md` |
| `src/prompts_md/caspar_design.md` | `src/prompts_md/caspar.md` |
| `src/prompts_md/caspar_analysis.md` | `src/prompts_md/caspar.md` |

## 4. `user_prompt` format changed (affects mock-based tests)

If your tests use a mock `LlmProvider` that captures the `user_prompt`
and asserts on its content, update the assertions. The new format is:

```
MODE: <mode>
---BEGIN USER CONTEXT <32-hex-nonce>---
<sanitized content>
---END USER CONTEXT <32-hex-nonce>---
```

Key changes for assertion code:

- The nonce is random per call — match with regex `^[0-9a-f]{32}$` or
  compare on structure, not literal string.
- `content` is sanitized before embedding via a fixed 3-layer pipeline:
  1. `normalize_newlines` — maps all Unicode line separators to `\n`.
  2. `strip_invisibles` — removes zero-width and invisible Unicode chars.
  3. `neutralize_headers` — prefixes lines starting with `MODE:`,
     `CONTEXT:`, `---BEGIN`, `---END` with two spaces.
- Pipeline order is fixed (normalize → strip → neutralize). Reversing
  enables bypass; see `docs/adr/001-prompt-injection-threat-model.md`.

For byte-exact assertions: `with_rng_source` is currently `pub(crate)`.
Test your integrations against structure, not exact nonce values.

## 5. `Agent::new` no longer takes `Mode`

Direct constructions of `Agent` (uncommon — typically `Magi` does this
internally):

**Before:**

```rust
let agent = Agent::new(AgentName::Melchior, Mode::CodeReview, provider);
```

**After:**

```rust
let agent = Agent::new(AgentName::Melchior, provider);
```

The system prompt is resolved by the orchestrator via `lookup_prompt`
and passed to `Agent::execute` directly.

## 6. New error variant: `MagiError::InvalidInput { reason }`

Returned when `build_user_prompt` detects that the sanitized content
contains the generated nonce (fail-closed; probability ~2^-64 effective
given `fastrand` state size). In practice unreachable; add a catch-all
branch in your error handling if exhaustive matching is required:

```rust
match magi.analyze(&mode, content).await {
    Ok(report) => { /* ... */ }
    Err(MagiError::InvalidInput { reason }) => {
        eprintln!("prompt construction failed: {reason}");
        // retry or propagate
    }
    Err(other) => { /* ... */ }
}
```

`MagiError` is `#[non_exhaustive]`, so exhaustive matches already
require a `_` arm — no additional change needed if you have one.

## 7. New dependencies

Transitively pulled in; no direct action required:

- `fastrand ~2` — non-cryptographic RNG for per-request nonce. Lighter
  footprint than `rand 0.8`, no `unsafe`, no transitive `getrandom`.
- `sha2 0.10` — dev-dependency only; used for fixture SHA-256
  verification. Not present in release builds.

## Security limitations (MAGI R3 W8)

The following limitations are **known and accepted** per the threat model
in `docs/adr/001-prompt-injection-threat-model.md` (Scope IS-NOT
section). Consumers with stricter threat models must apply additional
pre-filtering before calling `analyze`.

### Case-sensitivity

Header matching is **case-sensitive**. `mode:`, `Mode:`, and `MoDe:` are
NOT neutralized — only the exact uppercase tokens `MODE:`, `CONTEXT:`,
`---BEGIN`, and `---END` are matched. This preserves Python-MAGI
reference behavior. If your threat model requires case-insensitive
header neutralization, pre-filter input before passing to `analyze`.

### Non-ASCII whitespace (NBSP and similar)

U+00A0 (NBSP), U+3000 (Ideographic Space), and other non-ASCII
whitespace characters placed before a header token are NOT absorbed by
the `neutralize_headers` regex. A crafted input like
`\u{00A0}MODE: override` would not be neutralized.

`INVISIBLE_AND_SEPARATOR_RE` omits these characters (Python-parity).
Consumers must pre-filter if this gap is within their threat model.

### Nonce entropy ~64 bits (effective)

`fastrand`'s internal state is approximately 64 bits. The effective
nonce collision probability per call is therefore ~2^-64, not the
theoretical 2^-128 implied by the 128-bit representation. This is
acceptable per the ADR threat model (the adversary cannot observe or
predict the RNG state). An escape hatch `with_rng_source(Box<dyn
RngLike + Send>)` is available `pub(crate)` for test injection. Promote
to `pub` and switch to `getrandom` if cryptographic unpredictability is
required.

### Scope IS-NOT from ADR 001

The sanitization pipeline does NOT defend against:

- Semantic injection via natural-language manipulation ("ignore previous
  instructions...").
- LLM-specific jailbreaks (role-play, DAN, system-prompt extraction).
- Side-channel attacks (timing, token-count oracles).
- Exfiltration via the LLM's output.

These are application-layer concerns; callers are responsible for their
own defenses at those layers.

## Verification steps after upgrading

Run your test suite. Look for:

1. **`#[deprecated]` warnings** on `with_custom_prompt` call sites —
   harmless; migrate at leisure to `with_custom_prompt_for_mode`.
2. **Mock-based prompt assertion failures** due to the new
   `user_prompt` format — update assertions to match the new structure
   (MODE line + nonce delimiters + sanitized content).
3. **Direct reads of `src/prompts_md/*.md`** — update paths from
   `{agent}_{mode}.md` to `{agent}.md`.
4. **Exhaustive `MagiError` matches** — add a branch for
   `InvalidInput { reason }` if needed (or rely on the existing `_` arm
   since `MagiError` is `#[non_exhaustive]`).

No runtime behavior change for the common consumer path
`Magi::new(provider).analyze(mode, content)`.
