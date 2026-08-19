// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-18

//! The twelve scenarios valid against `magi-core` `3.2.0`: `S1, S2, S2b, S4, S5,
//! S6, S7, S14, S15, S16, S20, S21` — the exact set the spec's `§2.3` assigns
//! to stage E1 (`sbtdd/smoke-harness-spec.md`).
//!
//! # The rule every `assert_fn` below follows
//!
//! **Never `Pass` on absent data.** Each function asks itself, before
//! returning: could this state hold if the property under test were false, or
//! if the data it reads were simply missing? If yes, the branch is wrong.
//! Concretely:
//!
//! - Missing report / missing wire traffic → [`Assertion::skip`] with a reason
//!   an operator can act on — never `Pass`. One exception, and it is about
//!   WHOSE fault the absence is: for the four scenarios that read a preflight
//!   error, an ABSENT error — or one belonging to a stage this invocation never
//!   asked to break — is [`Assertion::out_of_scope`], not a `Skip`. Both are
//!   absences this run did not ask about, and the difference is not cosmetic:
//!   `Skip` is exit 2 and `OutOfScope` is exit 0.
//!   [`preflight_error_for_stage`] draws that line and says why.
//! - A shape the assertion CAN read, but that contradicts the property → `Fail`
//!   — never a silent `Skip` that degrades a real defect into "untested".
//! - An `.all()`/`.any()` over a collection that could legitimately be EMPTY is
//!   paired with a non-empty check, because `[].iter().all(f)` is vacuously
//!   `true` regardless of `f`.
//! - `S4` (rotation) is the sharpest instance of the rule: a rotation `chain`
//!   that is EMPTY, and one whose single hop lands on the SAME lineage it left,
//!   must both `Fail` the "rotated to a different candidate" property, not
//!   silently satisfy it — an assertion that passes on an empty `chain` is
//!   exactly the defect this milestone has shipped repeatedly.
//!
//! # The companion rule: never `Fail` on a run that never happened
//!
//! The rule above forbids PASSING on absent data; this one forbids FAILING on
//! it. An assertion whose sentence presupposes that traffic happened — "rotated
//! because the injected failure fired", "degraded because a seat was knocked
//! out" — is asking about an event, and if that event's own precondition never
//! occurred there is nothing for the crate to be right or wrong about.
//! Reporting `Fail` there means exit 1, a verdict about the crate, for a run
//! the crate never entered.
//!
//! **Three states, never two**, and collapsing any pair trades one blindness
//! for another:
//!
//! 1. the run never reached the wire → `Skip`, naming that;
//! 2. traffic happened and the injection did not fire → `Skip`, naming the
//!    injection. The single exception is `S4`'s WIRE assertion, whose own
//!    subject IS the firing: there, case 2 is the finding and must `Fail`;
//! 3. traffic happened, the injection fired, and the crate misbehaved →
//!    `Fail`, which is the verdict this harness exists to produce.
//!
//! [`why_the_wire_cannot_answer`] draws the line between 1 and the rest, and
//! [`why_the_forced_failure_cannot_be_read`] draws it between 2 and 3. `S4` and
//! `S15` are the two scenarios that need them; the audit of which other
//! scenarios do not, and why, is recorded at each of their own definitions.

use crate::alias::magi_core::reporting::MagiReport;
use crate::alias::magi_core::rotation::RotationKind;
use crate::config::RunId;
use crate::proxy::{sha256_hex, RequestRecord};
use crate::runner::{
    assert_that, Assertion, BackendNeed, BuildOutcome, RunContext, Scenario, Source,
};

// ---------------------------------------------------------------------------
// Shared constants
// ---------------------------------------------------------------------------

/// The OpenAI-compatible completions path `OllamaProvider` speaks in `3.2.0`
/// (the native `/api/chat` path arrives with the EC major, out of scope for
/// this stage). Duplicated from `runner.rs`'s own doc comment rather than
/// imported: no `pub` constant exists there for it, and this task does not
/// touch that module.
const COMPLETIONS_PATH: &str = "/v1/chat/completions";

/// Mirrors `runner.rs`'s private `INJECTED_FAILURE_STATUS`. Duplicated, not
/// imported, for the same reason as [`COMPLETIONS_PATH`]: the constant is
/// private to a module this task does not own.
const INJECTED_FAILURE_STATUS: u16 = 500;

/// Repo-relative suffix of the fixed certificate path
/// (`docs/test/smoke-certificate.md`), which `S16` must find to be the ONLY
/// thing `git status` names once a `--smoke-2` run finishes clean.
const CERT_PATH_SUFFIX: &str = "docs/test/smoke-certificate.md";

// ---------------------------------------------------------------------------
// Shared helper: the four `Source::Preflight` scenarios (S6, S7, S14, S20)
// ---------------------------------------------------------------------------

/// Extracts a preflight-sourced context's error text, but only if it names
/// THIS scenario's own stage.
///
/// `S6`, `S7`, `S14` and `S20` all read the SAME [`RunContext::error`] string —
/// the preflight fails at exactly one stage and stops, so every session that
/// reaches any of them shares that one message. If each scenario asserted
/// only "the preflight failed", all four would `Pass` on whichever ONE stage
/// actually broke. `PreflightError`'s `Display` is `"{stage:?}: {msg}"`
/// (`preflight.rs`), and `Stage`'s `Debug` output is a closed, stable set of
/// identifiers — matching on `"<Stage>: "` as a prefix is precise, not the
/// free-text substring matching this project rejects elsewhere, because the
/// thing being matched is a closed enum's own name, not prose.
///
/// **Wire-precondition audit (module doc, "The companion rule"): not needed for
/// any of the four.** They read `error` and nothing else — no `report`, no
/// `records` — and their subject is a preflight that stopped BEFORE any run, so
/// there is no traffic for them to presuppose. This function is their guard: an
/// absent error, and an error belonging to another stage, are both `OutOfScope`
/// — see "Why NOT `Skip`" below, which is the ruling that made them so.
///
/// # Parameters
///
/// * `ctx` — the context to read `error` from.
/// * `stage_prefix` — this scenario's own stage, e.g. `"Backend: "`.
///
/// # Errors
///
/// `Err(())` when there is no error to read, or when the error belongs to a
/// DIFFERENT stage. Both are `OutOfScope`, never `Fail` (a scenario cannot fail
/// on evidence pointing at a stage it does not own) and — since the ruling of
/// this round — never `Skip` either.
///
/// # Why NOT `Skip`, which is what it used to be
///
/// Each of these four asserts what happens when one preflight stage FAILS, and
/// no stage fails unless the invocation induces it: an unreachable endpoint for
/// `S6`, a slow one for `S7`, a broken config for `S14`, `--break-proxy` for
/// `S20`. A plain `cargo run` induces none, so all four skipped on every
/// healthy run — and since `Skip` is exit 2, **no invocation of this harness
/// could return 0 at all**. `Skip` means "I tried and could not test", which is
/// a fault worth someone's attention; these were never tried, because this run
/// was not asked to.
///
/// The reason string is dropped with the `Skip`: an out-of-scope row always
/// says the same thing, so there is nothing per-call to carry.
fn preflight_error_for_stage<'a>(ctx: &RunContext<'a>, stage_prefix: &str) -> Result<&'a str, ()> {
    match ctx.error {
        // The preflight did not fail at all, so this scenario's own subject
        // never happened.
        None => Err(()),
        // It failed at SOME stage, but not this one: the invocation induced a
        // different fault, and this scenario was not what it asked about.
        Some(e) if !e.starts_with(stage_prefix) => Err(()),
        Some(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// S1 — an outside LlmProvider fails typed
// ---------------------------------------------------------------------------

/// `S1` — an outside provider can fail in a typed way
/// (`sbtdd/smoke-harness-spec.md`, "S1").
///
/// Reads [`RunId::NoBackend`], whose every seat is
/// [`crate::external::AlwaysFailsExternally`] — an `LlmProvider` implemented
/// OUTSIDE `magi-core`. Inside `src/` every `ProviderError` variant is always
/// constructible, so a test living there could pass with the constructor
/// broken; this one lives in the harness for exactly that reason (the
/// `E0639` lesson of `2.0.0`).
///
/// With zero of three agents succeeding, `analyze()` returns
/// `Err(MagiError::InsufficientAgents { succeeded: 0, required: 2 })` — proof
/// the typed failure propagated instead of panicking or hanging.
///
/// **Wire-precondition audit (module doc, "The companion rule"): not needed.**
/// This scenario reads neither `report` nor `records`, and its run has no wire
/// at all — every seat is an in-process provider that fails before any request
/// is built. Its one precondition is the error itself, and the `let else` above
/// is the guard for it.
///
/// **Mage-local, not run-wide**: had the crate misclassified `ExternalErrorKind`
/// as connection-class, it would report `MagiError::EndpointDown` instead — a
/// DIFFERENT variant, whose `Display` names "endpoint down". Checking BOTH
/// strings, not just one, is what makes this non-vacuous: an opaque failure
/// message would satisfy neither half on its own.
fn s1_external_provider_fails_typed(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME: &str =
        "an outside LlmProvider fails typed, classified mage-local (never endpoint-down)";
    let Some(err) = ctx.error else {
        return vec![Assertion::skip(
            NAME,
            "the offline run produced no typed error to read: either it did not run, or it \
             unexpectedly succeeded",
        )];
    };
    vec![assert_that(
        NAME,
        err.contains("insufficient agents") && !err.contains("endpoint down"),
    )]
}

// ---------------------------------------------------------------------------
// S2 — happy path against a real backend
// ---------------------------------------------------------------------------

/// `S2` — the happy path against a real backend, through the proxy
/// (`sbtdd/smoke-harness-spec.md`, "S2").
///
/// Reads [`RunId::HappySmall`]. Four independent assertions, so a red row says
/// WHICH property broke rather than "the happy path is unhappy".
///
/// # Wire-precondition audit (module doc, "The companion rule"): already held
///
/// The fourth assertion reads `records` and would be vacuous over an empty set,
/// so it carries a non-empty check — and a non-empty check is exactly the shape
/// that can `Fail` on a run that never reached the wire. **Here it cannot**, and
/// the reason is the gate above it rather than a guard of its own: all four
/// assertions are reached only when `report` is `Some`, which means `analyze()`
/// returned a full report, which means three agents completed, which means
/// completion requests happened. A run that never got to the wire has no report
/// and skips with the run's own reason.
///
/// What the non-empty check can therefore still `Fail` on is a report in hand
/// with no completion visible to the proxy — traffic that bypassed it. That is a
/// real finding and must stay red: "every request goes through the proxy" is the
/// invariant that makes everything else in this harness observable.
///
/// # What the injection check does NOT claim
///
/// It cannot tell a backend's own `500` from an injected one; nothing on the
/// wire distinguishes them. It does not need to: **this run configures no
/// injection at all** ([`crate::runner::RunSpec::for_stage_e1`] gives
/// `HappySmall` `injection: None`), so the proxy has no rule to apply and a
/// `500` here can only be the backend's. The assertion's name says what is
/// actually checked — no completion carried the failure status — rather than
/// promising a distinction the wire cannot make. It used to read "the proxy
/// injected nothing", which is true by construction here and so was not a claim
/// about anything.
fn s2_happy_path_against_real_backend(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME_VERDICTS: &str = "all three mages returned a verdict";
    const NAME_DEGRADED: &str = "the run is not degraded";
    const NAME_JSON: &str = "the report serializes to JSON";
    const NAME_NO_INJECTION: &str =
        "every completion was answered, and none carried the failure status this harness injects";

    let Some(report) = ctx.report else {
        let reason = ctx
            .error
            .map(str::to_string)
            .unwrap_or_else(|| "the run never happened".to_string());
        return vec![
            Assertion::skip(NAME_VERDICTS, reason.clone()),
            Assertion::skip(NAME_DEGRADED, reason.clone()),
            Assertion::skip(NAME_JSON, reason.clone()),
            Assertion::skip(NAME_NO_INJECTION, reason),
        ];
    };

    let injection = if ctx.proxy_degraded {
        Assertion::skip(
            NAME_NO_INJECTION,
            "the proxy registry degraded during this run; a partial record could fail an \
             assertion the crate satisfied perfectly",
        )
    } else {
        // `.all()` over an EMPTY iterator is vacuously true, so "nothing was
        // ever injected" must not be provable by "nothing was ever seen" —
        // hence the explicit non-empty guard alongside it.
        let completions: Vec<&RequestRecord> = ctx
            .records
            .iter()
            .filter(|r| r.path == COMPLETIONS_PATH)
            .collect();
        // Deliberately does NOT require `response_recorded`. A review round
        // proposed that, reasoning that an unrecorded response leaves
        // `response_status` at 0 and lets "nothing was injected" pass over a
        // completion nobody saw the answer to. That reads the field as "a
        // response arrived", and it means "the response BODY was buffered" —
        // which happens only for the two probe paths. Completions STREAM, so
        // `response_recorded` is false for every real one while
        // `with_status_only` still records the true status. Adding the
        // requirement was tried and turned this scenario red on every live run.
        assert_that(
            NAME_NO_INJECTION,
            !completions.is_empty()
                && completions
                    .iter()
                    .all(|r| r.response_status != INJECTED_FAILURE_STATUS),
        )
    };

    vec![
        assert_that(NAME_VERDICTS, report.agents.len() == 3),
        assert_that(NAME_DEGRADED, !report.degraded),
        assert_that(NAME_JSON, serde_json::to_string(report).is_ok()),
        injection,
    ]
}

// ---------------------------------------------------------------------------
// S2b — proxy transparency, verified against a deterministic endpoint
// ---------------------------------------------------------------------------

/// `S2b` — the proxy is TRANSPARENT, checked against `POST /api/show`: a
/// deterministic endpoint that carries a body, unlike `/api/tags`
/// (`sbtdd/smoke-harness-spec.md`, "S2b").
///
/// Two independent checksums, both over bytes the HARNESS itself sent or
/// received directly — never two LLM completions, which are not deterministic
/// even at `temperature: 0` and would make "equivalent" meaningless.
///
/// **Three assertions, because the spec's transparency claim has three parts**:
/// *"los bytes del cuerpo de la respuesta son IDENTICOS **y el status
/// coincide**"*, plus the request body. The status comparison was specified and
/// missing: a proxy that relayed the right bytes under a different status — a
/// `200` turned into a `500`, or the reverse — satisfied both checksums while
/// changing exactly what the crate classifies on, which is the one thing the
/// rest of this harness reads.
fn s2b_the_proxy_is_transparent(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME_REQUEST: &str = "the request the proxy relayed is byte-identical to ours";
    const NAME_RESPONSE: &str = "and so is the response it relayed back";
    const NAME_STATUS: &str = "and the status it relayed is the one the backend gave";

    // A degraded proxy is a HARNESS fault, and this scenario reads a record the
    // proxy produced. Without this gate a failed body read — which the proxy
    // records as an empty body — makes the checksum compare a hash of nothing
    // and reports FAIL: exit 1, a verdict about the crate, for something the
    // harness did. Every other scenario that reads proxy state already gates on
    // this; this one did not, and it is the one whose whole subject IS the proxy.
    if ctx.proxy_degraded {
        const WHY: &str = "the proxy degraded during this run, so its record cannot answer for \
                           what went on the wire";
        return vec![
            Assertion::skip(NAME_REQUEST, WHY),
            Assertion::skip(NAME_RESPONSE, WHY),
            Assertion::skip(NAME_STATUS, WHY),
        ];
    }

    // All four terms together, never some of them: the probe sets them in one
    // step, so a subset present would mean a probe that half ran — and comparing
    // against a term that is not there says nothing about transparency.
    let (Some(rec), Some(sent), Some(direct), Some(direct_status)) = (
        ctx.probe_record,
        ctx.probe_sent_body,
        ctx.direct_probe_body,
        ctx.direct_probe_status,
    ) else {
        const WHY: &str = "the transparency probe did not complete; nothing to compare";
        return vec![
            Assertion::skip(NAME_REQUEST, WHY),
            Assertion::skip(NAME_RESPONSE, WHY),
            Assertion::skip(NAME_STATUS, WHY),
        ];
    };
    // All three skip on an unrecorded response, not just the body comparison —
    // which is more conservative than it strictly has to be, and is kept
    // deliberately. Reaching this branch means the probe's body read FAILED, and
    // the only path that can produce it (`forward_buffered`, the one the probe
    // paths take) latches `proxy_degraded` in the same step, so the gate above
    // has already returned. Splitting this into "skip the body, keep the request
    // and status" would add two branches to distinguish states the proxy cannot
    // produce independently — and this is a guard, where a state nobody can
    // reach is a state nobody tests.
    if !rec.response_recorded {
        const WHY: &str = "the probe response was not recorded";
        return vec![
            Assertion::skip(NAME_REQUEST, WHY),
            Assertion::skip(NAME_RESPONSE, WHY),
            Assertion::skip(NAME_STATUS, WHY),
        ];
    }
    // The comparison is by CHECKSUM, and `RequestRecord::record_of` hashes the
    // FULL body before any recording cap applies — so the cap cannot make this
    // comparison wrong, and there is deliberately no guard against it here. A
    // previous round added one on the opposite belief; the code contradicted it,
    // and the guard would have SKIPPED a valid comparison on exactly the large
    // payload the cap exists for.
    vec![
        assert_that(NAME_REQUEST, rec.body_sha256 == sha256_hex(sent.as_bytes())),
        assert_that(NAME_RESPONSE, rec.response_sha256 == sha256_hex(direct)),
        assert_that(NAME_STATUS, rec.response_status == direct_status),
    ]
}

// ---------------------------------------------------------------------------
// S4 — rotation and its cause, forced by injection
// ---------------------------------------------------------------------------

/// Whether the wire assertion has anything to answer WITH, and why not when it
/// does not.
///
/// # The distinction this function exists to draw
///
/// The wire assertion reads "the injected failure reached a completion request".
/// Exactly one shape of absence is a real finding, and it is NOT absence of
/// traffic:
///
/// * **Completion requests exist and none carries the injected status** — the
///   injection had its subject and did not fire. That is a defect of the proxy
///   or of the injection wiring, it is observable, and it must FAIL. This
///   function returns `None` for it, so the caller asserts.
/// * **No completion request exists at all** — the run never got as far as the
///   wire: the `Magi` would not build, the payload could not be generated, the
///   crate failed before dispatching, the attempt ran out of time. The injection
///   never had a subject, so "it did not fire" is not a fact about anything.
///   Asserting here reports **Fail**: exit 1, a verdict about the crate, for a
///   run the crate never entered. That is the single failure this harness exists
///   to eliminate, and it is what this guard closes.
/// * **Nothing was injected for this run** — the sentence has no subject at all.
///   Its two sibling assertions already skip on this; the wire one failed,
///   which was the same inconsistency one level down. Note that with no
///   injection configured a genuine backend `500` would have satisfied the
///   assertion by coincidence — precisely what its own name disclaims.
///
/// **The real finding and the harness fault ARE distinguishable from what
/// [`RunContext`] carries**, and the discriminator is the presence of completion
/// traffic — not the presence of a report, which was the other candidate and is
/// the wrong one: a run can produce a typed error and still have reached the
/// wire, and a run can produce no report for a reason that never touched it.
/// `records` is what answers "did a completion request happen at all", so
/// `records` is what decides. An injection that failed to fire ALWAYS leaves
/// completion records behind — it is applied per completion request — so the
/// case that must FAIL can never be mistaken for the case that must SKIP.
///
/// # Parameters
///
/// * `ctx` — the rotation run's context.
///
/// # Returns
///
/// `None` when the assertion can be evaluated, or `Some(reason)` naming what
/// stopped it — which the caller turns into a `Skip`, never a `Fail`.
///
/// # Complexity
///
/// `O(r)` in the number of records: one scan for a completion request.
fn why_the_wire_cannot_answer(ctx: &RunContext<'_>) -> Option<String> {
    if ctx.injected_agent.is_none() {
        return Some(
            "no agent was injected for this run, so no injected failure could reach the wire"
                .to_string(),
        );
    }
    if ctx.records.iter().any(|r| r.path == COMPLETIONS_PATH) {
        return None;
    }
    // Absence of traffic, with whatever the run said about why. The reason is
    // built here rather than at the call site so the three shapes stay together.
    Some(match (ctx.error, ctx.over_budget) {
        (Some(e), _) => format!(
            "the run never reached a completion request, so the injection had nothing to fire \
             on; it ended with: {e}"
        ),
        (None, Some(d)) => {
            format!("the run never reached a completion request: it ran out of time after {d:?}")
        }
        (None, None) => "the run never reached a completion request over the wire, so the \
                         injection had nothing to fire on"
            .to_string(),
    })
}

/// Whether the injected failure ACTUALLY reached the crate, and why not when it
/// did not.
///
/// # What this adds to [`why_the_wire_cannot_answer`], and why it is a separate
/// question
///
/// That function answers "did this run reach the wire at all". This one answers
/// the next question down: "was the crate actually made to fail". They are
/// distinct because the two callers need different amounts:
///
/// * `S4`'s wire assertion needs only the first. Its own subject is whether the
///   injection fired, so an injection that had its chance and did not fire is
///   the finding — it must `Fail`, not skip, and it stops at
///   [`why_the_wire_cannot_answer`].
/// * `S15`'s four assertions need BOTH. They read a report and ask whether
///   degradation was reported honestly, which presupposes that something forced
///   a seat down. If the injection never fired, the run was a healthy run: the
///   report is not degraded, three agents answered, and all four assertions go
///   red for a property the crate never had a chance to violate. That is the
///   same inversion `S4` closed one level up.
///
/// # The path is deliberately NOT constrained here
///
/// `S4` asks for the injected status on a COMPLETION request, because rotation
/// is what a failed completion causes. This function asks only that the injected
/// status appear on SOME recorded request, and the difference is deliberate: the
/// proxy applies an injection to any request whose body names the model, so a
/// seat can be knocked out by an injected probe answer just as well as by an
/// injected completion. What `S15` needs to know is that a failure was forced,
/// not which request carried it — and requiring the completion path would make
/// the guard skip a run that really did degrade for the reason we forced.
///
/// Absence of traffic is still measured in COMPLETIONS, through the delegation
/// above: that is what "the run got as far as doing its work" means.
///
/// # Parameters
///
/// * `ctx` — the injected run's context.
///
/// # Returns
///
/// `None` when the assertions can be evaluated, or `Some(reason)` naming what
/// stopped them — which the caller turns into a `Skip`, never a `Fail`.
///
/// # Complexity
///
/// `O(r)` in the number of records: at most two scans.
fn why_the_forced_failure_cannot_be_read(ctx: &RunContext<'_>) -> Option<String> {
    if let Some(reason) = why_the_wire_cannot_answer(ctx) {
        return Some(reason);
    }
    if ctx
        .records
        .iter()
        .any(|r| r.response_status == INJECTED_FAILURE_STATUS)
    {
        return None;
    }
    Some(
        "the run reached the wire but no request carried the injected failure status, so \
         nothing forced a seat down and there is no degradation to judge"
            .to_string(),
    )
}

/// `S4` — rotation and its cause, forced by injection
/// (`sbtdd/smoke-harness-spec.md`, "S4").
///
/// Reads [`RunId::Rotation`]. As of `runner.rs`'s `with_fallback_pool` wiring,
/// this run's trio has a real fallback candidate whose lineage differs from
/// every seat's (`Config::fallbacks`, verified by
/// `the_rotation_run_has_somewhere_to_rotate_to` in `runner.rs`), so rotation
/// is now genuinely observable — the injected seat has somewhere to go.
///
/// Three independent assertions, none of which can pass on absent or
/// unhelpful data:
///
/// - **Wire**: the injected status actually reached a completion request —
///   proof the injection fired at all, independent of rotation. **Guarded by
///   its own two preconditions**, so a run that never got as far as the wire
///   SKIPs instead of failing; see [`why_the_wire_cannot_answer`].
/// - **Rotated**: `report.rotations[agent]`'s first hop exists AND its
///   destination lineage differs from its origin. An EMPTY `chain` fails this
///   (the mage never left its primary); a chain whose hop lands back on the
///   SAME lineage it left also fails this — neither is "rotated to a second,
///   differently-lineaged candidate", and neither may read as `Pass`.
/// - **Cause**: that hop's `RotationKind` is `Transport` — the injected
///   failure is a genuine HTTP 500, so `Transport` is the crate's own,
///   correct classification for it; `Schema` or `Timeout` here would mean the
///   telemetry is naming the wrong cause. Checked only when a hop exists —
///   there is no cause to classify when nothing rotated.
///
/// # The cause assertion is NOT "mage-local versus run-wide", and used to say
/// it was
///
/// The spec's `S4` asks that *"the reported cause distinguishes mage-local
/// from run-wide"*, and this assertion was named for that sentence. **`3.2.0`
/// cannot express it.** `RotationKind` has three variants there — `Transport`,
/// `Schema`, `Timeout` — and its own rustdoc says connection and HTTP failures
/// *both normalize to* `Transport`; the mage-local causes report `Transport`
/// too, with the distinction carried in `detail` behind a `mage-local:` text
/// prefix. So `kind == Transport` separates transport from schema and timeout,
/// and nothing more.
///
/// The name now says what the body checks. **The spec clause is DEFERRED to
/// E2**, where the EC major's typed `RotationKind` variants make the property
/// expressible; asserting it here would have left a green row under a sentence
/// the code could not support, which is the shape of green-by-omission this
/// harness exists to refuse.
fn s4_rotation_and_its_cause(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME_WIRE: &str =
        "the injected failure reached a completion request over the wire, not a coincidence";
    const NAME_ROTATED: &str =
        "the injected agent rotated to a second, differently-lineaged candidate";
    const NAME_CAUSE: &str =
        "the reported rotation cause is Transport, not Schema or Timeout (3.2.0 cannot \
         distinguish mage-local from run-wide; deferred to E2)";

    if ctx.proxy_degraded {
        let reason = "the proxy registry degraded during this run";
        return vec![
            Assertion::skip(NAME_WIRE, reason),
            Assertion::skip(NAME_ROTATED, reason),
            Assertion::skip(NAME_CAUSE, reason),
        ];
    }

    let wire = match why_the_wire_cannot_answer(ctx) {
        Some(reason) => Assertion::skip(NAME_WIRE, reason),
        None => assert_that(
            NAME_WIRE,
            ctx.records.iter().any(|r| {
                r.path == COMPLETIONS_PATH && r.response_status == INJECTED_FAILURE_STATUS
            }),
        ),
    };

    let Some(report) = ctx.report else {
        let reason = ctx
            .error
            .map(str::to_string)
            .unwrap_or_else(|| "the rotation run never happened".to_string());
        return vec![
            wire,
            Assertion::skip(NAME_ROTATED, reason.clone()),
            Assertion::skip(NAME_CAUSE, reason),
        ];
    };

    let Some(agent) = ctx.injected_agent else {
        return vec![
            wire,
            Assertion::skip(NAME_ROTATED, "no agent was injected for this run"),
            Assertion::skip(NAME_CAUSE, "no agent was injected for this run"),
        ];
    };

    let Some(rotation) = report.rotations.get(&agent) else {
        let reason = format!("no rotation telemetry was recorded for {agent:?}");
        return vec![
            wire,
            Assertion::skip(NAME_ROTATED, reason.clone()),
            Assertion::skip(NAME_CAUSE, reason),
        ];
    };

    // From here on, real data exists: the outcome is Pass or Fail, NEVER Skip.
    // An empty chain, or a chain whose hop lands on the same lineage, must
    // FAIL — not vacuously satisfy — "rotated to a different candidate".
    let first_hop = rotation.chain.first();
    let rotated = assert_that(
        NAME_ROTATED,
        first_hop.is_some_and(|hop| hop.to() != hop.from()),
    );
    let cause = assert_that(
        NAME_CAUSE,
        first_hop.is_some_and(|hop| matches!(hop.kind(), RotationKind::Transport)),
    );

    vec![wire, rotated, cause]
}

// ---------------------------------------------------------------------------
// S5 — the probe still reads what it expects
// ---------------------------------------------------------------------------

/// Independently mirrors the SHAPE `OllamaProvider::parse_show_window` reads
/// (that function is `pub(crate)` inside `magi-core` and unreachable from
/// here), so this scenario verifies the wire itself, not the crate's own
/// parser re-run under a different name.
fn show_response_has_measurable_window(body: &[u8]) -> bool {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return false;
    };
    let Some(model_info) = v.get("model_info").and_then(|m| m.as_object()) else {
        return false;
    };
    model_info
        .iter()
        .any(|(k, val)| k.ends_with(".context_length") && val.as_u64().is_some_and(|n| n > 0))
}

/// Independently mirrors the SHAPE `OllamaProvider::parse_tags_digest` reads
/// (also `pub(crate)`), for the same reason as
/// [`show_response_has_measurable_window`].
fn tags_response_has_a_64_hex_digest(body: &[u8]) -> bool {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return false;
    };
    let Some(models) = v.get("models").and_then(|m| m.as_array()) else {
        return false;
    };
    models.iter().any(|entry| {
        entry
            .get("digest")
            .and_then(|d| d.as_str())
            .is_some_and(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit()))
    })
}

/// `S5` — the probe still reads what it expects
/// (`sbtdd/smoke-harness-spec.md`, "S5").
///
/// Reads [`RunId::HappySmall`]'s wire traffic: `analyze()` probes every
/// probing primary via `POST /api/show` (window) and `GET /api/tags`
/// (digest) BEFORE dispatching any completion, so both requests are already
/// in `ctx.records` by the time this scenario runs.
///
/// **FAILS, rather than skipping, on a shape it cannot parse** — that
/// asymmetry is the scenario's whole point: if the API's shape has changed, the
/// scenario FAILS instead of degrading in silence.
///
/// # EVERY answered probe is checked, not the first one found
///
/// A run probes once per probing candidate, so several records of each path
/// exist. Reading only the first left an INTERMITTENT shape change — one model's
/// entry losing its `context_length`, one manifest answering without a digest —
/// hidden behind whichever record happened to come first. `.all()` over an empty
/// set is vacuously true, so it is paired with a non-empty check, as everything
/// else in this file is.
///
/// # Wire-precondition audit (module doc, "The companion rule"): already held
///
/// This scenario reads `records` and presupposes probe traffic, and its
/// non-empty check resolves to `Skip` rather than to `Fail` — see the two
/// branches below. A run that never reached the wire therefore reports "no
/// answered probe was recorded", which is the first of the three states, and
/// nothing here can turn an absent run into a verdict about the crate. The
/// `Fail` side is reserved for a probe that ANSWERED with a shape the crate's
/// parser could no longer read, which is the change this scenario watches for.
fn s5_the_probe_still_reads_what_it_expects(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME_WINDOW: &str = "every probe answer carries a measurable context window";
    const NAME_DIGEST: &str = "every probe answer carries a 64-hex-character digest";

    if ctx.proxy_degraded {
        let reason = "the proxy registry degraded during this run";
        return vec![
            Assertion::skip(NAME_WINDOW, reason),
            Assertion::skip(NAME_DIGEST, reason),
        ];
    }

    let show = answered_probes(ctx, SHOW_PATH);
    let tags = answered_probes(ctx, TAGS_PATH);

    let window = if show.is_empty() {
        Assertion::skip(
            NAME_WINDOW,
            "no answered /api/show probe was recorded for this run",
        )
    } else {
        assert_that(
            NAME_WINDOW,
            show.iter()
                .all(|r| show_response_has_measurable_window(&r.response_body)),
        )
    };
    let digest = if tags.is_empty() {
        Assertion::skip(
            NAME_DIGEST,
            "no answered /api/tags probe was recorded for this run",
        )
    } else {
        assert_that(
            NAME_DIGEST,
            tags.iter()
                .all(|r| tags_response_has_a_64_hex_digest(&r.response_body)),
        )
    };
    vec![window, digest]
}

/// The window probe's path.
const SHOW_PATH: &str = "/api/show";

/// The digest probe's path.
const TAGS_PATH: &str = "/api/tags";

/// The lowest status this scenario reads as "the backend answered the question".
const FIRST_SUCCESS_STATUS: u16 = 200;

/// The first status above the success range.
const FIRST_NON_SUCCESS_STATUS: u16 = 300;

/// Every recorded probe response on `path` that the backend actually ANSWERED.
///
/// # Why a non-2xx record is excluded rather than failed
///
/// A `404` for a model the backend does not hold is a fact about the deployment,
/// not about the API's shape: its body is an error object, which no shape check
/// can parse, so counting it would turn "a fallback model is not pulled" into a
/// red row about the crate. What an API shape change looks like is a **`200`
/// whose body no longer carries what the probe reads**, and those are exactly
/// the records this returns.
///
/// # Parameters
///
/// * `ctx` — the run whose traffic is being read.
/// * `path` — the probe path to collect.
///
/// # Complexity
///
/// `O(r)` in the number of records.
fn answered_probes<'a>(ctx: &RunContext<'a>, path: &str) -> Vec<&'a RequestRecord> {
    ctx.records
        .iter()
        .filter(|r| {
            r.path == path
                && r.response_recorded
                && (FIRST_SUCCESS_STATUS..FIRST_NON_SUCCESS_STATUS).contains(&r.response_status)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// S6 — without a backend, there is no green
// ---------------------------------------------------------------------------

/// `S6` — without a backend, there is no green
/// (`sbtdd/smoke-harness-spec.md`, "S6").
///
/// `Source::Preflight`: an unreachable backend fails the preflight's `Backend`
/// step (`preflight::reachable`) before a single scenario runs. Exercised by
/// `MAGI_SMOKE_ENDPOINT=http://127.0.0.1:1 cargo run` (six-invocation table,
/// README).
fn s6_no_backend_no_green(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME: &str =
        "an unreachable backend makes the preflight cut with exit 2, naming the backend as \
         the cause";
    match preflight_error_for_stage(ctx, "Backend: ") {
        Err(()) => vec![Assertion::out_of_scope(NAME)],
        Ok(err) => vec![assert_that(
            NAME,
            err.contains("backend at")
                && (err.contains("did not answer") || err.contains("answered with status")),
        )],
    }
}

// ---------------------------------------------------------------------------
// S7 — a saturated endpoint reports "cannot test", with its declared scope
// ---------------------------------------------------------------------------

/// `S7` — a saturated endpoint reports "cannot test", with its declared scope
/// (`sbtdd/smoke-harness-spec.md`, "S7").
///
/// `Source::Preflight`: the contention probe's own `Probe` stage. Exercised
/// by pointing `MAGI_SMOKE_ENDPOINT` at a stub that answers slowly
/// (six-invocation table, README).
///
/// # Declared limitation of THIS assertion, not just of the probe it reads
///
/// `preflight::probe_failure_message()`'s RUSTDOC promises two limitations —
/// it cannot tell MAGI from other contention, and it cannot see contention
/// that starts AFTER the preflight — but only the FIRST is present in the
/// RUNTIME string the function actually returns (verified against
/// `smoke/src/preflight.rs`): *"the harness cannot tell them apart:
/// contention ... or a cold model still loading"*. The second limitation is
/// documented only in that function's doc comment, never printed. This
/// assertion checks what the message actually SAYS, not what its doc comment
/// promises: asserting text that the runtime string does not contain would
/// make this scenario permanently red for a reason inside `preflight.rs`,
/// which is outside this task's scope. Flagged, not silently worked around.
fn s7_a_saturated_endpoint_reports_cannot_test_with_its_scope(
    ctx: &RunContext<'_>,
) -> Vec<Assertion> {
    const NAME: &str =
        "a slow-but-responding endpoint is 'cannot test', naming that contention and a cold \
         model cannot be told apart";
    match preflight_error_for_stage(ctx, "Probe: ") {
        Err(()) => vec![Assertion::out_of_scope(NAME)],
        Ok(err) => vec![assert_that(
            NAME,
            err.contains("cannot tell them apart")
                && err.contains("contention")
                && err.contains("cold"),
        )],
    }
}

// ---------------------------------------------------------------------------
// S14 — an illegible TOML is FATAL, never a silent default
// ---------------------------------------------------------------------------

/// `S14` — an illegible TOML is FATAL, never a silent default
/// (`sbtdd/smoke-harness-spec.md`, "S14").
///
/// `Source::Preflight`: a config that fails to parse never reaches
/// `preflight::run` at all — it is wrapped as the `Config` stage before the
/// preflight starts, so no scenario runs against a default backend. Exercised
/// by `cargo run -- --config <a toml carrying an UNKNOWN FIELD>` (six-invocation
/// table, README). The input shape is named precisely because it matters: a
/// SYNTAX-broken toml is also unreadable, produces a different message, and used
/// to fail this assertion — reporting a verdict about the crate for an input the
/// operator chose. The name below is narrowed to what the body actually checks.
///
/// # The stage guard is not enough on its own, which is what this scenario got wrong
///
/// [`preflight_error_for_stage`] separates `Config:` from the other three
/// stages, and there it stopped: EVERY Config-stage failure that was not an
/// unknown field fell through to the assertion and came out `Fail`. A syntax
/// error, an absent file and a bad value are all illegible configs, all chosen
/// by the operator, and none of them is the shape this scenario asserts about.
/// So the shape is now part of the guard rather than part of the verdict, on the
/// same ruling the stage prefix already follows: a fault this invocation did not
/// ask about is `OutOfScope`, never a red row about the crate.
///
/// # Declared limitation of this assertion
///
/// It reads the preflight's error string and nothing else, so it cannot see the
/// config that produced it. The regression of an unknown field being SILENTLY
/// DEFAULTED therefore arrives here as no error at all, which is `OutOfScope` —
/// indistinguishable from a run that passed no config. Closing that would need
/// the harness to compare the loaded config against the file it came from, which
/// belongs to `config.rs` rather than to a scenario. Flagged, not worked around.
fn s14_illegible_toml_is_fatal(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME: &str =
        "a config with an unknown field names it and cuts with exit 2 before any run";
    match preflight_error_for_stage(ctx, "Config: ") {
        Err(()) => vec![Assertion::out_of_scope(NAME)],
        // The config was rejected for something other than an unrecognised key,
        // so the invocation induced a different fault from this one.
        Ok(err) if !err.contains(UNKNOWN_FIELD) => vec![Assertion::out_of_scope(NAME)],
        // It cut at the `Config` stage, which is before any run by
        // construction — the stage prefix proves that half. What is left to
        // check is the other half of the sentence, and it is the half an
        // operator acts on: WHICH field.
        Ok(err) => vec![assert_that(NAME, names_the_offending_field(err))],
    }
}

/// What `toml`'s own rejection of an unrecognised key says. The shape `S14`
/// asserts about, and — since the ruling in [`s14_illegible_toml_is_fatal`] —
/// the shape that selects it at all.
const UNKNOWN_FIELD: &str = "unknown field";

/// Whether an unknown-field rejection goes on to NAME the field, the way
/// `toml` renders it: ``unknown field `some_key` ``.
///
/// Selecting on [`UNKNOWN_FIELD`] alone would leave `S14` unable to fail at
/// all, since every message it still judges would already contain the marker
/// that selected it — an assertion that cannot go red is the defect this
/// milestone keeps closing. The field name is what makes the check real, and it
/// is also the only part of that message an operator can act on: "unknown
/// field" tells them the config is wrong, and the backticked name tells them
/// where.
///
/// # Parameters
///
/// * `err` — the rendered preflight error, already known to contain
///   [`UNKNOWN_FIELD`].
///
/// # Complexity
///
/// `O(n)` in `err.len()`.
fn names_the_offending_field(err: &str) -> bool {
    /// The delimiter `toml` puts around the key it did not recognise.
    const QUOTE: char = '`';
    err.split_once(UNKNOWN_FIELD)
        .and_then(|(_, tail)| tail.split_once(QUOTE))
        .and_then(|(_, named)| named.split_once(QUOTE))
        // A backtick pair with nothing between it names no more than the bare
        // marker did.
        .is_some_and(|(name, _)| !name.trim().is_empty())
}

// ---------------------------------------------------------------------------
// S15 — degradation is honest in its FOUR assertions, forced by injection
// ---------------------------------------------------------------------------

/// `degraded` is set at all.
const S15_NAME_DEGRADED: &str = "degraded is true";

/// The failed seat is the one the harness asked to fail, not merely some seat.
const S15_NAME_AGENT: &str = "failed_agents names the INJECTED agent, not just any agent";

/// The contributor count is the one that ANSWERED, not the one dispatched.
const S15_NAME_COUNT: &str = "agent_count counts those that RESPONDED, not those launched";

/// Degraded mode caps a STRONG label down to its regular form.
const S15_NAME_LABEL: &str = "no STRONG label survives a 2/3 consensus";

/// The four names in the order [`s15_four_assertions`] emits them, so the skip
/// path cannot list a different set — or a differently ordered one — from the
/// assert path. They were written out twice before, and two lists of the same
/// four strings is one edit away from disagreeing.
const S15_NAMES: [&str; 4] = [
    S15_NAME_DEGRADED,
    S15_NAME_AGENT,
    S15_NAME_COUNT,
    S15_NAME_LABEL,
];

/// All four of `S15`'s assertions as `Skip`s carrying one shared reason.
///
/// # Parameters
///
/// * `reason` — what stopped the four from being evaluated, in terms an
///   operator can act on.
fn s15_skips(reason: impl Into<String>) -> Vec<Assertion> {
    let reason = reason.into();
    S15_NAMES
        .iter()
        .map(|&name| Assertion::skip(name, reason.clone()))
        .collect()
}

/// The four assertions, transcribed VERBATIM from the spec (task brief,
/// Checkpoint 2 loop 1: the three field paths below were corrected against
/// the tree — `report.agent_count`, `report.successful_agents()` and
/// `report.consensus.label` do NOT exist in `magi-core` `3.2.0`; the correct
/// paths are `report.consensus.agent_count`, `report.agents.len()` and
/// `report.consensus.consensus`).
///
/// Kept as its own function, taking `report` directly, so the four
/// assertions read as one block matching the spec verbatim;
/// [`s15_degradation_is_honest`] adapts it to the `assert_fn` signature and
/// covers the "no report" case.
fn s15_four_assertions(report: &MagiReport, ctx: &RunContext<'_>) -> Vec<Assertion> {
    vec![
        assert_that(S15_NAME_DEGRADED, report.degraded),
        assert_that(
            S15_NAME_AGENT,
            ctx.injected_agent
                .is_some_and(|a| report.failed_agents.contains_key(&a))
                && report.failed_agents.values().all(|r| !r.is_empty()),
        ),
        // (3) and (4) are the subtle ones: no unit test notices them, because
        // they require the complete run.
        //
        // `consensus.agent_count` is documented as "agents that CONTRIBUTED",
        // and `report.agents` holds exactly the ones that produced output.
        // With one seat down, both must read 2 while three were launched.
        // Comparing the two is what catches a count that drifts back to "how
        // many we dispatched".
        assert_that(
            S15_NAME_COUNT,
            report.consensus.agent_count == report.agents.len() && report.agents.len() == 2,
        ),
        // The label field is `consensus`, a String such as "GO (2-0)".
        // Degraded mode caps STRONG labels down to their regular form.
        assert_that(
            S15_NAME_LABEL,
            !report.consensus.consensus.contains("STRONG"),
        ),
    ]
}

/// `S15` — degradation is honest in its FOUR assertions, forced by injection
/// (`sbtdd/smoke-harness-spec.md`, "S15").
///
/// Reads [`RunId::Degradation`]. This is the property the "a degraded MAGI run
/// never approves a gate" discipline rests on
/// (`CLAUDE.local.md`, "Integridad de MAGI"), so every field is checked
/// individually rather than folded into one boolean — a red row must say
/// WHICH of the four broke.
///
/// # Its wire precondition, which it did not have
///
/// All four assertions presuppose that a seat was knocked out. Without a guard,
/// an injection that silently failed to fire left them reading a report that was
/// never going to be degraded — `degraded` false, three agents present — and
/// produced four red rows: exit 1, a verdict about the crate, for something the
/// crate was never asked to do. That is the inversion `S4`'s wire assertion
/// closed one scenario over, and the fix here is the same shape:
///
/// * the proxy degraded → `Skip`. The record that would establish the
///   precondition is the very thing that went partial, so whether the failure
///   this report shows is the one we forced cannot be settled.
/// * the run never reached the wire, or nothing was injected → `Skip`, from
///   [`why_the_wire_cannot_answer`].
/// * traffic happened and the injection never fired → `Skip`, from
///   [`why_the_forced_failure_cannot_be_read`]. **This is where `S15` and `S4`
///   differ on purpose**: for `S4`'s wire assertion that case is the finding and
///   FAILS, because its subject IS the firing; here the firing is only the
///   precondition, so it cannot be the answer.
/// * the injection fired and the report still disagrees → `Fail`, unchanged.
///   That case is what the scenario exists for, and the guard must not be able
///   to swallow it — which is what
///   `s15_still_fails_when_the_injection_fired_and_the_report_is_healthy` pins.
fn s15_degradation_is_honest(ctx: &RunContext<'_>) -> Vec<Assertion> {
    if ctx.proxy_degraded {
        return s15_skips(
            "the proxy registry degraded during this run, so whether the failure this report \
             shows is the one we forced cannot be established",
        );
    }
    if let Some(reason) = why_the_forced_failure_cannot_be_read(ctx) {
        return s15_skips(reason);
    }
    match ctx.report {
        Some(report) => s15_four_assertions(report, ctx),
        // Reached only when the injection DID fire and `analyze()` still
        // produced no report: a typed crate failure, whose text is the reason.
        None => s15_skips(
            ctx.error
                .map(str::to_string)
                .unwrap_or_else(|| "the degradation run never happened".to_string()),
        ),
    }
}

// ---------------------------------------------------------------------------
// S16 — the harness leaves no trace in the repo, except the certificate
// ---------------------------------------------------------------------------

/// Whether every line of `git status --porcelain` output names ONLY the fixed
/// certificate path. Split out as a pure function so it is testable without a
/// real `git` invocation; [`s16_no_trace_left_in_the_repo`] supplies the real
/// command.
fn status_shows_nothing_outside_the_certificate(porcelain: &str) -> bool {
    porcelain
        .lines()
        .all(|l| l.trim_end().ends_with(CERT_PATH_SUFFIX))
}

/// `S16` — the harness leaves no trace in the repo, except the certificate
/// (`sbtdd/smoke-harness-spec.md`, "S16").
///
/// `Source::Session`: evaluated ONCE, after every run finished and BEFORE the
/// certificate is written — the certificate is this scenario's declared
/// exception, and it must be the ONLY line `git status` can show. Checking
/// this AFTER writing the certificate would fail on `--smoke-2`, the one
/// invocation that writes it, i.e. it would fail exactly when everything else
/// went right; the ordering in `evaluate()` (before `Report::render_certificate`)
/// is what this scenario depends on.
///
/// **Wire-precondition audit (module doc, "The companion rule"): not needed.**
/// It reads neither `report` nor `records` — its subject is the repository, not
/// the wire — and both of the questions it does presuppose are guarded: an
/// absent baseline and a `git status` that could not be taken are `Skip`s.
fn s16_no_trace_left_in_the_repo(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME: &str = "the harness added nothing to the tree outside the certificate path";
    // The claim is a DELTA, not absolute cleanliness. Asserting the tree is
    // clean blames the harness for whatever was already uncommitted, so an
    // operator running it over work in progress gets a red naming the harness
    // for their own edits. What the harness can honestly claim is that it added
    // nothing of its own.
    let Some(before) = ctx.repo_status_before else {
        return vec![Assertion::skip(
            NAME,
            "no pre-run baseline was captured, so nothing can be attributed to the harness",
        )];
    };
    // The command comes from `crate::git`, so the flags cannot drift from the
    // ones the BASELINE was taken with — comparing two different questions is
    // how a delta stops being a delta. The POLICY stays here: a tree this
    // cannot read is a question this scenario cannot answer, which is a `Skip`
    // carrying git's own message, never a verdict about the crate.
    let after = match crate::git::status_porcelain(&crate::paths::repo_root()) {
        Ok(a) => a,
        Err(e) => return vec![Assertion::skip(NAME, e)],
    };
    let baseline: std::collections::BTreeSet<&str> = before.lines().collect();
    let added: String = after
        .lines()
        .filter(|l| !baseline.contains(l))
        .collect::<Vec<_>>()
        .join("\n");
    vec![assert_that(
        NAME,
        status_shows_nothing_outside_the_certificate(&added),
    )]
}

// ---------------------------------------------------------------------------
// S20 — a broken proxy does NOT produce a scenario red
// ---------------------------------------------------------------------------

/// `S20` — a broken proxy does NOT produce a scenario red
/// (`sbtdd/smoke-harness-spec.md`, "S20").
///
/// `Source::Preflight`: `preflight::raise_proxy` fails the `Proxy` step.
/// Exercised by `cargo run -- --break-proxy`, the harness's own self-test
/// hook (`preflight.rs`).
fn s20_broken_proxy_is_not_a_scenario_red(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME: &str =
        "a proxy that cannot start is a harness fault, never a crate verdict, and fails no \
         scenario";
    match preflight_error_for_stage(ctx, "Proxy: ") {
        Err(()) => vec![Assertion::out_of_scope(NAME)],
        Ok(err) => vec![assert_that(
            NAME,
            err.contains("HARNESS fault") && err.contains("never a verdict about the crate"),
        )],
    }
}

// ---------------------------------------------------------------------------
// S21 — the two dependency modes cannot be confused
// ---------------------------------------------------------------------------

/// `S21` — the two dependency modes cannot be confused
/// (`sbtdd/smoke-harness-spec.md`, "S21").
///
/// `Source::Build`: the property is a `compile_error!` in `alias.rs`, so there
/// is no BINARY to observe it from at runtime — a scenario that runs has
/// already compiled. Reads `ctx.build_matrix`, populated only under
/// `--build-matrix`; without it, never PASS (R25).
///
/// # Out of scope without the flag, but SKIP when the flag was given
///
/// The two are different facts and the exit code separates them. No matrix at
/// all means the invocation did not ask for one — four `cargo check` runs are
/// slow, which is why they sit behind a flag — and a run that was not asked is
/// not a run that failed: `OutOfScope`, exit 0. A matrix that WAS asked for and
/// whose `cargo` could not be spawned is the harness trying and failing to
/// test: `Skip`, exit 2, which is a fault worth someone's attention.
///
/// **THREE of the four combinations are asserted, and `published` alone is
/// not**, for a narrow reason: this scenario's property is that the two modes
/// cannot be CONFUSED, and the three asserted combinations establish exactly
/// that — `tree` builds, both together do not, neither does. Whether `published`
/// builds on its own is a different claim, that the published mode WORKS, and it
/// depends on crates.io being reachable: a fact about the machine rather than
/// about the crate under test.
///
/// It is **not** left out for want of a version to resolve. `smoke/Cargo.toml`
/// pins `magi_core_pub` at `version = "3.2"`, which is on crates.io, so the
/// combination resolves and compiles today — and it must, since cargo resolves
/// an optional dependency whether or not its feature is on, so a version that
/// did not exist would break every build of this package rather than only this
/// one combination.
///
/// **Wire-precondition audit (module doc, "The companion rule"): not needed.**
/// It reads the build matrix, never `report` or `records`; a matrix nobody asked
/// for is already `OutOfScope`, and a `cargo` that could not be run is already a
/// `Skip`. The section above is where those two are told apart, and neither is a
/// `Fail`, which is all this audit needs.
fn s21_the_two_modes_cannot_be_confused(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME: &str = "the two dependency modes cannot be confused";
    let Some(m) = ctx.build_matrix else {
        return vec![Assertion::out_of_scope(NAME)];
    };
    // A combination `cargo` could not even be RUN for is not a combination that
    // refused to compile. Reading the two the same way would report a missing
    // toolchain as the very regression this scenario exists to catch.
    let asserted = ["tree", "tree,published", ""];
    let unrunnable: Vec<&str> = asserted
        .into_iter()
        .filter(|name| {
            m.iter()
                .any(|(c, o)| c == name && *o == BuildOutcome::CouldNotRun)
        })
        .collect();
    if !unrunnable.is_empty() {
        return vec![Assertion::skip(
            NAME,
            format!(
                "cargo could not be run for {unrunnable:?}, so nothing was learned about \
                 those combinations either way"
            ),
        )];
    }
    let outcome = |name: &str, want: BuildOutcome| m.iter().any(|(c, o)| c == name && *o == want);
    vec![assert_that(
        NAME,
        outcome("tree", BuildOutcome::Built)
            && outcome("tree,published", BuildOutcome::DidNotBuild)
            && outcome("", BuildOutcome::DidNotBuild),
    )]
}

// ---------------------------------------------------------------------------
// The set
// ---------------------------------------------------------------------------

/// The twelve scenarios valid against `3.2.0`, in the EXACT order the spec's
/// `§2.3` assigns to stage E1. Order is part of the contract:
/// `e1_contains_exactly_the_scenarios_valid_against_3_2_0` pins it.
pub fn e1_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            id: "S1",
            source: Source::Run(RunId::NoBackend),
            backend_tag: BackendNeed::None,
            assert_fn: s1_external_provider_fails_typed,
        },
        Scenario {
            id: "S2",
            source: Source::Run(RunId::HappySmall),
            backend_tag: BackendNeed::Required,
            assert_fn: s2_happy_path_against_real_backend,
        },
        Scenario {
            id: "S2b",
            source: Source::Run(RunId::HappySmall),
            backend_tag: BackendNeed::Required,
            assert_fn: s2b_the_proxy_is_transparent,
        },
        Scenario {
            id: "S4",
            source: Source::Run(RunId::Rotation),
            backend_tag: BackendNeed::Required,
            assert_fn: s4_rotation_and_its_cause,
        },
        Scenario {
            id: "S5",
            source: Source::Run(RunId::HappySmall),
            backend_tag: BackendNeed::Required,
            assert_fn: s5_the_probe_still_reads_what_it_expects,
        },
        Scenario {
            id: "S6",
            source: Source::Preflight,
            backend_tag: BackendNeed::None,
            assert_fn: s6_no_backend_no_green,
        },
        Scenario {
            id: "S7",
            source: Source::Preflight,
            backend_tag: BackendNeed::None,
            assert_fn: s7_a_saturated_endpoint_reports_cannot_test_with_its_scope,
        },
        Scenario {
            id: "S14",
            source: Source::Preflight,
            backend_tag: BackendNeed::None,
            assert_fn: s14_illegible_toml_is_fatal,
        },
        Scenario {
            id: "S15",
            source: Source::Run(RunId::Degradation),
            backend_tag: BackendNeed::Required,
            assert_fn: s15_degradation_is_honest,
        },
        Scenario {
            id: "S16",
            source: Source::Session,
            backend_tag: BackendNeed::None,
            assert_fn: s16_no_trace_left_in_the_repo,
        },
        Scenario {
            id: "S20",
            source: Source::Preflight,
            backend_tag: BackendNeed::None,
            assert_fn: s20_broken_proxy_is_not_a_scenario_red,
        },
        Scenario {
            id: "S21",
            source: Source::Build,
            backend_tag: BackendNeed::None,
            assert_fn: s21_the_two_modes_cannot_be_confused,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alias::magi_core::schema::AgentName;
    use crate::outcome::ScenarioState;

    // -- shared fixtures --

    fn blank_ctx(run: RunId) -> RunContext<'static> {
        RunContext {
            run,
            report: None,
            error: None,
            records: &[],
            proxy_degraded: false,
            attempts: 1,
            over_budget: None,
            direct_probe_body: None,
            direct_probe_status: None,
            probe_record: None,
            probe_sent_body: None,
            injected_agent: None,
            build_matrix: None,
            repo_status_before: None,
        }
    }

    fn record(path: &str, status: u16) -> RequestRecord {
        RequestRecord {
            path: path.to_string(),
            body_sha256: String::new(),
            response_status: status,
            response_recorded: false,
            response_sha256: String::new(),
            response_body: Vec::new(),
        }
    }

    fn recorded_response(path: &str, status: u16, body: &[u8]) -> RequestRecord {
        RequestRecord {
            response_recorded: true,
            response_body: body.to_vec(),
            response_sha256: sha256_hex(body),
            ..record(path, status)
        }
    }

    const HEALTHY_REPORT_JSON: &str = r#"{
      "agents": [
        {"agent":"melchior","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"},
        {"agent":"balthasar","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"},
        {"agent":"caspar","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"}
      ],
      "consensus": {
        "consensus":"STRONG GO","consensus_verdict":"approve","confidence":0.95,"score":1.0,
        "agent_count":3,"votes":{},"majority_summary":"","dissent":[],"findings":[],"conditions":[],"recommendations":{}
      },
      "banner":"","report":"","degraded":false,"failed_agents":{}
    }"#;

    const DEGRADED_REPORT_JSON: &str = r#"{
      "agents": [
        {"agent":"melchior","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"},
        {"agent":"balthasar","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"}
      ],
      "consensus": {
        "consensus":"GO (2-0)","consensus_verdict":"approve","confidence":0.85,"score":0.85,
        "agent_count":2,"votes":{},"majority_summary":"","dissent":[],"findings":[],"conditions":[],"recommendations":{}
      },
      "banner":"","report":"","degraded":true,"failed_agents":{"caspar":"transport: injected failure"}
    }"#;

    /// A degraded report whose `consensus` label carries an ILLEGITIMATE
    /// `STRONG` — the case assertion (4) must reject even though everything
    /// else about it looks like S15's happy path.
    const DEGRADED_REPORT_WITH_STRONG_LABEL_JSON: &str = r#"{
      "agents": [
        {"agent":"melchior","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"},
        {"agent":"balthasar","verdict":"approve","confidence":0.9,"summary":"s","reasoning":"r","findings":[],"recommendation":"go"}
      ],
      "consensus": {
        "consensus":"STRONG GO","consensus_verdict":"approve","confidence":0.85,"score":0.85,
        "agent_count":2,"votes":{},"majority_summary":"","dissent":[],"findings":[],"conditions":[],"recommendations":{}
      },
      "banner":"","report":"","degraded":true,"failed_agents":{"caspar":"transport: injected failure"}
    }"#;

    fn report_from(json: &str) -> MagiReport {
        serde_json::from_str(json).expect("test fixture JSON must deserialize into MagiReport")
    }

    // -- structural tests (Step 1 of the task brief) --

    // `every_scenario_declares_whether_it_needs_a_backend` USED to live here.
    // It is gone because `Scenario::backend_tag` stopped being an `Option`: all
    // twelve wrote `Some(..)`, so the `None` layer had no consumer and the test
    // asserted over a case nothing could produce. The property is now enforced
    // by the type — a scenario without a tag does not compile — which is
    // strictly stronger than a test that runs.

    #[test]
    fn the_no_backend_partition_is_neither_empty_nor_everything() {
        // What the deleted test was reaching for, stated as a property the type
        // cannot enforce: if every scenario needed a backend, `--no-backend`
        // would report twelve OutOfScope rows and test nothing; if none did,
        // the flag would be inert.
        let tags: Vec<BackendNeed> = e1_scenarios().iter().map(|s| s.backend_tag).collect();
        assert!(tags.contains(&BackendNeed::None));
        assert!(tags.contains(&BackendNeed::Required));
    }

    #[test]
    fn e1_contains_exactly_the_scenarios_valid_against_3_2_0() {
        let ids: Vec<_> = e1_scenarios().iter().map(|s| s.id).collect();
        assert_eq!(
            ids,
            ["S1", "S2", "S2b", "S4", "S5", "S6", "S7", "S14", "S15", "S16", "S20", "S21"]
        );
    }

    // -- S1 --

    #[test]
    fn s1_skips_rather_than_passes_when_no_error_was_recorded() {
        let ctx = blank_ctx(RunId::NoBackend);
        let a = s1_external_provider_fails_typed(&ctx);
        assert_eq!(a.len(), 1);
        assert!(matches!(a[0].state, ScenarioState::Skip(_)));
    }

    #[test]
    fn s1_passes_on_a_genuine_insufficient_agents_error() {
        let err = "insufficient agents: 0 succeeded, 2 required".to_string();
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::NoBackend)
        };
        let a = s1_external_provider_fails_typed(&ctx);
        assert_eq!(a[0].state, ScenarioState::Pass);
    }

    #[test]
    fn s1_fails_on_a_run_wide_endpoint_down_condemnation() {
        // Proves the check is not vacuous: an error IS present, but it is the
        // WRONG kind, and the assertion must say so.
        let err = "endpoint down: no lineage reachable (a, b)".to_string();
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::NoBackend)
        };
        let a = s1_external_provider_fails_typed(&ctx);
        assert_eq!(a[0].state, ScenarioState::Fail);
    }

    // -- S2 --

    #[test]
    fn s2_skips_all_four_when_there_is_no_report() {
        let ctx = blank_ctx(RunId::HappySmall);
        let a = s2_happy_path_against_real_backend(&ctx);
        assert_eq!(a.len(), 4);
        assert!(a.iter().all(|x| matches!(x.state, ScenarioState::Skip(_))));
    }

    #[test]
    fn s2_passes_on_a_healthy_report_with_real_completions() {
        let report = report_from(HEALTHY_REPORT_JSON);
        let records = vec![record(COMPLETIONS_PATH, 200), record(COMPLETIONS_PATH, 200)];
        let ctx = RunContext {
            report: Some(&report),
            records: &records,
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s2_happy_path_against_real_backend(&ctx);
        assert!(a.iter().all(|x| x.state == ScenarioState::Pass), "{a:?}");
    }

    #[test]
    fn s2_fails_the_injection_check_when_no_completion_was_ever_seen() {
        // The vacuous-pass trap: `.all()` over an empty set is trivially
        // true, so the non-empty guard must be what actually decides this.
        let report = report_from(HEALTHY_REPORT_JSON);
        let ctx = RunContext {
            report: Some(&report),
            records: &[],
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s2_happy_path_against_real_backend(&ctx);
        assert_eq!(a[3].state, ScenarioState::Fail);
    }

    #[test]
    fn s2_fails_the_injection_check_when_a_completion_got_the_injected_status() {
        let report = report_from(HEALTHY_REPORT_JSON);
        let records = vec![
            record(COMPLETIONS_PATH, 200),
            record(COMPLETIONS_PATH, INJECTED_FAILURE_STATUS),
        ];
        let ctx = RunContext {
            report: Some(&report),
            records: &records,
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s2_happy_path_against_real_backend(&ctx);
        assert_eq!(a[3].state, ScenarioState::Fail);
    }

    #[test]
    fn s2_degraded_check_fails_on_a_degraded_report() {
        let report = report_from(DEGRADED_REPORT_JSON);
        let ctx = RunContext {
            report: Some(&report),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s2_happy_path_against_real_backend(&ctx);
        assert_eq!(a[1].state, ScenarioState::Fail);
    }

    // -- S2b --

    /// The status both halves of a healthy probe carry.
    const PROBE_OK_STATUS: u16 = 200;

    #[test]
    fn s2b_skips_when_the_probe_never_completed() {
        let ctx = blank_ctx(RunId::HappySmall);
        let a = s2b_the_proxy_is_transparent(&ctx);
        assert_eq!(a.len(), 3);
        assert!(a.iter().all(|x| matches!(x.state, ScenarioState::Skip(_))));
    }

    #[test]
    fn s2b_skips_when_the_direct_half_left_no_status_to_compare_against() {
        // Half a probe compares against a term that is not there. Failing on it
        // would report a transparency difference the proxy never introduced.
        let sent = "the probe body".to_string();
        let direct = b"the probe response".to_vec();
        let rec = recorded_response(SHOW_PATH, PROBE_OK_STATUS, &direct);
        let rec = RequestRecord {
            body_sha256: sha256_hex(sent.as_bytes()),
            ..rec
        };
        let ctx = RunContext {
            probe_record: Some(&rec),
            probe_sent_body: Some(&sent),
            direct_probe_body: Some(&direct),
            direct_probe_status: None,
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s2b_the_proxy_is_transparent(&ctx);
        assert!(
            a.iter().all(|x| matches!(x.state, ScenarioState::Skip(_))),
            "{a:?}"
        );
    }

    #[test]
    fn s2b_fails_when_the_relayed_status_differs_from_the_one_the_backend_gave() {
        // The gap this closes: a proxy that relayed the right bytes under a
        // DIFFERENT status satisfied both checksums, while changing exactly what
        // the crate classifies on. The spec asked for this comparison; the code
        // did not have it.
        let sent = "the probe body".to_string();
        let direct = b"the probe response".to_vec();
        let rec = recorded_response(SHOW_PATH, 500, &direct);
        let rec = RequestRecord {
            body_sha256: sha256_hex(sent.as_bytes()),
            ..rec
        };
        let ctx = RunContext {
            probe_record: Some(&rec),
            probe_sent_body: Some(&sent),
            direct_probe_body: Some(&direct),
            direct_probe_status: Some(PROBE_OK_STATUS),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s2b_the_proxy_is_transparent(&ctx);
        assert_eq!(a[0].state, ScenarioState::Pass, "the bodies still match");
        assert_eq!(a[1].state, ScenarioState::Pass, "the bodies still match");
        assert_eq!(a[2].state, ScenarioState::Fail, "{a:?}");
    }

    #[test]
    fn s2b_skips_when_the_proxy_degraded_instead_of_blaming_the_crate() {
        // A failed body read is recorded as an EMPTY body, so without this gate
        // the checksum compares a hash of nothing and reports FAIL — exit 1, a
        // verdict about the crate, for a harness fault.
        let sent = "the probe body".to_string();
        let direct = b"the probe response".to_vec();
        let rec = recorded_response(SHOW_PATH, PROBE_OK_STATUS, &direct);
        let rec = RequestRecord {
            body_sha256: sha256_hex(b""),
            ..rec
        };
        let ctx = RunContext {
            probe_record: Some(&rec),
            probe_sent_body: Some(&sent),
            direct_probe_body: Some(&direct),
            direct_probe_status: Some(PROBE_OK_STATUS),
            proxy_degraded: true,
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s2b_the_proxy_is_transparent(&ctx);
        assert!(
            a.iter().all(|x| matches!(x.state, ScenarioState::Skip(_))),
            "a degraded proxy must not be reported as a transparency failure: {a:?}"
        );
    }

    #[test]
    fn s2b_passes_when_both_hashes_and_the_status_match() {
        let sent = "the probe body".to_string();
        let direct = b"the probe response".to_vec();
        let rec = recorded_response(SHOW_PATH, PROBE_OK_STATUS, &direct);
        let rec = RequestRecord {
            body_sha256: sha256_hex(sent.as_bytes()),
            ..rec
        };
        let ctx = RunContext {
            probe_record: Some(&rec),
            probe_sent_body: Some(&sent),
            direct_probe_body: Some(&direct),
            direct_probe_status: Some(PROBE_OK_STATUS),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s2b_the_proxy_is_transparent(&ctx);
        assert!(a.iter().all(|x| x.state == ScenarioState::Pass), "{a:?}");
    }

    #[test]
    fn s2b_fails_when_the_recorded_request_body_hash_disagrees() {
        let sent = "the probe body".to_string();
        let direct = b"the probe response".to_vec();
        // `body_sha256` is deliberately left at its default (the empty-body
        // hash), which will not match `sha256_hex(sent)` — proving the
        // comparison is real, not vacuously true.
        let rec = recorded_response(SHOW_PATH, PROBE_OK_STATUS, &direct);
        let ctx = RunContext {
            probe_record: Some(&rec),
            probe_sent_body: Some(&sent),
            direct_probe_body: Some(&direct),
            direct_probe_status: Some(PROBE_OK_STATUS),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s2b_the_proxy_is_transparent(&ctx);
        assert_eq!(a[0].state, ScenarioState::Fail);
    }

    // -- S4 --

    /// A minimal, valid `MagiReport` (empty agents/consensus — S4 never reads
    /// them) carrying ONE `rotations` entry for Caspar, whose `chain` is
    /// exactly `chain_json`. Shared by all three "distinguish the three
    /// cases" tests below so each differs only in the chain, never in the
    /// surrounding shape.
    fn report_with_caspar_rotation_chain(chain_json: &str) -> MagiReport {
        let json = format!(
            r#"{{
              "agents": [],
              "consensus": {{
                "consensus":"GO","consensus_verdict":"approve","confidence":0.5,"score":0.5,
                "agent_count":0,"votes":{{}},"majority_summary":"","dissent":[],"findings":[],"conditions":[],"recommendations":{{}}
              }},
              "banner":"","report":"","degraded":false,"failed_agents":{{}},
              "rotations": {{
                "caspar": {{
                  "model_configured":"glm-5.2:cloud","model_used":"deepseek-v4-pro:cloud",
                  "chain": {chain_json},
                  "ran_unmeasured": false
                }}
              }}
            }}"#
        );
        report_from(&json)
    }

    fn s4_ctx_with_rotation<'a>(
        report: &'a MagiReport,
        records: &'a [RequestRecord],
    ) -> RunContext<'a> {
        RunContext {
            report: Some(report),
            records,
            injected_agent: Some(AgentName::Caspar),
            ..blank_ctx(RunId::Rotation)
        }
    }

    #[test]
    fn s4_rotated_check_passes_when_the_hop_lands_on_a_different_lineage() {
        let report = report_with_caspar_rotation_chain(
            r#"[{"from":"zhipu","to":"deepseek","model_resolved":"deepseek-v4-pro:cloud","kind":"transport","detail":"d"}]"#,
        );
        let ctx = s4_ctx_with_rotation(&report, &[]);
        let a = s4_rotation_and_its_cause(&ctx);
        assert_eq!(a[1].state, ScenarioState::Pass, "{a:?}");
    }

    #[test]
    fn s4_rotated_check_fails_when_the_chain_is_empty_did_not_rotate() {
        // Case 1 of 3: the injected agent never left its primary lineage.
        // This must NOT read as "the property could not be tested" — real
        // rotation telemetry exists and says it did not happen.
        let report = report_with_caspar_rotation_chain("[]");
        let ctx = s4_ctx_with_rotation(&report, &[]);
        let a = s4_rotation_and_its_cause(&ctx);
        assert_eq!(a[1].state, ScenarioState::Fail, "{a:?}");
    }

    #[test]
    fn s4_rotated_check_fails_when_the_hop_lands_on_the_same_lineage() {
        // Case 2 of 3: a hop exists, but `to == from` — not a rotation to a
        // DIFFERENT candidate, and an assertion that could not tell the
        // difference is the exact defect being guarded against here.
        let report = report_with_caspar_rotation_chain(
            r#"[{"from":"zhipu","to":"zhipu","model_resolved":"other-zhipu-model","kind":"transport","detail":"d"}]"#,
        );
        let ctx = s4_ctx_with_rotation(&report, &[]);
        let a = s4_rotation_and_its_cause(&ctx);
        assert_eq!(a[1].state, ScenarioState::Fail, "{a:?}");
    }

    #[test]
    fn s4_cause_check_passes_on_a_transport_classified_hop() {
        // Case 3 of 3: a hop to a genuinely different lineage, classified
        // correctly.
        let report = report_with_caspar_rotation_chain(
            r#"[{"from":"zhipu","to":"deepseek","model_resolved":"deepseek-v4-pro:cloud","kind":"transport","detail":"d"}]"#,
        );
        let ctx = s4_ctx_with_rotation(&report, &[]);
        let a = s4_rotation_and_its_cause(&ctx);
        assert_eq!(a[2].state, ScenarioState::Pass, "{a:?}");
    }

    #[test]
    fn s4_cause_check_fails_on_a_hop_misclassified_as_schema() {
        // Proves the cause check is not vacuous: a hop DOES exist, but its
        // kind is wrong for an injected HTTP 500.
        let report = report_with_caspar_rotation_chain(
            r#"[{"from":"zhipu","to":"deepseek","model_resolved":"deepseek-v4-pro:cloud","kind":"schema","detail":"d"}]"#,
        );
        let ctx = s4_ctx_with_rotation(&report, &[]);
        let a = s4_rotation_and_its_cause(&ctx);
        assert_eq!(a[2].state, ScenarioState::Fail, "{a:?}");
    }

    #[test]
    fn s4_cause_check_fails_rather_than_skips_when_nothing_rotated() {
        // No hop exists, so there is no cause to classify — that is still a
        // FAIL of "the reported cause distinguishes ...", not a Skip: the
        // property did not hold, and real data (an empty chain) said so.
        let report = report_with_caspar_rotation_chain("[]");
        let ctx = s4_ctx_with_rotation(&report, &[]);
        let a = s4_rotation_and_its_cause(&ctx);
        assert_eq!(a[2].state, ScenarioState::Fail, "{a:?}");
    }

    #[test]
    fn s4_skips_rotated_and_cause_when_there_is_no_report() {
        let ctx = blank_ctx(RunId::Rotation);
        let a = s4_rotation_and_its_cause(&ctx);
        assert!(matches!(a[1].state, ScenarioState::Skip(_)));
        assert!(matches!(a[2].state, ScenarioState::Skip(_)));
    }

    #[test]
    fn s4_skips_rotated_and_cause_when_no_agent_was_injected() {
        let report = report_with_caspar_rotation_chain("[]");
        let ctx = RunContext {
            report: Some(&report),
            ..blank_ctx(RunId::Rotation)
        };
        let a = s4_rotation_and_its_cause(&ctx);
        assert!(matches!(a[1].state, ScenarioState::Skip(_)));
        assert!(matches!(a[2].state, ScenarioState::Skip(_)));
    }

    #[test]
    fn s4_skips_rotated_and_cause_when_the_injected_agent_has_no_rotation_entry() {
        let report = report_with_caspar_rotation_chain("[]");
        let ctx = RunContext {
            report: Some(&report),
            injected_agent: Some(AgentName::Melchior), // no entry under Melchior
            ..blank_ctx(RunId::Rotation)
        };
        let a = s4_rotation_and_its_cause(&ctx);
        assert!(matches!(a[1].state, ScenarioState::Skip(_)));
        assert!(matches!(a[2].state, ScenarioState::Skip(_)));
    }

    #[test]
    fn s4_wire_check_passes_when_the_injected_status_actually_fired() {
        let records = vec![record(COMPLETIONS_PATH, INJECTED_FAILURE_STATUS)];
        let ctx = RunContext {
            records: &records,
            injected_agent: Some(AgentName::Caspar),
            ..blank_ctx(RunId::Rotation)
        };
        let a = s4_rotation_and_its_cause(&ctx);
        assert_eq!(a[0].state, ScenarioState::Pass);
    }

    #[test]
    fn s4_wire_check_fails_when_no_completion_ever_saw_the_injected_status() {
        // DIRECTION 2 of the guard's mutation proof: the run DID reach the wire,
        // the injection did not fire. That is a real finding — the guard must not
        // have turned it into a skip.
        let records = vec![record(COMPLETIONS_PATH, 200)];
        let ctx = RunContext {
            records: &records,
            injected_agent: Some(AgentName::Caspar),
            ..blank_ctx(RunId::Rotation)
        };
        let a = s4_rotation_and_its_cause(&ctx);
        assert_eq!(a[0].state, ScenarioState::Fail);
    }

    #[test]
    fn s4_wire_check_skips_rather_than_fails_when_the_run_never_reached_the_wire() {
        // DIRECTION 1: nothing was ever sent — the `Magi` would not build, the
        // proxy could not start, the run never happened. Asserting here reports
        // FAIL: exit 1, a verdict about the crate, for a run the crate never
        // entered. That inversion is the one failure this harness exists to
        // eliminate.
        let ctx = RunContext {
            records: &[],
            injected_agent: Some(AgentName::Caspar),
            ..blank_ctx(RunId::Rotation)
        };
        let a = s4_rotation_and_its_cause(&ctx);
        match &a[0].state {
            ScenarioState::Skip(reason) => assert!(
                reason.contains("never reached a completion request"),
                "the skip must name what was missing: {reason:?}"
            ),
            other => panic!("expected a Skip, got {other:?}"),
        }
    }

    #[test]
    fn s4_wire_skip_carries_the_typed_failure_the_run_ended_with() {
        // A crate error that arrives BEFORE any dispatch is still "never reached
        // the wire" — but the operator needs to know which one, or the skip sends
        // them looking for a proxy fault that is not there.
        let err = "endpoint down: no lineage reachable (a, b)".to_string();
        let ctx = RunContext {
            records: &[],
            error: Some(&err),
            injected_agent: Some(AgentName::Caspar),
            ..blank_ctx(RunId::Rotation)
        };
        let a = s4_rotation_and_its_cause(&ctx);
        match &a[0].state {
            ScenarioState::Skip(reason) => assert!(
                reason.contains("endpoint down"),
                "the skip must carry what the run ended with: {reason:?}"
            ),
            other => panic!("expected a Skip, got {other:?}"),
        }
    }

    #[test]
    fn s4_wire_check_skips_when_nothing_was_injected_instead_of_blaming_the_crate() {
        // Its two sibling assertions already skipped on this; the wire one
        // failed, which is the same inconsistency one level down. And with no
        // injection configured, a GENUINE backend 500 would have satisfied the
        // assertion by coincidence — exactly what its own name disclaims.
        let records = vec![record(COMPLETIONS_PATH, INJECTED_FAILURE_STATUS)];
        let ctx = RunContext {
            records: &records,
            injected_agent: None,
            ..blank_ctx(RunId::Rotation)
        };
        let a = s4_rotation_and_its_cause(&ctx);
        match &a[0].state {
            ScenarioState::Skip(reason) => assert!(
                reason.contains("no agent was injected"),
                "the skip must name the absent injection: {reason:?}"
            ),
            other => panic!("expected a Skip, got {other:?}"),
        }
    }

    // -- S5 --

    #[test]
    fn s5_skips_both_when_no_probe_traffic_was_recorded() {
        let ctx = blank_ctx(RunId::HappySmall);
        let a = s5_the_probe_still_reads_what_it_expects(&ctx);
        assert_eq!(a.len(), 2);
        assert!(a.iter().all(|x| matches!(x.state, ScenarioState::Skip(_))));
    }

    #[test]
    fn s5_passes_on_a_well_formed_show_and_tags_response() {
        let show_body = br#"{"model_info":{"gemma4.context_length":262144}}"#.to_vec();
        let tags_body = br#"{"models":[{"name":"m","digest":"4eb23ef187e2c5462566d6a1d3bbbc2f1346d0b4327cbb66d58fffbcc9b2b05c"}]}"#.to_vec();
        let records = vec![
            recorded_response("/api/show", 200, &show_body),
            recorded_response("/api/tags", 200, &tags_body),
        ];
        let ctx = RunContext {
            records: &records,
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s5_the_probe_still_reads_what_it_expects(&ctx);
        assert!(a.iter().all(|x| x.state == ScenarioState::Pass), "{a:?}");
    }

    #[test]
    fn s5_fails_rather_than_skips_when_the_api_shape_no_longer_has_context_length() {
        let show_body = br#"{"model_info":{"gemma4.something_else":1}}"#.to_vec();
        let records = vec![recorded_response("/api/show", 200, &show_body)];
        let ctx = RunContext {
            records: &records,
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s5_the_probe_still_reads_what_it_expects(&ctx);
        assert_eq!(a[0].state, ScenarioState::Fail);
    }

    #[test]
    fn s5_fails_rather_than_skips_when_the_digest_is_not_64_hex_chars() {
        let tags_body = br#"{"models":[{"name":"m","digest":"tooshort"}]}"#.to_vec();
        let records = vec![recorded_response("/api/tags", 200, &tags_body)];
        let ctx = RunContext {
            records: &records,
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s5_the_probe_still_reads_what_it_expects(&ctx);
        assert_eq!(a[1].state, ScenarioState::Fail);
    }

    #[test]
    fn s5_notices_a_shape_change_that_only_the_second_probe_answer_shows() {
        // A run probes once per candidate. Reading only the FIRST record hid an
        // intermittent change — one model's entry losing `context_length` — behind
        // whichever answer happened to arrive first.
        let good = br#"{"model_info":{"gemma4.context_length":262144}}"#.to_vec();
        let changed = br#"{"model_info":{"gemma4.something_else":1}}"#.to_vec();
        let records = vec![
            recorded_response(SHOW_PATH, 200, &good),
            recorded_response(SHOW_PATH, 200, &changed),
        ];
        let ctx = RunContext {
            records: &records,
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s5_the_probe_still_reads_what_it_expects(&ctx);
        assert_eq!(a[0].state, ScenarioState::Fail, "{a:?}");
    }

    #[test]
    fn s5_reads_only_the_probes_the_backend_answered() {
        // A 404 for a model the backend does not hold is a fact about the
        // DEPLOYMENT: its body is an error object, which no shape check can
        // parse. Counting it would turn "a fallback model is not pulled" into a
        // red row about the crate — the 1-versus-2 inversion, arriving through
        // the config.
        let missing = br#"{"error":"model 'x' not found"}"#.to_vec();
        let good = br#"{"model_info":{"gemma4.context_length":262144}}"#.to_vec();
        let records = vec![
            recorded_response(SHOW_PATH, 404, &missing),
            recorded_response(SHOW_PATH, 200, &good),
        ];
        let ctx = RunContext {
            records: &records,
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s5_the_probe_still_reads_what_it_expects(&ctx);
        assert_eq!(a[0].state, ScenarioState::Pass, "{a:?}");
    }

    #[test]
    fn s5_skips_when_every_probe_answer_was_an_error_status() {
        // Nothing was learned about the API's shape, and a skip says so. Passing
        // here would be green by omission; failing would blame the crate for a
        // model the backend does not hold.
        let missing = br#"{"error":"model 'x' not found"}"#.to_vec();
        let records = vec![recorded_response(SHOW_PATH, 404, &missing)];
        let ctx = RunContext {
            records: &records,
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s5_the_probe_still_reads_what_it_expects(&ctx);
        assert!(matches!(a[0].state, ScenarioState::Skip(_)), "{a:?}");
    }

    // -- S6, S7, S14, S20 (preflight-sourced) --

    #[test]
    fn s6_is_out_of_scope_when_a_different_stage_failed() {
        let err = "Probe: cannot test: ...".to_string();
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s6_no_backend_no_green(&ctx);
        // The invocation induced a DIFFERENT fault, so this scenario's own
        // subject never happened. Not a question we tried and could not
        // answer — one this run was never asked.
        assert_eq!(a[0].state, ScenarioState::OutOfScope);
    }

    #[test]
    fn s6_passes_on_a_genuine_backend_stage_failure() {
        let err =
            "Backend: backend at http://127.0.0.1:1 did not answer: connection refused".to_string();
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s6_no_backend_no_green(&ctx);
        assert_eq!(a[0].state, ScenarioState::Pass);
    }

    #[test]
    fn s6_fails_on_a_backend_stage_error_missing_the_named_cause() {
        let err = "Backend: something went wrong".to_string();
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s6_no_backend_no_green(&ctx);
        assert_eq!(a[0].state, ScenarioState::Fail);
    }

    #[test]
    fn s7_passes_on_the_real_probe_failure_message() {
        let err = format!("Probe: {}", crate::preflight::probe_failure_message());
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s7_a_saturated_endpoint_reports_cannot_test_with_its_scope(&ctx);
        assert_eq!(a[0].state, ScenarioState::Pass);
    }

    #[test]
    fn s7_is_out_of_scope_when_a_different_stage_failed() {
        let err = "Backend: unreachable".to_string();
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s7_a_saturated_endpoint_reports_cannot_test_with_its_scope(&ctx);
        // The invocation induced a DIFFERENT fault, so this scenario's own
        // subject never happened. Not a question we tried and could not
        // answer — one this run was never asked.
        assert_eq!(a[0].state, ScenarioState::OutOfScope);
    }

    #[test]
    fn s14_passes_on_a_config_error_naming_an_unknown_field() {
        let err = "Config: config: TOML parse error: unknown field `totally_unknown`".to_string();
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s14_illegible_toml_is_fatal(&ctx);
        assert_eq!(a[0].state, ScenarioState::Pass);
    }

    #[test]
    fn s14_is_out_of_scope_when_a_different_stage_failed() {
        let err = "Proxy: refused to start".to_string();
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s14_illegible_toml_is_fatal(&ctx);
        // The invocation induced a DIFFERENT fault, so this scenario's own
        // subject never happened. Not a question we tried and could not
        // answer — one this run was never asked.
        assert_eq!(a[0].state, ScenarioState::OutOfScope);
    }

    #[test]
    fn s14_does_not_blame_the_crate_for_a_config_fault_of_another_shape() {
        // The gap its own rustdoc admitted and nothing closed: the stage guard
        // separates `Config:` from the other three stages, and then every
        // Config-stage failure that is not an unknown field falls through to
        // the assertion and comes out `Fail` — exit 1, a verdict about the
        // crate, for an input the OPERATOR chose. A syntax-broken toml, a
        // missing file and a bad value are all illegible configs this scenario
        // was not asked about.
        for err in [
            "Config: config: TOML parse error at line 3, column 1: expected `=`",
            "Config: config: no such file or directory",
            "Config: config: invalid type: string, expected u32",
        ] {
            let err = err.to_string();
            let ctx = RunContext {
                error: Some(&err),
                ..blank_ctx(RunId::HappySmall)
            };
            let a = s14_illegible_toml_is_fatal(&ctx);
            assert_eq!(
                a[0].state,
                ScenarioState::OutOfScope,
                "the Config stage failed in a shape this scenario does not assert about, so \
                 this invocation never asked its question: {err}"
            );
        }
    }

    #[test]
    fn s14_fails_when_the_rejection_does_not_name_the_unknown_field() {
        // Selecting the scenario on the same marker it then judges would leave
        // it unable to go red at all, and an assertion that cannot fail is the
        // defect this milestone keeps closing. This is the half that keeps it
        // real, and it is the half an operator acts on: "unknown field" says
        // the config is wrong, the backticked name says where.
        //
        // Mutation-verified: with `names_the_offending_field` replaced by a
        // bare `true` this reports `Pass` and every sibling test stays green,
        // which is why the marker alone was not a check.
        for err in [
            "Config: config: TOML parse error: unknown field",
            "Config: config: TOML parse error: unknown field ``",
            "Config: config: TOML parse error: unknown field `   `",
        ] {
            let err = err.to_string();
            let ctx = RunContext {
                error: Some(&err),
                ..blank_ctx(RunId::HappySmall)
            };
            let a = s14_illegible_toml_is_fatal(&ctx);
            assert_eq!(
                a[0].state,
                ScenarioState::Fail,
                "a rejection that names no field leaves the operator nowhere to go: {err}"
            );
        }
    }

    #[test]
    fn s20_passes_on_the_real_break_proxy_message() {
        let err = "Proxy: proxy: refused to start (--break-proxy). This is a HARNESS fault, \
                    never a verdict about the crate: no scenario is reported as passed."
            .to_string();
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s20_broken_proxy_is_not_a_scenario_red(&ctx);
        assert_eq!(a[0].state, ScenarioState::Pass);
    }

    #[test]
    fn s20_is_out_of_scope_when_a_different_stage_failed() {
        let err = "Backend: unreachable".to_string();
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s20_broken_proxy_is_not_a_scenario_red(&ctx);
        // The invocation induced a DIFFERENT fault, so this scenario's own
        // subject never happened. Not a question we tried and could not
        // answer — one this run was never asked.
        assert_eq!(a[0].state, ScenarioState::OutOfScope);
    }

    // -- S15 --

    /// The wire as it looks when the injection DID fire: one completion the
    /// backend answered, and one carrying the injected failure status.
    ///
    /// Every `S15` test that expects the four assertions to be EVALUATED passes
    /// this, because the scenario now refuses to judge degradation it cannot
    /// show was forced.
    fn a_fired_injection() -> Vec<RequestRecord> {
        vec![
            record(COMPLETIONS_PATH, 200),
            record(COMPLETIONS_PATH, INJECTED_FAILURE_STATUS),
        ]
    }

    fn s15_ctx<'a>(report: &'a MagiReport, records: &'a [RequestRecord]) -> RunContext<'a> {
        RunContext {
            report: Some(report),
            records,
            injected_agent: Some(AgentName::Caspar),
            ..blank_ctx(RunId::Degradation)
        }
    }

    #[test]
    fn s15_skips_all_four_when_there_is_no_report() {
        let ctx = blank_ctx(RunId::Degradation);
        let a = s15_degradation_is_honest(&ctx);
        assert_eq!(a.len(), 4);
        assert!(a.iter().all(|x| matches!(x.state, ScenarioState::Skip(_))));
    }

    #[test]
    fn s15_passes_all_four_on_a_correctly_degraded_report() {
        let report = report_from(DEGRADED_REPORT_JSON);
        let records = a_fired_injection();
        let a = s15_degradation_is_honest(&s15_ctx(&report, &records));
        assert!(a.iter().all(|x| x.state == ScenarioState::Pass), "{a:?}");
    }

    #[test]
    fn s15_fails_the_named_agent_check_when_a_different_agent_was_injected() {
        // Proves this is not "some agent failed" — it must be the INJECTED
        // one.
        let report = report_from(DEGRADED_REPORT_JSON);
        let records = a_fired_injection();
        let ctx = RunContext {
            injected_agent: Some(AgentName::Melchior),
            ..s15_ctx(&report, &records)
        };
        let a = s15_degradation_is_honest(&ctx);
        assert_eq!(a[1].state, ScenarioState::Fail);
    }

    #[test]
    fn s15_fails_the_strong_label_check_on_an_illegitimate_strong_label() {
        let report = report_from(DEGRADED_REPORT_WITH_STRONG_LABEL_JSON);
        let records = a_fired_injection();
        let a = s15_degradation_is_honest(&s15_ctx(&report, &records));
        assert_eq!(a[3].state, ScenarioState::Fail);
    }

    #[test]
    fn s15_still_fails_when_the_injection_fired_and_the_report_is_healthy() {
        // DIRECTION 3 of the guard's mutation proof, and the one that matters
        // most: the injection fired, the crate had every chance to degrade
        // honestly, and the report says three healthy agents. That is a verdict
        // about the crate and the guard must not be able to swallow it — a guard
        // that turned this into a Skip would have traded one blindness for
        // another.
        let report = report_from(HEALTHY_REPORT_JSON);
        let records = a_fired_injection();
        let a = s15_degradation_is_honest(&s15_ctx(&report, &records));
        assert_eq!(a[0].state, ScenarioState::Fail, "{a:?}");
    }

    #[test]
    fn s15_skips_rather_than_fails_when_the_run_never_reached_the_wire() {
        // DIRECTION 1: no completion request was ever made — the `Magi` would
        // not build, the payload could not be generated, the crate failed before
        // dispatching. The report handed in is HEALTHY, so without the guard all
        // four assertions go red: exit 1, a verdict about the crate, for a run
        // the crate never entered.
        let report = report_from(HEALTHY_REPORT_JSON);
        let a = s15_degradation_is_honest(&s15_ctx(&report, &[]));
        assert_eq!(a.len(), 4);
        for assertion in &a {
            match &assertion.state {
                ScenarioState::Skip(reason) => assert!(
                    reason.contains("never reached a completion request"),
                    "the skip must name what was missing: {reason:?}"
                ),
                other => panic!("expected a Skip, got {other:?}"),
            }
        }
    }

    #[test]
    fn s15_skips_rather_than_fails_when_the_injection_never_fired() {
        // DIRECTION 2: the run DID reach the wire — completions happened — but
        // no request came back with the injected status, so nothing forced a
        // seat down. The report is healthy because the run WAS healthy; judging
        // it as dishonest degradation would blame the crate for the harness's
        // injection not firing.
        //
        // This is deliberately the OPPOSITE of what S4's wire assertion does
        // with the same state: there, the firing is the subject and this case is
        // the finding; here it is only the precondition.
        let report = report_from(HEALTHY_REPORT_JSON);
        let records = vec![record(COMPLETIONS_PATH, 200)];
        let a = s15_degradation_is_honest(&s15_ctx(&report, &records));
        assert_eq!(a.len(), 4);
        for assertion in &a {
            match &assertion.state {
                ScenarioState::Skip(reason) => assert!(
                    reason.contains("no request carried the injected failure status"),
                    "the skip must name the injection that did not fire: {reason:?}"
                ),
                other => panic!("expected a Skip, got {other:?}"),
            }
        }
    }

    #[test]
    fn s15_skips_when_the_proxy_degraded_instead_of_judging_a_report_it_cannot_attribute() {
        // The evidence that the injection fired comes from the proxy's record,
        // so a degraded registry cannot establish the precondition — and a
        // partial record could equally well show a fired injection that never
        // reached the crate. A harness fault must not become a verdict.
        let report = report_from(HEALTHY_REPORT_JSON);
        let records = a_fired_injection();
        let ctx = RunContext {
            proxy_degraded: true,
            ..s15_ctx(&report, &records)
        };
        let a = s15_degradation_is_honest(&ctx);
        assert!(
            a.iter().all(|x| matches!(x.state, ScenarioState::Skip(_))),
            "{a:?}"
        );
    }

    #[test]
    fn s15_names_the_same_four_properties_whether_it_asserts_or_skips() {
        // The two paths used to write the four names out separately, and two
        // lists of the same four strings are one edit away from disagreeing —
        // at which point a skipped row and an asserted row would describe
        // different properties under the same scenario id.
        let report = report_from(DEGRADED_REPORT_JSON);
        let records = a_fired_injection();
        let asserted: Vec<&str> = s15_degradation_is_honest(&s15_ctx(&report, &records))
            .iter()
            .map(|a| a.name)
            .collect();
        let skipped: Vec<&str> = s15_degradation_is_honest(&blank_ctx(RunId::Degradation))
            .iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(asserted, skipped);
        assert_eq!(asserted, S15_NAMES.to_vec());
    }

    // -- S16 --

    #[test]
    fn status_helper_accepts_a_clean_tree() {
        assert!(status_shows_nothing_outside_the_certificate(""));
    }

    #[test]
    fn status_helper_accepts_only_the_certificate_changing() {
        let porcelain = " M docs/test/smoke-certificate.md\n";
        assert!(status_shows_nothing_outside_the_certificate(porcelain));
    }

    #[test]
    fn status_helper_rejects_a_stray_file_outside_the_certificate() {
        let porcelain = "?? smoke/some-leftover-temp-file\n";
        assert!(!status_shows_nothing_outside_the_certificate(porcelain));
    }

    #[test]
    fn status_helper_rejects_the_certificate_plus_a_stray_file() {
        // Proves the check is not satisfied by "the certificate is among the
        // lines" — EVERY line must match.
        let porcelain = " M docs/test/smoke-certificate.md\n?? leftover.txt\n";
        assert!(!status_shows_nothing_outside_the_certificate(porcelain));
    }

    // -- S21 --

    #[test]
    fn s21_is_out_of_scope_without_the_build_matrix_and_skips_when_it_could_not_be_built() {
        // Both halves, because only the pair carries the distinction: no matrix
        // means nobody asked for one (exit 0), while a matrix that WAS asked for
        // and whose cargo could not be spawned is the harness trying and failing
        // to test (exit 2).
        let ctx = blank_ctx(RunId::HappySmall);
        let a = s21_the_two_modes_cannot_be_confused(&ctx);
        assert_eq!(
            a[0].state,
            ScenarioState::OutOfScope,
            "four cargo checks sit behind a flag; not passing the flag is not a fault"
        );

        let unrunnable = vec![
            ("tree".to_string(), BuildOutcome::CouldNotRun),
            ("tree,published".to_string(), BuildOutcome::CouldNotRun),
            (String::new(), BuildOutcome::CouldNotRun),
        ];
        let ctx = RunContext {
            build_matrix: Some(&unrunnable),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s21_the_two_modes_cannot_be_confused(&ctx);
        assert!(
            matches!(a[0].state, ScenarioState::Skip(_)),
            "a matrix that was requested and could not be built is unanswered: {a:?}"
        );
    }

    #[test]
    fn s21_passes_when_tree_builds_and_both_conflicting_combinations_fail() {
        let matrix = vec![
            ("tree".to_string(), BuildOutcome::Built),
            ("published".to_string(), BuildOutcome::Built),
            ("tree,published".to_string(), BuildOutcome::DidNotBuild),
            (String::new(), BuildOutcome::DidNotBuild),
        ];
        let ctx = RunContext {
            build_matrix: Some(&matrix),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s21_the_two_modes_cannot_be_confused(&ctx);
        assert_eq!(a[0].state, ScenarioState::Pass);
    }

    #[test]
    fn s21_fails_when_both_features_together_actually_compiled() {
        // The exact regression this scenario exists to catch: the two
        // `compile_error!` guards silently stopped firing.
        let matrix = vec![
            ("tree".to_string(), BuildOutcome::Built),
            ("published".to_string(), BuildOutcome::Built),
            ("tree,published".to_string(), BuildOutcome::Built),
            (String::new(), BuildOutcome::DidNotBuild),
        ];
        let ctx = RunContext {
            build_matrix: Some(&matrix),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s21_the_two_modes_cannot_be_confused(&ctx);
        assert_eq!(a[0].state, ScenarioState::Fail);
    }

    #[test]
    fn s21_skips_rather_than_fails_when_cargo_could_not_be_run() {
        // A `cargo` that could not be SPAWNED told us nothing. Reading that as
        // "the combination did not build" would report a missing toolchain as
        // the regression this scenario exists to catch — exit 1, a verdict
        // about the crate, over a fault of ours.
        let matrix = vec![
            ("tree".to_string(), BuildOutcome::CouldNotRun),
            ("published".to_string(), BuildOutcome::CouldNotRun),
            ("tree,published".to_string(), BuildOutcome::CouldNotRun),
            (String::new(), BuildOutcome::CouldNotRun),
        ];
        let ctx = RunContext {
            build_matrix: Some(&matrix),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s21_the_two_modes_cannot_be_confused(&ctx);
        match &a[0].state {
            ScenarioState::Skip(reason) => assert!(
                reason.contains("cargo could not be run"),
                "the skip must name what could not be done: {reason:?}"
            ),
            other => panic!("expected a Skip, got {other:?}"),
        }
    }

    #[test]
    fn s21_fails_when_the_default_mode_does_not_build() {
        let matrix = vec![
            ("tree".to_string(), BuildOutcome::DidNotBuild),
            ("published".to_string(), BuildOutcome::Built),
            ("tree,published".to_string(), BuildOutcome::DidNotBuild),
            (String::new(), BuildOutcome::DidNotBuild),
        ];
        let ctx = RunContext {
            build_matrix: Some(&matrix),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s21_the_two_modes_cannot_be_confused(&ctx);
        assert_eq!(a[0].state, ScenarioState::Fail);
    }
}
