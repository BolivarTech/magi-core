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
//! - Missing report / missing error / missing wire traffic → [`Assertion::skip`]
//!   with a reason an operator can act on — never `Pass`.
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
/// # Parameters
///
/// * `ctx` — the context to read `error` from.
/// * `stage_prefix` — this scenario's own stage, e.g. `"Backend: "`.
///
/// # Errors
///
/// A reason string when there is no error to read, or when the error belongs
/// to a DIFFERENT stage — both cases the caller turns into `Skip`, never `Fail`
/// (a scenario cannot fail on evidence pointing at a stage it does not own).
fn preflight_error_for_stage<'a>(
    ctx: &RunContext<'a>,
    stage_prefix: &str,
) -> Result<&'a str, String> {
    match ctx.error {
        None => Err("no preflight error was recorded for this session".to_string()),
        Some(e) if !e.starts_with(stage_prefix) => {
            Err(format!("the preflight failed at a different stage: {e:?}"))
        }
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
fn s2_happy_path_against_real_backend(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME_VERDICTS: &str = "all three mages returned a verdict";
    const NAME_DEGRADED: &str = "the run is not degraded";
    const NAME_JSON: &str = "the report serializes to JSON";
    const NAME_NO_INJECTION: &str =
        "every completion got a real backend answer; the proxy injected nothing";

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
fn s2b_the_proxy_is_transparent(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME_REQUEST: &str = "the request the proxy relayed is byte-identical to ours";
    const NAME_RESPONSE: &str = "and so is the response it relayed back";

    // A degraded proxy is a HARNESS fault, and this scenario reads a record the
    // proxy produced. Without this gate a failed body read — which the proxy
    // records as an empty body — makes the checksum compare a hash of nothing
    // and reports FAIL: exit 1, a verdict about the crate, for something the
    // harness did. Every other scenario that reads proxy state already gates on
    // this; this one did not, and it is the one whose whole subject IS the proxy.
    if ctx.proxy_degraded {
        const WHY: &str = "the proxy degraded during this run, so its record cannot answer for                            what went on the wire";
        return vec![
            Assertion::skip(NAME_REQUEST, WHY),
            Assertion::skip(NAME_RESPONSE, WHY),
        ];
    }

    let (Some(rec), Some(sent), Some(direct)) =
        (ctx.probe_record, ctx.probe_sent_body, ctx.direct_probe_body)
    else {
        return vec![
            Assertion::skip(
                NAME_REQUEST,
                "the transparency probe did not complete; nothing to compare",
            ),
            Assertion::skip(
                NAME_RESPONSE,
                "the transparency probe did not complete; nothing to compare",
            ),
        ];
    };
    if !rec.response_recorded {
        return vec![
            Assertion::skip(NAME_REQUEST, "the probe response was not recorded"),
            Assertion::skip(NAME_RESPONSE, "the probe response was not recorded"),
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
    ]
}

// ---------------------------------------------------------------------------
// S4 — rotation and its cause, forced by injection
// ---------------------------------------------------------------------------

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
///   proof the injection fired at all, independent of rotation.
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
fn s4_rotation_and_its_cause(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME_WIRE: &str =
        "the injected failure reached a completion request over the wire, not a coincidence";
    const NAME_ROTATED: &str =
        "the injected agent rotated to a second, differently-lineaged candidate";
    const NAME_CAUSE: &str = "the reported rotation cause distinguishes mage-local from run-wide";

    if ctx.proxy_degraded {
        let reason = "the proxy registry degraded during this run";
        return vec![
            Assertion::skip(NAME_WIRE, reason),
            Assertion::skip(NAME_ROTATED, reason),
            Assertion::skip(NAME_CAUSE, reason),
        ];
    }

    let wire = assert_that(
        NAME_WIRE,
        ctx.records
            .iter()
            .any(|r| r.path == COMPLETIONS_PATH && r.response_status == INJECTED_FAILURE_STATUS),
    );

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
fn s5_the_probe_still_reads_what_it_expects(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME_WINDOW: &str = "the probe returns a measurable context window";
    const NAME_DIGEST: &str = "the probe returns a 64-hex-character digest";

    if ctx.proxy_degraded {
        let reason = "the proxy registry degraded during this run";
        return vec![
            Assertion::skip(NAME_WINDOW, reason),
            Assertion::skip(NAME_DIGEST, reason),
        ];
    }

    let show = ctx
        .records
        .iter()
        .find(|r| r.path == "/api/show" && r.response_recorded);
    let tags = ctx
        .records
        .iter()
        .find(|r| r.path == "/api/tags" && r.response_recorded);

    let window = match show {
        None => Assertion::skip(
            NAME_WINDOW,
            "no /api/show probe request was recorded for this run",
        ),
        Some(r) => assert_that(
            NAME_WINDOW,
            show_response_has_measurable_window(&r.response_body),
        ),
    };
    let digest = match tags {
        None => Assertion::skip(
            NAME_DIGEST,
            "no /api/tags probe request was recorded for this run",
        ),
        Some(r) => assert_that(
            NAME_DIGEST,
            tags_response_has_a_64_hex_digest(&r.response_body),
        ),
    };
    vec![window, digest]
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
        Err(reason) => vec![Assertion::skip(NAME, reason)],
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
        Err(reason) => vec![Assertion::skip(NAME, reason)],
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
/// by `cargo run -- --config <a broken toml>` (six-invocation table, README).
fn s14_illegible_toml_is_fatal(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME: &str =
        "an unreadable config names the offending field and cuts with exit 2 before any run";
    match preflight_error_for_stage(ctx, "Config: ") {
        Err(reason) => vec![Assertion::skip(NAME, reason)],
        Ok(err) => vec![assert_that(NAME, err.contains("unknown field"))],
    }
}

// ---------------------------------------------------------------------------
// S15 — degradation is honest in its FOUR assertions, forced by injection
// ---------------------------------------------------------------------------

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
        assert_that("degraded is true", report.degraded),
        assert_that(
            "failed_agents names the INJECTED agent, not just any agent",
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
            "agent_count counts those that RESPONDED, not those launched",
            report.consensus.agent_count == report.agents.len() && report.agents.len() == 2,
        ),
        // The label field is `consensus`, a String such as "GO (2-0)".
        // Degraded mode caps STRONG labels down to their regular form.
        assert_that(
            "no STRONG label survives a 2/3 consensus",
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
fn s15_degradation_is_honest(ctx: &RunContext<'_>) -> Vec<Assertion> {
    match ctx.report {
        Some(report) => s15_four_assertions(report, ctx),
        None => {
            let reason = ctx
                .error
                .map(str::to_string)
                .unwrap_or_else(|| "the degradation run never happened".to_string());
            vec![
                Assertion::skip("degraded is true", reason.clone()),
                Assertion::skip(
                    "failed_agents names the INJECTED agent, not just any agent",
                    reason.clone(),
                ),
                Assertion::skip(
                    "agent_count counts those that RESPONDED, not those launched",
                    reason.clone(),
                ),
                Assertion::skip("no STRONG label survives a 2/3 consensus", reason),
            ]
        }
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
    let out = std::process::Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(crate::paths::repo_root())
        .output();
    let Ok(out) = out else {
        return vec![Assertion::skip(
            NAME,
            "could not invoke git to check the tree",
        )];
    };
    if !out.status.success() {
        return vec![Assertion::skip(
            NAME,
            "git status did not exit successfully",
        )];
    }
    let after = String::from_utf8_lossy(&out.stdout);
    let baseline: std::collections::BTreeSet<&str> = before.lines().collect();
    let added: String = after
        .lines()
        .filter(|l| !baseline.contains(l))
        .collect::<Vec<_>>()
        .join(
            "
",
        );
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
        Err(reason) => vec![Assertion::skip(NAME, reason)],
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
/// `--build-matrix`; without it, SKIP, never PASS (R25).
///
/// **THREE of the four combinations are asserted, and `published` alone is
/// not.** *(The reason recorded here was stale: it said `published` resolves
/// `magi-core = "4.0"`, "which does not exist". `smoke/Cargo.toml` pins
/// `version = "3.2"`, which is on crates.io, so that combination resolves and
/// compiles today.)*
///
/// The real reason is narrower. This scenario's property is that the two modes
/// cannot be CONFUSED, and the three asserted combinations establish exactly
/// that: `tree` builds, both together do not, neither does. Whether `published`
/// builds on its own is a different claim — that the published mode works — and
/// it depends on crates.io being reachable, which is a fact about the machine
/// rather than about the crate under test.
fn s21_the_two_modes_cannot_be_confused(ctx: &RunContext<'_>) -> Vec<Assertion> {
    const NAME: &str = "the two dependency modes cannot be confused";
    let Some(m) = ctx.build_matrix else {
        return vec![Assertion::skip(
            NAME,
            "feature matrix not built; re-run with --build-matrix",
        )];
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

    #[test]
    fn s2b_skips_when_the_probe_never_completed() {
        let ctx = blank_ctx(RunId::HappySmall);
        let a = s2b_the_proxy_is_transparent(&ctx);
        assert_eq!(a.len(), 2);
        assert!(a.iter().all(|x| matches!(x.state, ScenarioState::Skip(_))));
    }

    #[test]
    fn s2b_skips_when_the_proxy_degraded_instead_of_blaming_the_crate() {
        // A failed body read is recorded as an EMPTY body, so without this gate
        // the checksum compares a hash of nothing and reports FAIL — exit 1, a
        // verdict about the crate, for a harness fault.
        let sent = "the probe body".to_string();
        let direct = b"the probe response".to_vec();
        let rec = recorded_response("/api/show", 200, &direct);
        let rec = RequestRecord {
            body_sha256: sha256_hex(b""),
            ..rec
        };
        let ctx = RunContext {
            probe_record: Some(&rec),
            probe_sent_body: Some(&sent),
            direct_probe_body: Some(&direct),
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
    fn s2b_passes_when_both_hashes_match() {
        let sent = "the probe body".to_string();
        let direct = b"the probe response".to_vec();
        let rec = recorded_response("/api/show", 200, &direct);
        let rec = RequestRecord {
            body_sha256: sha256_hex(sent.as_bytes()),
            ..rec
        };
        let ctx = RunContext {
            probe_record: Some(&rec),
            probe_sent_body: Some(&sent),
            direct_probe_body: Some(&direct),
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
        let rec = recorded_response("/api/show", 200, &direct);
        let ctx = RunContext {
            probe_record: Some(&rec),
            probe_sent_body: Some(&sent),
            direct_probe_body: Some(&direct),
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
            ..blank_ctx(RunId::Rotation)
        };
        let a = s4_rotation_and_its_cause(&ctx);
        assert_eq!(a[0].state, ScenarioState::Pass);
    }

    #[test]
    fn s4_wire_check_fails_when_no_completion_ever_saw_the_injected_status() {
        let records = vec![record(COMPLETIONS_PATH, 200)];
        let ctx = RunContext {
            records: &records,
            ..blank_ctx(RunId::Rotation)
        };
        let a = s4_rotation_and_its_cause(&ctx);
        assert_eq!(a[0].state, ScenarioState::Fail);
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

    // -- S6, S7, S14, S20 (preflight-sourced) --

    #[test]
    fn s6_skips_when_a_different_stage_failed() {
        let err = "Probe: cannot test: ...".to_string();
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s6_no_backend_no_green(&ctx);
        assert!(matches!(a[0].state, ScenarioState::Skip(_)));
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
    fn s7_skips_when_a_different_stage_failed() {
        let err = "Backend: unreachable".to_string();
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s7_a_saturated_endpoint_reports_cannot_test_with_its_scope(&ctx);
        assert!(matches!(a[0].state, ScenarioState::Skip(_)));
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
    fn s14_skips_when_a_different_stage_failed() {
        let err = "Proxy: refused to start".to_string();
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s14_illegible_toml_is_fatal(&ctx);
        assert!(matches!(a[0].state, ScenarioState::Skip(_)));
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
    fn s20_skips_when_a_different_stage_failed() {
        let err = "Backend: unreachable".to_string();
        let ctx = RunContext {
            error: Some(&err),
            ..blank_ctx(RunId::HappySmall)
        };
        let a = s20_broken_proxy_is_not_a_scenario_red(&ctx);
        assert!(matches!(a[0].state, ScenarioState::Skip(_)));
    }

    // -- S15 --

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
        let ctx = RunContext {
            report: Some(&report),
            injected_agent: Some(AgentName::Caspar),
            ..blank_ctx(RunId::Degradation)
        };
        let a = s15_degradation_is_honest(&ctx);
        assert!(a.iter().all(|x| x.state == ScenarioState::Pass), "{a:?}");
    }

    #[test]
    fn s15_fails_the_named_agent_check_when_a_different_agent_was_injected() {
        // Proves this is not "some agent failed" — it must be the INJECTED
        // one.
        let report = report_from(DEGRADED_REPORT_JSON);
        let ctx = RunContext {
            report: Some(&report),
            injected_agent: Some(AgentName::Melchior),
            ..blank_ctx(RunId::Degradation)
        };
        let a = s15_degradation_is_honest(&ctx);
        assert_eq!(a[1].state, ScenarioState::Fail);
    }

    #[test]
    fn s15_fails_the_strong_label_check_on_an_illegitimate_strong_label() {
        let report = report_from(DEGRADED_REPORT_WITH_STRONG_LABEL_JSON);
        let ctx = RunContext {
            report: Some(&report),
            injected_agent: Some(AgentName::Caspar),
            ..blank_ctx(RunId::Degradation)
        };
        let a = s15_degradation_is_honest(&ctx);
        assert_eq!(a[3].state, ScenarioState::Fail);
    }

    #[test]
    fn s15_fails_the_degraded_check_on_a_healthy_report() {
        let report = report_from(HEALTHY_REPORT_JSON);
        let ctx = RunContext {
            report: Some(&report),
            injected_agent: Some(AgentName::Caspar),
            ..blank_ctx(RunId::Degradation)
        };
        let a = s15_degradation_is_honest(&ctx);
        assert_eq!(a[0].state, ScenarioState::Fail);
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
    fn s21_skips_without_the_build_matrix() {
        let ctx = blank_ctx(RunId::HappySmall);
        let a = s21_the_two_modes_cannot_be_confused(&ctx);
        assert!(matches!(a[0].state, ScenarioState::Skip(_)));
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
