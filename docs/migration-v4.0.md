# Migrating from 3.2.0 to 4.0.0

`4.0.0` fixes one defect and everything that defect was hiding behind.

The short version: a reasoning model would spend its entire output budget thinking, return
`content: ""` with `finish_reason: "length"`, and the crate would turn that into a synthetic
HTTP error — which then condemned the model's lineage **run-wide**, taking it away from the other
two mages over what one mage had seen. The operator was told "transport", and went to look at a
network that had answered `HTTP 200` perfectly.

Fixing it properly meant reading the wire completely, naming contract failures as contract
failures, and giving rotation telemetry types that do not lie. That is a major, so the breaks are
spent all at once rather than saved up.

**Nine observable changes.** Each section says what it was, what it is, and what you do.

---

## 1. `complete()` returns `Completion`, not `String`

In one line: complete() returns Completion where it used to return String. That is **two edits
per implementation**, not one — the signature and the returned value — spelled out below.

**Before**

```rust
async fn complete(&self, system: &str, user: &str, config: &CompletionConfig)
    -> Result<String, ProviderError>
{
    Ok(self.call_my_backend(system, user).await?)
}
```

**After**

```rust
async fn complete(&self, system: &str, user: &str, config: &CompletionConfig)
    -> Result<Completion, ProviderError>
{
    Ok(self.call_my_backend(system, user).await?.into())
}
```

**What you do:** change the return type, and add `.into()` on the value you return. Two edits per
implementation, plus any helper of your own that returns the old type.

`Completion` carries the text plus whatever the provider could measure about producing it. A
provider that measures nothing reports **not measured** — never zeros, which would claim a
measurement that never happened. That is what `String::into()` gives you.

**Why the trait had to change.** The report now records every completion attempt with its
termination reason, its token counts, and its reasoning state. None of that fits in a `String`. A
new trait method with a default body was considered and rejected: it would leave two completion
paths that can drift, and an implementor who never overrode it would be silently mute — the quiet
no-op this release exists to remove.

**Reporting more than the minimum**, if your backend does measure:

```rust
use magi_core::prelude::{Completion, CompletionTelemetry, FinishReason};

let telemetry = CompletionTelemetry::unmeasured()
    .with_finish(FinishReason::Stop)
    .with_completion_tokens(1280)
    .with_prompt_tokens(569);
Ok(Completion::new(text).with_telemetry(telemetry))
```

Both types are `#[non_exhaustive]`, so they are built with `unmeasured()` plus `with_*` rather
than a struct literal. A field added later costs one more method and breaks nobody.

---

## 2. `RotationKind` gains four variants and becomes `#[non_exhaustive]`

**Before** — three causes shared `Transport`, which everywhere else means *the run was condemned*:

```rust
match hop.kind() {
    RotationKind::Transport => /* ...including content failures that condemned nothing */,
    RotationKind::Timeout => …,
    RotationKind::Schema => …,
}
```

**After**

```rust
match hop.kind() {
    RotationKind::Transport | RotationKind::Timeout => /* run-wide */,
    other if other.is_mage_local() => /* this seat only */,
    _ => /* a cause a newer version reports */,
}
```

**What you do:** add a `_` arm, and prefer `is_mage_local()` over listing the mage-local variants
by hand. The accessor is not sugar: with `#[non_exhaustive]` you **must** write a catch-all, and
that is precisely where the next cause would be classified into the wrong scope with nothing
failing.

The new variants are `OversizedResponse`, `ExternalFailure`, `EmptyCompletion` and
`ResponseContract`. The first two **already behaved** mage-local — they simply could not say so
while the enum was frozen, so the precision rode in the `detail` text behind a `mage-local: `
prefix.

**That prefix is gone.** If you were matching on it, match on the kind instead. A copy of the
scope in prose can only ever contradict the type that carries it.

---

## 3. `ProviderError::Http { status: 0 }` no longer exists

The sentinel it used was `PARSE_FAILURE_STATUS`, and it is gone from the crate entirely.

**Before** — a parse or contract failure arrived as an HTTP error with a status that is not one:

```rust
if let ProviderError::Http { status: 0, .. } = err { /* unusable response */ }
```

**After**

```rust
use magi_core::prelude::{ProviderError, ResponseContractCause};

match err {
    ProviderError::ResponseContract { reason: ResponseContractCause::Unreadable, .. } => …,
    ProviderError::ResponseContract { reason: ResponseContractCause::NoMessage, .. } => …,
    ProviderError::ResponseContract { reason: ResponseContractCause::RedirectRefused, .. } => …,
    ProviderError::EmptyCompletion { telemetry, cap, .. } => …,
    ProviderError::NoGeneration { .. } => …,
    _ => …,
}
```

The three causes, since the prose further down names them: `Unreadable` is a body that could not
be read at all — unparseable, or not valid UTF-8; `NoMessage` is a well-formed body that does not
carry the message the contract promises; `RedirectRefused` is a redirect chain this crate declines
to follow. `ResponseContractCause` is `#[non_exhaustive]`, so match it with a `_` arm.

`EmptyCompletion` carries a `CompletionTelemetry`, not a bare termination reason: the
termination is `telemetry.finish`, and alongside it travel the token counts and the reasoning
measurement. That is deliberate — an empty completion is usually a model that spent its whole
budget reasoning, and reporting the cut without the number that explains it is the blindness
this release exists to end.

**One behaviour change inside this one, if you use the Anthropic provider with extended
thinking.** A response that exhausts `max_tokens` comes back carrying a thinking block and no
text block. That used to surface as `ResponseContract { NoMessage }` — a broken contract — and
now surfaces as `EmptyCompletion`, naming the budget that cut it. A genuinely empty `content`
array is still `NoMessage`: nothing was sent at all, and no termination reason makes that
legitimate.

**What you do:** match the contract variants. `ResponseContractCause` is exported from the
prelude; its own variants are plain unit variants, and it is the enum that carries
`#[non_exhaustive]`. The `..` in the example above belongs to `ProviderError`'s variants, which
are each `#[non_exhaustive]` — so match those with `..` and keep a `_` arm for causes added
later.

**One of them is now retried where the old sentinel was not.** `Http { status: 0 }` was
explicitly non-retryable; `ResponseContractCause::Unreadable` **is** retryable, because a body can
be unreadable for having been clipped in transit and asking again can genuinely return something
different. A backend that consistently returns malformed JSON therefore costs a full retry chain
where it used to cost one attempt. `NoMessage` and `RedirectRefused` stay non-retryable: neither
changes on a second try.

`Http.status` now only ever holds a real HTTP status. That is what makes lineage condemnation
honest **by construction** rather than by comment: the synthetic zero is how a contract failure
inherited run-wide semantics it was never entitled to.

`EmptyCompletion` carries the budget in force, and its message says so — the error names its own
fix instead of sending you to inspect a healthy network.

**The message names the cap as the fix only where the cap can be the fix**, and that distinction
is worth knowing if you match on the text. The budget in force is always stated, because it is an
observation. What is conditional is the advice, on three cases:

| the backend reported | the message |
|---|---|
| `max_tokens`, or no reason at all | says the budget is `configurable via CompletionConfig::max_tokens` |
| a reason this crate interprets and that is not the budget (`end_turn`, `load`) | says raising it **does not address** the case |
| a reason this crate does not interpret | says whether the budget was reached **cannot be told** from it |

**The wording is diagnostic, not a contract.** It is written for a person reading a failure,
and it will be reworded when a clearer sentence is found. Do not branch on the text — branch on
`FinishReason`, which is typed, `#[non_exhaustive]`, and the thing the wording is derived from.

The third case exists because asserting the other way would be inventing evidence:
`model_context_window_exceeded` reaches it and *is* about running out of room. Telling an operator
to raise `max_tokens` because the model refused would be the same misdiagnosis as the `http error
0` this release removed, one layer in.

**`NoGeneration` is different from the rest and aborts the run.** It means the backend accepted a
request and generated nothing, whose known cause is a request this crate built wrongly. It is
raised as `MagiError::CrateDefect` and does **not** rotate: rotating would reproduce our own bad
request against every seat in turn. Its message keeps what was **observed** apart from the
**hypothesis** about why, because the classification rests on a single captured case.

---

## 4. `OllamaProvider` completes on `/api/chat`

**Before:** completions went to `/v1/chat/completions`, Ollama's OpenAI-compatibility layer.

**After:** always `POST {base}/api/chat`, Ollama's native API. There is no conditional route and
no compatibility mode.

**What you do:** nothing, unless something between you and the backend has per-path rules —
a reverse proxy allow-list, a gateway route, a firewall rule, or request logging keyed on the old
path. Point those at `/api/chat`.

**Why unconditional.** `think: false` was measured **accepted with HTTP 200 and no effect** on the
`/v1` layer, and effective natively: the same model, the same 62k payload, `602` tokens and a
valid verdict in 7.7 s natively, against 32 768 tokens and nothing on `/v1`. A conditional route
would have shipped a second mode into a public surface, and a second mode costs another major to
remove — so it does not get removed.

`OpenAiCompatibleProvider` is unaffected and remains the documented path for OpenAI cloud,
LocalAI, vLLM, LM Studio and llama.cpp-server.

---

## 5. `CompletionConfig::max_tokens` defaults to `16_384`

**Before:** `4096`. **After:** `16_384`.

**What you do:** nothing for most deployments — but read the Anthropic note below if you pin a
literal model id.

**Why.** With the real system prompt on a 62k bundle, `glm-5.2` demanded **10 686** completion
tokens. The old default truncated a legitimate verdict from a model that converges, not merely
from a pathological one. In one captured degraded run, the second candidate in a seat's rotation
chain is measured converging at this budget — that `degraded 2/3` would have been `3/3`.

**What the raise does NOT buy.** A model that spends its whole budget in a reasoning channel is
not rescued by a bigger budget: one was measured returning nothing at `16_384` **and** at
`32_768`. That is what `ReasoningControl` is for. Reading this number as the fix repeats the first
hypothesis the reporters measured until it broke.

The memory bound does not move: the response body cap stays at its 1 MiB floor until
`max_tokens` passes `65_536`.

> ### ⚠ The Anthropic provider takes this value verbatim
>
> `ClaudeProvider` passes `max_tokens` straight through **without clamping it** against the
> model's own output ceiling, and that ceiling varies by model. Asking for more than it is a
> **400 from Anthropic**, not a degraded answer.
>
> It is deliberately not clamped: a silent clamp is exactly the quiet no-op this release removes
> elsewhere.
>
> **Who is affected:** only a consumer that pins a **literal pre-4.x model id** through the
> passthrough and never sets `max_tokens`. The three aliases this crate resolves — `sonnet`,
> `opus`, `haiku` — are all 4.x models whose ceilings are far above `16_384`, and
> `default_model_for_mode` returns `opus` for every mode, so a consumer on the default
> configuration cannot hit this.
>
> **What you do:** if you pin a literal pre-4.x id, set `max_tokens` explicitly.

---

## 6. `MagiReport` gains `completions`

**Before:** the report said which seats failed and where they rotated to, but nothing about what
each attempt actually spent.

**After:**

```rust
pub completions: BTreeMap<AgentName, Vec<CompletionRecord>>,
```

One entry per completion **attempt** — all of them, not only the ones that were cut. Each carries
the model, the budget in force, the termination reason, the token counts and the reasoning state.

### One field inside it is an `Option`, and the reason is the release's own thesis

`ReasoningState::Unsupported` carries `chars: Option<usize>`, not `usize`. All three providers
that report this state can reach it with nothing to count: a wire with no separate reasoning
channel has nothing to read, a compatibility body can omit the field, and an Anthropic response
can carry a `redacted_thinking` block that proves the channel FIRED while carrying nothing
countable. They all used to report `0`, and on this variant a zero reads as "no reasoning came
back" — which suggests the control worked, on the variant that exists to declare it did not.

`Some(0)` keeps its meaning and is worth having: the channel was read and was empty. `None` means
nobody could measure it. If you match on this field, add the `Option`.

**What you do:** nothing to keep compiling for `completions` itself — the field is additive and
`MagiReport` is `#[non_exhaustive]`. But if you **deserialize** reports with a stricter reader (a schema
validator, another language's model, a `deny_unknown_fields` struct), a `4.0.0` report will not
load until that reader tolerates the new key. Additive is not the same as invisible.

**Why all of them.** Recording only the notable attempts leaves you blind until the first cut,
which is the blindness this whole release is about: `4096` did not fail all at once, it had been
scraping by. Knowing how close an attempt came is what makes the next budget a decision instead of
a reaction.

**What it costs, said out loud:** about **3 entries (~400 B)** on a clean three-seat run, and up
to **~18 (~2.5 KB)** when every seat rotates and takes its corrective retry.

**It is not an extraction failure.** `completions` and `extraction_failures` are **disjoint**. An
attempt that was cut and still produced a valid verdict belongs in the first and nowhere else —
filing it in the second would assert a failure that did not happen, and a consumer counting that
list to gate a run would start seeing failures where extraction went perfectly.

### `ReasoningState`'s `Debug` output changed

**Before:** `#[derive(Debug)]`, which printed the trace text when a consumer had opted into
carrying it.

**After:** hand-written, rendering `text: "<N chars withheld>"` instead.

**What you do:** nothing, unless you were parsing `Debug` output — which you should not be. The
reason it changed is that the trace is model-authored text that never passes the `Validator` and
is never redacted, and `ProviderError` derives `Debug` and can hold this type, so a consumer
logging an error with `{:?}` was carrying it into their logs. The elision is announced rather
than silent, the same way `ClaudeProvider`'s `Debug` marks its API key: a `Debug` that drops a
field without saying so misleads whoever reads it.

### The reasoning trace is opt-in, and turning it on accepts four things

`CompletionConfig::reasoning_trace` is `false` by default. With `false` the report carries the
trace's **length**; with `true` it carries the length **and** the text. The flag adds, it never
substitutes.

Turning it on accepts: the text is the **model's**, not this crate's; it does **not** pass the
`Validator`; it is **not redacted**; and it has **no cap** — worst case
`(1 + max_rotations) × calls_per_model` traces per agent, times three agents, which is up to
**18 per run** with the shipped defaults, and traces of ~141 000 characters per agent have been
measured. That figure is a measured reference, not a ceiling.

---

## 7. The vendor termination vocabularies are fully translated

**Before:** on the Anthropic wire, `end_turn`, `stop_sequence`, `tool_use` and `max_tokens` were
translated and everything else became `FinishReason::Other`.

**After:** `refusal` and `pause_turn` also read as `FinishReason::Stop`, and
`model_context_window_exceeded` reads as `FinishReason::Length`. On the OpenAI-compatible wire,
`content_filter`, `tool_calls` and `function_call` likewise read as `Stop`.

**What you do:** nothing, unless you match on `FinishReason::Other` expecting to find those
strings in it. **Why it changed** is the part worth keeping: what lands in `Other` decides what
the empty-completion message is allowed to claim. An untranslated reason renders as "cannot be
told"; a *documented* one left untranslated by oversight turned a knowable case into an
unknowable one, and filed Anthropic's own out-of-room response as a broken contract. `Other` now
means a value neither vendor has published.

`FinishReason::Stop` is correspondingly wider than "the model finished its answer" — it always
was, since `tool_use` mapped there and a turn that stops to call a tool has finished nothing.
What its members share is the only property anything downstream asks of them: the reply is not
short because it ran out of room.

## 8. `ClaudeProvider::parse_response` is gone

**Before:** `pub fn parse_response(body: &str) -> Result<String, ProviderError>`.

**After:** removed. Nothing replaces it, and nothing inside the crate called it either.

**What you do:** call `complete()`. If you were parsing a captured body outside a request there
is no replacement, and the reason it went is worth stating: it and `complete()` gave **opposite
answers for the identical body**. A reply whose only text block is empty is `Ok("")` through
`parse_response` and `EmptyCompletion` through `complete()`. One of those says the call
succeeded and the other says the model produced nothing — and a release whose entire subject is
telling those two apart cannot ship both as public answers. The one with no telemetry to answer
with is the one that went.

Its behaviour also changed on the way out, which matters only if you vendored it: it joins
**every** text block instead of returning the first. Anthropic interleaves text with `thinking`
and `tool_use` blocks, so a reply split across two text blocks used to come back cut at the
first, and a first block carrying `null` used to discard the rest entirely — which the
completion path then reported as an exhausted output budget.

## 9. The time defaults change

<!-- PENDING: MS2 F-2 — the seven time values. Enforced by ci/check_pending.sh, which the
     release workflow runs: the tag cannot be cut while this marker is here. -->

`MagiConfig::timeout` rises and `RetryConfig::operation_budget` falls, so that the agent's ceiling
covers the retry chain's worst case and an exhausted budget is reported as a **typed** abandonment
rather than an opaque timeout cut. Waiting times change for a consumer who never configured them,
which is why this is a contract change and not an internal adjustment.

The exact values ship with the milestone that derives them; this section is completed before the
release is tagged.

> ### ⚠ Infrastructure Timeout Checklist — run this BEFORE you upgrade
>
> The agent ceiling rises past **600 seconds**, which crosses the range where infrastructure
> timeouts live. A proxy that cuts the connection at 600 s reaches this crate as
> `ProviderError::Network` — the one class that feeds the endpoint-down latch — so **two of them
> abort the run** with `MagiError::EndpointDown`, an error that does not mention your proxy.
>
> The crate cannot distinguish a proxy reset from a real network failure: they are the same error
> on the socket, and guessing is exactly what it refuses to do. So the check is yours:
>
> - [ ] reverse proxy read/idle timeout **above** `MagiConfig::timeout`
> - [ ] load balancer idle timeout **above** it
> - [ ] API gateway timeout **above** it
> - [ ] Kubernetes ingress `proxy-read-timeout` **above** it
> - [ ] any service mesh or sidecar timeout **above** it
>
> **If the symptom is `EndpointDown` and your backend was answering, look at these first** — the
> crate will not name them for you. The timeout error carries the configured ceiling so you can
> compare it against where the cut actually happened, but a proxy reset does not arrive as a
> timeout, so that message will not appear on this path.

---

## What did not change

- **The verdict sentinel.** A cut before the closing marker is still `Unterminated`, inside the
  JSON still `InvalidJson`, and after the closing marker still loses only trailing prose. One of
  the reports that motivated this release believed a truncated-but-parseable verdict could be
  accepted as complete; that was closed in `3.0.0` and is pinned here against regression.
- **Run-wide condemnation for genuine transport failures.** `is_connection`, the endpoint-down
  threshold, and the ordering inside `register_transport_failure` are untouched. Their semantics
  were always right; the problem was who was arriving there in disguise.
- **`OpenAiCompatibleProvider`'s endpoint**, and every hardening `3.1.0` shipped: the derived body
  cap, URL redaction, and `.referer(false)` are shared machinery that the native path reuses
  rather than reimplements.
