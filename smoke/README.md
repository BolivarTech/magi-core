# magi-smoke — the smoke harness

A small binary that exercises `magi-core` **the way an outside consumer does**: it builds the
trio through the public builder, points every provider at a spy proxy, runs `analyze()` against
a real backend, and asserts properties over what came back.

It is a tool, not a gate. It produces **evidence**; nothing here blocks a release, and deleting
this directory leaves the crate compiling, testing and publishing untouched.

---

## 1. Three exit codes, and the difference between two of them

| code | meaning |
|---|---|
| `0` | every assertion that ran passed |
| `1` | an assertion **FAILED** — a verdict about the crate |
| `2` | **could not test** — a fault of ours: config, fixtures, backend, probe or proxy |

**Confusing `1` with `2` is the failure this harness exists to eliminate.** A slow backend
reported as `1` sends someone hunting through the code for a problem that is in the cable; a
real defect reported as `2` gets filed under "flaky" and never looked at. The code is decided in
exactly one place — `outcome::exit_code` — and the report delegates to it rather than deciding
again.

There is a fourth row state, `OUT_OF_SCOPE`, which contributes to no exit code: it means a
partition nobody asked to run, not a question that went unanswered.

## 2. Two dependency modes

The harness links `magi-core` from one of two sources, chosen at **build** time:

- `--features tree` (the default) — the crate as it stands in this working tree. This is what
  SMOKE #1 and #2 use.
- `--no-default-features --features published` — the crate as published on crates.io.

They are mutually exclusive and the compiler enforces it: selecting both, or neither, is a
`compile_error!` explaining which to pick. A compiled binary cannot change which crate it links
against, so this is a build fact rather than a runtime flag — and the mode is printed at startup
so a run's output always says what it tested.

The published mode matters because some defects only exist against the packaged artifact. A
regression that made `ProviderError` unconstructible from another crate reached a consumer eight
days after release precisely because, inside `src/`, the variants are always constructible.

**Release checklist item, because nothing enforces it:** the published mode pins a version in
`smoke/Cargo.toml` (`magi_core_pub`), and that requirement is **not** derived from the crate's
own version — cargo resolves optional dependencies whether or not their feature is enabled, so
naming a version that does not exist yet breaks EVERY build of this package, the default one
included. **Bump it in the same commit that bumps the crate.** Forgetting it is caught after the
publish, by the version-drift guard, which compares the job-resolved lock against the version
just published; it can never pass silently, but it fails after the fact rather than before.

## 3. What the proxy does, and why its red is never the crate's

Every request the crate makes goes through a local spy proxy, which records it and forwards it
**unchanged**. That is what lets a scenario assert over the wire instead of over a mock.

The proxy can also **inject** a failure for one named model, which is how the rotation and
degradation runs are driven without waiting for a real backend to misbehave.

**A proxy fault is ours, never a verdict.** If it cannot start, the preflight stops with exit
`2` and every scenario reports that it could not be tested — none of them fails. If the proxy
degrades mid-run (a poisoned lock, an accept error) it says so, and any assertion that reads the
recorded traffic skips rather than failing over a registry it knows is partial.

## 4. The contention probe, and its declared scope

Before any run, the harness asks the backend for **one real completion of one token** — a `POST`
to the completions endpoint, naming the weakest model the config declares. If it does not answer
within the configured window, the run reports **cannot test** rather than starting.

**It must be a completion, and that is the whole design.** A manifest listing (`GET /api/tags`)
reads files off disk: it never loads a model, never touches the GPU and never enters the
inference queue, so a backend saturated by three mages answers it instantly. Generation is what
queues, so generation is what gets asked for — bounded to one token so asking costs nothing.
Reachability, the step before, *does* use the listing, because "is anybody there?" is a different
question and a listing is the right way to ask it.

**What it catches:** an endpoint that is saturated, or a model cold enough that loading it would
swallow the run's whole budget. It retries once with a widened window, so "clone and run" works
without pre-warming.

**What it does NOT catch, stated because a probe implies more than it delivers:**

- Contention that **begins after** the probe. The probe is a snapshot at one instant; a backend
  that becomes saturated once the runs start is invisible to it, and shows up later as a time
  failure instead.
- The difference between **contention and a cold model**. Both look like "slow", and the harness
  says so rather than guessing which.

## 5. Where it fits in the cycle

```
TDD cycle  →  SMOKE #1  →  review gate  →  SMOKE #2  →  release
                                              ↓ regression
                                     fix → re-run the gate
```

**#1 runs before the review gate**, because the gate is expensive and spending it to discover
that the code does not work is waste. **#2 runs after**, because the gate CHANGES the code: a
fix made to satisfy a review can break what already worked.

**A regression found by #2 expires the gate's verdict** — a verdict binds the exact artifact it
saw — so the fix goes in and the gate runs again. Only #2 writes a certificate, because a
certificate emitted from #1 would certify an artifact the gate has not touched yet.

## 6. Cost per run

The harness announces its estimate **before** it starts and records what it spent **after**.

The default run analyses a small payload three times over — happy path, rotation, degradation —
plus one run with no backend at all. Against a cloud backend that is on the order of a few
minutes and tens of thousands of tokens; against a local model it is bounded by your hardware,
not by the harness.

**The large-payload run is deliberately not part of this stage.** No assertion here reads it, so
launching it would pay for the most expensive run twice per cycle for nobody. The payload is
still generated and checked by size, which is the only claim available until the telemetry that
would justify running it exists.

## 7. The coverage cliff: several SKIPs sharing a run-id are ONE failure

Scenarios share runs. When a shared run does not produce a report, every scenario reading it
skips — so **one** broken run can print five skipped rows.

That is why every row carries its **run-id**. Five skips with the same id are one failure with
five symptoms, not five defects; without the id a single crashed run reads as a cascade and
sends someone looking for five problems.

## 8. Known limitations

Anyone reading a certificate needs to know what was **not** verified.

- **The `published` mode is not covered by the certificate.** Its scenarios verify a package
  that does not exist yet when the certificate is emitted — they run after the publish that the
  merge triggers. A certificate that stayed quiet about this would claim coverage it does not
  have.
- **Proxy transparency is verified only on the SMALL probe request**, not on a large-payload
  run. The comparison is by checksum over a request the harness itself sends and receives down
  both paths; the large-payload run is not part of this stage at all, so nothing here shows that
  the proxy relays a 62 000-token body unchanged. The property is the same and the machinery is
  the same, but the size is not exercised.
- **Fixture currency is not proven.** A hash proves a file is the one recorded; it does **not**
  prove the backend still answers that way. Only a live scenario proves currency, and entries
  marked `unverified:` have not had one.
- **The probe does not see contention that starts after the preflight** (§4).
- **A panic in a dependency the crate ALSO uses** — `reqwest`, `tokio`, `sha2` — is ambiguous by
  nature and is attributed to the crate (`FAIL`). Attributing it to the harness would bury a
  real defect; the reverse costs one investigation that finds nothing, which is the cheaper
  error.
- **A panic in a thread the crate spawned, one crossing FFI, and an abort are not caught at
  all.** They end the process, and the exit code is what the operator sees.
- **The relaxation-mark guard detects marks that are MALFORMED, not marks that are ABSENT.**
  Someone who relaxes an assertion without marking it stays invisible, and there is no general
  way to detect that without guessing the intent of a change.
- **The symlink-skipping property of the payload generator is unverified on Windows without
  Developer Mode**, because creating a file symlink there is privileged. It is verified on any
  Linux runner. A junction is not a substitute — measured: `symlink_metadata` reports
  `is_dir() == false` for one, so the walker ignores it whether its guard is present or not.

## 9. Running it — six invocations, not one

**"Green" is the union of six commands.** Four scenarios need the preflight to stop at a
*different* step, and the preflight stops at the first failure, so they cannot share an
invocation: putting two together leaves the second one unrun, which is green by omission.

| invocation | what it covers |
|---|---|
| `cargo run` | the live path: the outside provider, the happy run, proxy transparency, rotation, degradation, and the no-trace check |
| `MAGI_SMOKE_ENDPOINT=http://127.0.0.1:1 cargo run` | an unreachable backend |
| `cargo run -- --break-proxy` | a proxy that refuses to start |
| `MAGI_SMOKE_ENDPOINT=<slow stub> MAGI_SMOKE_PROBE_TIMEOUT_SECS=1 cargo run` | a saturated endpoint |
| `cargo run -- --config <broken.toml>` | an unreadable configuration |
| `cargo run -- --build-matrix` | the four feature combinations (slow: four `cargo check` runs) |

`--build-matrix` builds each combination into its own directory under the system temp
directory (`<temp>/magi-smoke-feature-matrix/<combination>`), never inside the checkout: one
directory per combination because two feature sets sharing one relink the same binaries and
produce link errors that read as code defects. Deleting that tree costs only the next run's
rebuild time.

Other flags: `--smoke-2` (this is SMOKE #2, so the certificate IS written), `--no-backend` (only
the scenarios that need none), `--json` (also emit the machine-readable report),
`--print-payload-size` (generate the payload, print its size, exit).

Configuration: copy `magi-smoke.toml.example` to `magi-smoke.toml` and edit. Every value in the
example is also the built-in default, and a test compares the two — so copying it unchanged
changes nothing. Credentials never live in it; they travel by environment variable.

## 10. `cargo audit` for the harness

The harness has its own dependencies and its own lockfile, so audit it on its own:

```bash
cd smoke && cargo audit
```

This is **advisory and outside the crate's release workflow**. The harness is not published and
its dependencies are not the crate's; a finding here is worth fixing but does not gate a release
of `magi-core`.

---

## 11. The reproduction that justifies the harness

Run **once**, by hand, on 2026-08-18, against `magi-core 3.2.0` — the version this tree was on
when the harness was built. It is a dated historical record, **not a test**: no assertion reads
it, and against `4.0.0` the same command is expected to stop reproducing anything. That is the
point.

```bash
cd smoke
cargo run -- --config <a config with run_payload_bytes = 250000> --json
```

**Setup:** the shipped trio (`qwen3.5:397b-cloud` / `kimi-k2.6:cloud` / `glm-5.2:cloud`) against
a local Ollama, one fallback candidate of a different lineage, and the crate's default output
budget of `max_tokens: 4096`. The generated payload was 250 000 bytes — about 62 500 tokens by
the harness's own coarse `bytes/4` bound.

**What came back (exit `1` — a verdict about the crate):**

```
[FAIL] run=happy_small — all three mages returned a verdict
[FAIL] run=happy_small — the run is not degraded
[SKIP] run=degradation — degraded is true (skipped: insufficient agents: 1 succeeded, 2 required)
```

The same trio, the same models and the same backend answer cleanly on a small payload — 17
assertions pass, none fails. Enlarge the payload and the happy path comes back **degraded**, and
the injected run collapses to **one surviving mage out of three**.

**What this reproduction does NOT show, and the omission is itself the finding.** It does not
say WHICH mage returned nothing, nor that its lineage was condemned for the other two, because
`3.2.0` does not report either. The crate turns an empty completion into an HTTP-shaped error
with a synthetic status, and from there into a transport failure that removes a lineage
run-wide — so what reaches a consumer is a collapsed trio with no attribution. The harness can
see the collapse and cannot see its cause.

That is precisely the gap `4.0.0` closes, and it is why this record is kept: the value of the
harness is not that it reproduced a known bug, but that it reproduced it **from outside**, the
way a consumer meets it.
