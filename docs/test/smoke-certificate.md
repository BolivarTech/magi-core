# Smoke Certificate

- version: 3.2.0
- commit: 6f4cedd
- date: 2026-08-23 (UTC)
- dependency mode: tree
- real cost: 8 backend run(s) in 244.0s
- rounds needed: 3
- invocation: --smoke-2 --round 3
- fixtures: 0 declared, 0 verified by a live scenario, 0 unverified
- result: 50 passed, 8 not passed, 58 total

> NOTE: this certificate covers the ONE invocation named above. A green release is the      union of several — the preflight stops at its first failure, so the scenarios that need      it to stop at different steps cannot share a command line.

[PASS] S3 run=large_62k — no seat was lost to an empty completion classified as transport
[PASS] S3 run=large_62k — a completion cut by the budget names the budget in its error
[PASS] S3 run=large_62k — no content failure condemned a lineage run-wide
[PASS] S10 run=large_62k — the large-payload run reached the wire and was recorded
[PASS] S10 run=large_62k — no seat was lost to an empty completion on the large payload
[PASS] S10 run=large_62k — the large-payload run is not degraded
[PASS] S10 run=large_62k — every completion ran under the raised output budget, not the old one
[PASS] S10 run=large_62k — the large payload reached the model large IN TOKENS, not just in bytes
[PASS] S11 run=large_62k — a reasoning trace was measured on the large-payload run
[PASS] S11 run=large_62k — with the flag on, the trace carries its text as well as its length
[PASS] S1 run=no_backend — an outside LlmProvider fails typed, classified mage-local (never endpoint-down)
[PASS] S2 run=happy_small — all three mages returned a verdict
[PASS] S2 run=happy_small — the run is not degraded
[PASS] S2 run=happy_small — the report serializes to JSON
[PASS] S2 run=happy_small — at least one completion reached the proxy, and none carried the failure status this harness injects
[PASS] S2b run=happy_small — the request the proxy relayed is byte-identical to ours
[PASS] S2b run=happy_small — and so is the response it relayed back
[PASS] S2b run=happy_small — and the status it relayed is the one the backend gave
[PASS] S4 run=rotation — the injected failure reached a completion request over the wire, not a coincidence
[PASS] S4 run=rotation — the injected agent rotated to a second, differently-lineaged candidate
[PASS] S4 run=rotation — the injected HTTP failure rotates as run-wide Transport, not as a mage-local cause
[PASS] S5 run=happy_small — every probe answer carries a measurable context window
[PASS] S5 run=happy_small — every probe answer carries a 64-hex-character digest
[OUT_OF_SCOPE] S6 run=(no run) — an unreachable backend makes the preflight cut with exit 2, naming the backend as the cause
[OUT_OF_SCOPE] S7 run=(no run) — a slow-but-responding endpoint is 'cannot test', naming that contention and a cold model cannot be told apart
[OUT_OF_SCOPE] S14 run=(no run) — a config with an unknown field names it and cuts with exit 2 before any run
[PASS] S15 run=degradation — degraded is true
[PASS] S15 run=degradation — failed_agents names the INJECTED agent, not just any agent
[PASS] S15 run=degradation — agent_count counts those that RESPONDED, not those launched
[PASS] S15 run=degradation — no STRONG label survives a 2/3 consensus
[PASS] S16 run=(no run) — the harness added nothing to the tree outside the certificate path
[OUT_OF_SCOPE] S20 run=(no run) — a proxy that cannot start is a harness fault, never a crate verdict, and fails no scenario
[OUT_OF_SCOPE] S21 run=(no run) — the two dependency modes cannot be confused
[PASS] S8 run=happy_small — every completion request goes to the native endpoint
[PASS] S8 run=happy_small — no request reaches the OpenAI-compatible completions path
[PASS] S8b run=large_62k_no_reasoning — every seat produced a verdict on the large payload
[PASS] S8b run=large_62k_no_reasoning — no seat spent the run reasoning
[PASS] S8b run=large_62k_no_reasoning — and no completion came near the output budget
[PASS] S9 run=crate_defect — the run ends with an error that names a defect of this crate
[PASS] S9 run=crate_defect — no report was produced, so no seat was blamed for it
[PASS] S9b run=(no run) — the live backend still OMITS the token counters
[PASS] S9b run=(no run) — the live backend still reports the load termination
[PASS] S9b run=(no run) — the live backend still returns empty content
[PASS] S12 run=mixed_trio — the mixed trio completes without breaking or degrading
[PASS] S12 run=mixed_trio — the seat that cannot honour the control declares it, naming its backend
[PASS] S12 run=mixed_trio — the declaration is distinguishable from a measured absence of reasoning
[PASS] S13 run=happy_small — every seat that answered left a completion record
[PASS] S13 run=happy_small — every record names its model and the budget it ran under
[PASS] S13 run=happy_small — every record carries the termination reason the backend reported
[PASS] S13 run=happy_small — with the flag off, no record carries the trace text
[PASS] S-F1 run=(no run) — the derived worst case is per seat and does not bound anything
[PASS] S-F2a run=(no run) — the agent ceiling covers the chain worst case
[OUT_OF_SCOPE] S-F2b run=(no run) — an exhausted budget is reported as a TYPED abandonment (OUT OF SCOPE here: the variant is stringified before it leaves the crate, and this harness does not wrap its providers in RetryProvider; the crate's own tests match it)
[OUT_OF_SCOPE] S-F3 run=(no run) — an attempt-limited class stops at its own count (OUT OF SCOPE here: this harness does not wrap its providers in RetryProvider, so no attempt count reaches the wire; verified inside the crate instead)
[OUT_OF_SCOPE] S-F4 run=(no run) — the configuration warning fires on a bad relation and not on the defaults (OUT OF SCOPE here: it leaves only through tracing and dangerous_settings is pub(crate); observing it would need a new dependency to re-check a construction property)
[PASS] S-F5 run=(no run) — no time value makes construction fail
[PASS] S-E2 run=pool_eligibility — a seat that never rotated still reports which candidates were not eligible, and why
[PASS] S-E3 run=pool_eligibility — every failing condition is reported, not only the first
