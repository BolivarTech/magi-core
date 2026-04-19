# ADR 001: Prompt-Injection Threat Model for magi-core v0.3.0

**Status:** Accepted
**Date:** 2026-04-19
**Related:** `sbtdd/spec-behavior.md` §5, §6, §11

## Context

`Magi::analyze(mode, content)` embeds `content` in a user_prompt sent to the
LLM. In v0.2.0 the prompt was constructed by a naive `format!` that did not
sanitize `content`. v0.3.0 introduces a structured `build_user_prompt`
pipeline with defense-in-depth against adversarial `content`.

## Threat Model

**Adversary capability:** controls the `content` argument to `analyze`. Goal:
subvert the analysis by injecting material the LLM interprets as system
instruction.

**Attack vectors:**

1. **MODE override.** Insert `\nMODE: design` in content while the consumer
   called `analyze(Mode::CodeReview, ...)`.
2. **Context delimiter spoof.** Insert a premature `---END USER CONTEXT ...---`
   followed by fake "system" instructions.
3. **Hidden-character smuggling.** Embed zero-width chars or bidi marks
   before/inside header tokens.
4. **Line-ending exploits.** Use non-LF line separators so regex-based
   neutralization misses injected headers. Includes `\r` (CR), U+0085 (NEL),
   U+000B (VT), U+000C (FF), U+2028 (LS), U+2029 (PS).
5. **Leading-whitespace bypass.** Prefix a header with space/tab so regex
   anchored at `^(MODE|...)` does not match.

**Out of scope:** semantic injection in natural language ("ignore previous
instructions..."). Structural defenses cannot detect intent; application-
layer filters are caller's responsibility.

## Defense-in-Depth (3 sanitization layers + 1 fail-closed layer)

1. **Layer 1 — normalize_newlines.** Map all Unicode line separators
   (`\r\n`, `\r`, U+000B, U+000C, U+0085, U+2028, U+2029) to `\n`. Closes
   attack vector 4.
2. **Layer 2 — strip invisibles.** Remove all characters in
   `INVISIBLE_AND_SEPARATOR_RE` (Python-parity set). Closes vector 3.
3. **Layer 3 — neutralize headers.** Regex
   `(?m)^([\t ]*)(MODE|CONTEXT|---BEGIN|---END)(\s|:|$)` matches
   header-starting lines after normalization, absorbing any leading
   ASCII whitespace. Substitute with `"$1  $2$3"`. Closes vectors 1, 2,
   and 5.
4. **Layer 4 — nonce per request.** 128-bit random value formatted
   `{:032x}` (32-char hex). Delimiters become
   `---BEGIN USER CONTEXT {nonce}---` and
   `---END USER CONTEXT {nonce}---`. If sanitized content contains the
   nonce literally, `build_user_prompt` fails closed with
   `MagiError::InvalidInput`. Closes vector 2 against dynamic spoofs.

**Order matters:** normalize → strip → neutralize. Reversing enables bypass
(see spec §5.2 for the detailed bypass catalog).

## Scope of Mitigation

**IS defended:**

- Literal injection of reserved header tokens (`MODE:`, `CONTEXT:`,
  `---BEGIN USER CONTEXT ...`, `---END USER CONTEXT ...`).
- Invisible-character smuggling before header tokens.
- Unicode line-separator exploits (all 7 common separators).
- Leading-whitespace evasion (ASCII space and tab).
- Static-delimiter spoof attacks (nonces are per-request).

**IS NOT defended:**

- Semantic injection via natural-language manipulation ("ignore...").
- LLM-specific jailbreaks (role-play, DAN, system-prompt extraction).
- Case-variant tokens: `mode:`, `Mode:`, `MoDe:` are NOT neutralized.
  Matches Python MAGI reference which is case-sensitive. Consumers with
  stricter threat models must pre-filter input.
- Non-ASCII whitespace before headers (e.g., U+00A0 NBSP, U+3000 IDEOGRAPHIC
  SPACE). Accepted gap: Python reference has the same limitation.
  `INVISIBLE_AND_SEPARATOR_RE` omits these characters; consumers must
  pre-filter if needed.
- Side-channel attacks (timing, token-count oracles).
- Exfiltration via the LLM's output. Callers must validate model responses.

## Rationale: treat `content` as untrusted by default

The library has no knowledge of whether `content` originated from a trusted
teammate's code review or from a public-facing web form. Treating all
inputs as untrusted is the safe default. Consumers with truly trusted
inputs pay a small constant cost (4 regex passes on typical ~1 KB content
is <1 ms).

## Alternatives Considered and Rejected

1. **Structured-output API (Anthropic tool-use / OpenAI functions).**
   Would let the LLM receive `content` as a typed parameter. Rejected:
   requires per-provider implementation; current `LlmProvider` trait is
   text-based; v0.3.0 scope is prompt architecture equivalence, not
   provider refactor. Revisit in v0.5+.
2. **Per-model content filters (cloud provider safety APIs).** Rejected:
   not portable across providers; does not address delimiter spoofing.
3. **Escaping / quoting `content` with delimiter-free format** (base64).
   Rejected: loses human readability in logs.
4. **Cryptographic RNG for the nonce (`getrandom`).** Rejected: the threat
   model does not require cryptographic unpredictability. `fastrand`
   (non-crypto) is sufficient and has lighter footprint.
5. **Case-insensitive header regex.** Rejected: breaks Python-parity
   (RNF-04 is scoped to prompt content, but the header grammar ancestry
   is Python's and we preserve its semantics for cross-impl consistency).

## Decision: Nonce RNG choice — `fastrand`

- **Size:** ~5x smaller than `rand 0.8` dependency tree.
- **No `unsafe` code.**
- **No transitive `getrandom`.**
- Sufficient for per-request uniqueness within the threat model.

`pub(crate) fn MagiBuilder::with_rng_source(Box<dyn RngLike + Send>)`
allows **internal** tests to inject a deterministic RNG for end-to-end
verification of the fail-closed branch. **External consumers cannot inject
a custom RNG in v0.3.0** — promotion to `pub` is deferred to v0.4+ if a
real consumer use-case emerges. For the threat model documented here
(non-cryptographic attacker, single-process deployments), `fastrand`'s
~64-bit effective entropy is sufficient. Consumers with stricter
requirements today should wrap `Magi::analyze` with their own
application-layer rate limits and filtering rather than rely on
cryptographic nonce unpredictability.

## Implementation References

- `src/user_prompt.rs::build_user_prompt` — defense pipeline.
- `src/user_prompt.rs::normalize_newlines` — Layer 1.
- `src/user_prompt.rs::strip_invisibles` — Layer 2.
- `src/user_prompt.rs::neutralize_headers` — Layer 3 regex + substitution.
- `spec-behavior.md` §5 — algorithmic specification.
- `spec-behavior.md` §9 BDD-01..BDD-14 — observable behaviors including
  BDD-08b (Unicode newline bypass) and BDD-08c (leading-whitespace bypass).
