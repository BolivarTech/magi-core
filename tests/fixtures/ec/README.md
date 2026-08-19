# EC fixtures — captured responses from Ollama's native `/api/chat`

Real responses, captured once against a live daemon, kept so the test suite can exercise the
native wire format without a backend. Every file here is verbatim output; nothing is hand-edited.

## Task 0 — the cold-start capture: **GO**

**Question it answered.** `MagiError::CrateDefect` aborts a run when a response carries a specific
fingerprint: token counters **absent** (not zero), `done_reason: "load"`, and empty content. That
fingerprint was inferred from a single observation — a `POST /api/chat` with no `messages` — and the
open worry was that a **transient cold start** produces the same shape, in which case an abort
would fire on a healthy backend recovering from an unload.

**Result: it does not.** A genuinely cold request (14.5 s of `load_duration`) answers normally.

| capture | `done_reason` | content | `eval_count` / `prompt_eval_count` |
|---|---|---|---|
| `native-cold-start.json` | `stop` | non-empty | **17 / 82 — present** |
| `native-unload-empty-messages.json` | `unload` | empty | **absent** |

So the fingerprint is **not** produced by a cold start, and the abort path stands as designed.

**Method.** The configured trio is all `:cloud` tags, and `keep_alive: 0` unloads from the **local**
daemon, where a cloud tag does not live — so the experiment ran against a locally available model,
`gemma4:12b`, as the plan prescribes. What is measured is the daemon's behaviour on a cold model,
not that model in particular.

## Two findings the experiment was not looking for

**1. A fourth `done_reason` value: `unload`.** The known set was `stop`, `length` and `load`. This is
the second time the observed space has grown, which is why the finish-reason type is
`#[non_exhaustive]` with a textual escape rather than a closed enum: a closed one would have broken
here, and a bare `String` would push callers into comparing text.

**2. The fingerprint's third condition is load-bearing, not decorative.** The `unload` response
satisfies **two of the three** conditions — counters absent, content empty — and only `done_reason`
separates it from a crate defect. Had the fingerprint been "absent counters and empty content", a
caller passing `keep_alive: 0` would have been classified as a defect of this crate and **aborted
the run**. Anyone tempted to relax the trigger to two conditions should read this paragraph first.

## Files

| file | what it is |
|---|---|
| `native-cold-start.json` | `POST /api/chat`, well-formed, against a model just unloaded |
| `native-unload-empty-messages.json` | `POST /api/chat` with `keep_alive: 0` and `messages: []` |
