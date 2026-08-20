// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-19

//! Captured backend responses, one `include_str!` per artifact.
//!
//! These are verbatim captures from a live Ollama daemon, copied out of a
//! gitignored working directory so the suite can exercise both wire formats
//! from a clean clone with no backend. Nothing here is hand-written: a
//! synthetic response presented as a capture would assert that a backend
//! behaves a way nobody observed.
//!
//! # Which tests use this module, and which cannot
//!
//! `include_str!` resolves relative to the file that writes it, so a `mod
//! tests` inside `src/` reaches these artifacts by relative path — for example
//! `include_str!("../../tests/fixtures/ec/native-N1.json")` from
//! `src/providers/`. **`tests/` is not a module of the crate**, so `src/`
//! cannot import this file; it serves the integration tests, and the unit
//! tests use the relative path. Saying so here is cheaper than having someone
//! discover it and "fix" it by making `pub(crate)` types public.
//!
//! # The shapes, read off the artifacts rather than remembered
//!
//! | constant | format | terminator | notes |
//! |---|---|---|---|
//! | `FIX_N1` | native | `stop` | no `message.thinking`; `eval_count` 6800 |
//! | `FIX_N2` | native | `length` | **truncated**, with `message.thinking` |
//! | `FIX_LOAD` | native | `load` | empty content, **counters ABSENT** |
//! | `FIX_E_404` | native | — | HTTP 404 body, single `error` key |
//! | `FIX_LENGTH` | compat | `length` | **empty content** — the EC defect itself |
//! | `FIX_REASONING` | compat | `stop` | `message.reasoning` present, content 1898 chars |
//! | `FIX_COLD_START` | native | `stop` | a just-unloaded model answering normally |
//! | `FIX_UNLOAD` | native | `unload` | empty content, counters absent — see below |
//!
//! **`FIX_N2` is a TRUNCATED success, not a plain one.** The plan called it
//! "native success, with thinking"; its `done_reason` is `length`. The
//! distinction matters because a test reaching for "a native success carrying
//! thinking" would be reaching for a response that also hit its cap.
//!
//! **`FIX_UNLOAD` exists to keep a guard honest.** It satisfies two of the
//! three conditions that make a response a defect of this crate — counters
//! absent and content empty — and only `done_reason` separates it from
//! `FIX_LOAD`. Anyone tempted to key the crate-defect fingerprint on two
//! conditions instead of three should make it fail against this artifact
//! first.
//!
//! There is deliberately **no** fixture for "a compat response WITHOUT
//! `message.reasoning`": no such capture exists, every compat artifact carries
//! the field, and tolerating an absent field is a property of OUR
//! deserialiser rather than of any backend. Its contract belongs in a JSON
//! literal inside the test that asserts it.

/// Native success, terminated by `stop`, without `message.thinking`.
pub const FIX_N1: &str = include_str!("native-N1.json");

/// Native response terminated by `length` — truncated — WITH `message.thinking`.
pub const FIX_N2: &str = include_str!("native-N2.json");

/// Native `200` with `done_reason: "load"`, empty content and token counters
/// **absent**: the fingerprint that marks a malformed request of ours.
pub const FIX_LOAD: &str = include_str!("native-E-malformed.json");

/// Native error body for a model the backend does not hold (HTTP 404).
pub const FIX_E_404: &str = include_str!("native-E-model-not-found.json");

/// Compatibility format, `finish_reason: "length"` with EMPTY content — the
/// response shape this milestone exists to classify correctly.
pub const FIX_LENGTH: &str = include_str!("resp-C.json");

/// Compatibility format, `finish_reason: "stop"`, carrying `message.reasoning`.
pub const FIX_REASONING: &str = include_str!("resp-H.json");

/// Native success from a model that had just been unloaded — a genuinely cold
/// start, answering normally. Evidence that a cold start does NOT produce the
/// crate-defect fingerprint.
pub const FIX_COLD_START: &str = include_str!("native-cold-start.json");

/// Native response to `keep_alive: 0` with empty `messages`: `done_reason:
/// "unload"`, empty content, counters absent. Two of the three fingerprint
/// conditions, and the reason the third one is not decorative.
pub const FIX_UNLOAD: &str = include_str!("native-unload-empty-messages.json");
