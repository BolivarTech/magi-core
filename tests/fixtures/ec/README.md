# EC fixtures — captured responses from Ollama, both wire formats

Real responses, captured against a live daemon and copied here out of a gitignored working
directory so the suite can run from a clean clone with no backend. **Every file is verbatim
output; nothing is hand-written.** `origen: capturado` for all of them.

That distinction is the point. A synthetic response presented as a capture asserts that a backend
behaves a way nobody observed, and the milestone's own acceptance criterion asks for *captured*
responses: a JSON we wrote proves our deserialiser does what we intended, not that a backend
answers that way.

**How they are consumed, stated because it was once claimed wrongly.** Each file is pulled in
with `include_str!` from the unit tests that need it, by relative path from `src/`. There is no
fixture module: one existed, declaring a constant per artifact, and **nothing compiled it** --
`mod ec` appeared in no test target, so nine constants sat there looking like an index of a
corpus that was in fact read from somewhere else entirely. A file that certifies nothing while
looking like it does is the defect this whole directory exists to avoid, so it was deleted
rather than wired up.

Corollary, and it is a rule rather than an observation: **a capture with no reader is not a
guard**. Adding an artifact here means adding the test that reads it in the same change.

## Files

| file | format | terminator | what it carries |
|---|---|---|---|
| `native-N1.json` | native | `stop` | success, no `message.thinking`, `eval_count` 6800 |
| `native-N2.json` | native | `length` | **truncated**, WITH `message.thinking` |
| `native-E-malformed.json` | native | `load` | empty content, **counters ABSENT** |
| `native-E-model-not-found.json` | native | — | HTTP 404 body, single `error` key |
| `native-cold-start.json` | native | `stop` | a just-unloaded model answering normally |
| `native-unload-empty-messages.json` | native | `unload` | empty content, counters absent |
| `resp-C.json` | compat | `length` | **empty content** — the defect this milestone classifies |
| `resp-H.json` | compat | `stop` | `message.reasoning` present, content 1898 chars |
| `native-think-false-nonthinking-local.json` | native | `stop` | `think:false` on a model with NO thinking capability, **local** |
| `native-think-false-nonthinking-cloud.json` | native | `stop` | the same, **cloud-routed** |

`native-N2.json` is a **truncated** success rather than a plain one — its `done_reason` is
`length`. Worth stating, because a test reaching for "a native success carrying thinking" would
otherwise reach for a response that also hit its cap.

## The cold-start experiment: **GO**

**Question.** The crate-defect abort fires on a specific fingerprint: token counters **absent**
(not zero), `done_reason: "load"`, and empty content. That fingerprint came from a single
observation — a `POST /api/chat` with no `messages` — and the open worry was that a **transient
cold start** produces the same shape, which would abort a run on a healthy backend recovering from
an unload.

**Result: it does not.** A genuinely cold request (14.5 s of `load_duration`) answered with
`done_reason: "stop"`, real content, and both counters present. The abort path stands as designed,
now on measurement instead of inference.

**Method.** The configured trio is all `:cloud` tags, and `keep_alive: 0` unloads from the
**local** daemon where a cloud tag does not live — so the experiment ran against a locally
available model, `gemma4:12b`. What is measured is the daemon's behaviour on a cold model, not
that model in particular.

## Two findings the experiment was not looking for

**1. A fourth `done_reason`: `unload`.** The known set was `stop`, `length` and `load`. Second time
the observed space has grown, which is why the finish-reason type is `#[non_exhaustive]` with a
textual escape: a closed enum would have broken here, and a bare `String` would push callers into
comparing text.

**2. The fingerprint's third condition is load-bearing.** `native-unload-empty-messages.json`
satisfies **two of the three** conditions — counters absent, content empty — and only
`done_reason` separates it from a defect of this crate. Had the fingerprint been "absent counters
and empty content", a caller passing `keep_alive: 0` would have been read as a crate defect and
**aborted the run**. Anyone relaxing the trigger to two conditions should make it fail against
this artifact first.

## The two `think: false` captures, and what they are evidence FOR

They exist because a review round raised, as its only `[CRITICAL]`, that sending `think: false`
to a model without a thinking capability might be rejected — turning a control that is supposed
to DECLARE it cannot be honoured into a run-wide lineage condemnation. The objection was sound
and the remedy asked for was the right one: capture it rather than reason about it.

**The measurement refuted it.** Both return HTTP **200**, `done_reason: "stop"`, content present,
counters present and `message.thinking` absent — the field is accepted and simply has nothing to
do. Both deployment shapes were captured because this project has already been bitten by `/v1`
and native behaving differently, and cloud routing is not local routing.

They are kept as fixtures rather than as a note in a review log for the reason every other file
here is kept: a guarantee that depends on a live backend is a guarantee that stops being checked.
