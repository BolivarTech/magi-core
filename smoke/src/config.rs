// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-17

//! Harness configuration. Fail-closed by construction: an unknown field is an
//! error and an unreadable file is fatal, because falling back to defaults would
//! run against a DIFFERENT backend than the operator believes and report green
//! over what it never tested.

use crate::alias::magi_core::schema::AgentName;
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

/// The harness's full configuration: backend endpoint, contention-probe
/// window, generated-payload size, per-run time budgets, and the trio of
/// seats to run.
///
/// Every struct in this module derives `#[serde(deny_unknown_fields)]`, so an
/// unrecognised key anywhere in the file is a parse error, never a silently
/// ignored one — see the module doc for why that matters.
#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Base URL of the backend under test, e.g. `"http://localhost:11434"`.
    /// No trailing path segment; each scenario appends its own.
    ///
    /// Checked at load time by [`Config::validate_endpoint`] for a known scheme
    /// and a non-empty, whitespace-free host — enough to keep a value that is
    /// not a URL from being discovered at the first request instead of here.
    /// Whether anything ANSWERS at that address is the preflight's question.
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    /// How long, in seconds, the contention probe waits for a trivial request
    /// to answer before a run reports "cannot test" instead of attempting the
    /// scenario. Converted to a [`Duration`] by [`Config::probe_timeout`]; the
    /// retry widens this window by [`PROBE_RETRY_FACTOR`].
    #[serde(default = "default_probe_timeout")]
    pub probe_timeout_secs: u64,
    /// Target size, in bytes, of the payload the large-payload scenario
    /// generates. Bytes, not tokens: bytes are what the generator can measure
    /// without a tokenizer, so the token count is asserted at runtime instead.
    #[serde(default = "default_payload_target")]
    pub payload_target_bytes: usize,
    /// Target size, in bytes, of the payload THIS stage's runs analyse.
    ///
    /// Separate from [`Config::payload_target_bytes`], which sizes the
    /// large-payload scenario that this stage deliberately does not run. Small
    /// by default because these runs exist to exercise paths, not to reproduce
    /// the large-input failure — and configurable because raising it is exactly
    /// how that failure IS reproduced, by hand, once.
    #[serde(default = "default_run_payload")]
    pub run_payload_bytes: usize,
    /// Per-scenario time caps. See [`Budgets`] and [`Config::budget`].
    #[serde(default)]
    pub budgets: Budgets,
    /// Seats MUST be last in the file: in TOML every loose key must precede the
    /// first array-of-tables, so a key added afterwards silently parses into the
    /// wrong table.
    #[serde(default)]
    pub seats: Vec<Seat>,
    /// The rotation candidates the trio can fall back to.
    ///
    /// Without at least one, the run that exists to exercise rotation has
    /// nowhere to rotate TO, and the scenario reading it can only report that it
    /// could not be tested — which is what it did before this field existed.
    #[serde(default)]
    pub fallbacks: Vec<Fallback>,
}

/// One agent's assignment: which mage seat, which model, which rotation
/// lineage, and whether that model's provider can honour a reasoning-control
/// request. See [`Seat::agent_name`] for the mapping to the crate's own enum.
#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Seat {
    /// Which mage seat this entry fills, as one of the three lowercase names
    /// `"melchior"`, `"balthasar"` or `"caspar"`. Any other value is a config
    /// error at load time (see [`Seat::agent_name`]) — never silently defaulted
    /// or dropped.
    pub agent: String,
    /// The model identifier this seat's provider is built with, e.g.
    /// `"glm-5.2:cloud"`. Opaque to this harness: forwarded to the provider,
    /// never parsed or validated here.
    pub model: String,
    /// The rotation diversity key. **Not optional and not derivable**:
    /// `MagiBuilder::with_probing_agent` takes it in the signature, so without it
    /// the trio cannot be built at all.
    pub lineage: String,
    /// Declares whether this seat's provider can honour a reasoning-control
    /// request. The mixed-trio scenario needs one that cannot.
    #[serde(default = "yes")]
    pub supports_reasoning_control: bool,
}

/// A rotation candidate the whole trio can fall back to.
///
/// It carries no `agent`, and that is not an omission: the crate's fallback pool
/// is shared by every seat rather than declared per seat, so a candidate belongs
/// to the run, not to a mage.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fallback {
    /// The model identifier this candidate's provider is built with. Opaque
    /// here: forwarded to the provider, never parsed.
    pub model: String,
    /// The rotation diversity key. A candidate sharing a seat's lineage buys
    /// nothing — rotation exists to reach a DIFFERENT lineage — so it is
    /// required for the same reason a seat's is.
    pub lineage: String,
}

impl Seat {
    /// Maps the TOML string to the crate's enum. An unknown name is a config
    /// error, never a default: silently seating a fourth mage would make the
    /// degradation assertions read a trio that is not the one configured.
    pub fn agent_name(&self) -> Result<AgentName, ConfigError> {
        match self.agent.as_str() {
            "melchior" => Ok(AgentName::Melchior),
            "balthasar" => Ok(AgentName::Balthasar),
            "caspar" => Ok(AgentName::Caspar),
            other => Err(ConfigError(format!(
                "unknown agent {other:?}; expected melchior, balthasar or caspar"
            ))),
        }
    }
}

/// Per-scenario time caps, in seconds. **Caps, not predictions**: a run that
/// reaches its budget reports a TIME failure, never a verdict about the crate
/// under test. See [`Config::budget`] for the [`RunId`] -> [`Duration`]
/// mapping these back.
#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    /// Budget for [`RunId::HappySmall`]: a small payload against a healthy
    /// backend.
    #[serde(default = "b_happy")]
    pub happy_secs: u64,
    /// Budget for [`RunId::Large62k`]: the large-payload scenario.
    #[serde(default = "b_large")]
    pub large_payload_secs: u64,
    /// Budget shared by [`RunId::Rotation`] and [`RunId::Degradation`]: runs
    /// that inject a failure into the first model and expect the harness to
    /// recover or degrade rather than hang.
    #[serde(default = "b_injected")]
    pub injected_secs: u64,
    /// Budget for [`RunId::NoBackend`]: the offline run, which touches no
    /// network except the published docs.
    #[serde(default = "b_nobackend")]
    pub no_backend_secs: u64,
}

fn default_endpoint() -> String {
    "http://localhost:11434".to_string()
}
fn default_probe_timeout() -> u64 {
    10
}
fn default_payload_target() -> usize {
    250_000
}
/// The payload this stage's runs analyse.
///
/// Enough content for a real analysis and small enough that three runs of it
/// are cheap. The large-input failure needs two orders of magnitude more, which
/// is why reproducing it is a deliberate, separate invocation.
fn default_run_payload() -> usize {
    2_048
}
fn yes() -> bool {
    true
}
fn b_happy() -> u64 {
    120
}
fn b_large() -> u64 {
    300
}
fn b_injected() -> u64 {
    180
}
fn b_nobackend() -> u64 {
    30
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            happy_secs: b_happy(),
            large_payload_secs: b_large(),
            injected_secs: b_injected(),
            no_backend_secs: b_nobackend(),
        }
    }
}

/// A configuration error. Its `Display` always NAMES the offending field or
/// environment variable, so a reader is never sent hunting through the file or
/// the environment to find what was wrong — every fallible `Config` operation
/// (parsing, range validation, environment-override handling) constructs one
/// this way.
#[derive(Debug)]
pub struct ConfigError(String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// **R30: the CLOSED list of environment overrides.** The declared precedence is
/// `env > file > built-in`, so a range check that only looks at the file leaves
/// the HIGHEST-precedence path unguarded — `MAGI_SMOKE_PROBE_TIMEOUT_SECS=0`
/// would walk straight in.
///
/// **It is explicit because without it "validation also covers env vars" cannot
/// be verified or implemented completely**: there is nothing to check coverage
/// against.
///
/// The test `every_declared_env_override_is_actually_handled` walks this list
/// and requires [`Config::apply_env_override`] to handle **every** entry —
/// where "handled" means an arm of its own that CHANGES the config, not merely
/// the absence of an error. This sentence used to be a claim with no such test
/// behind it, and `MAGI_SMOKE_RUN_PAYLOAD_BYTES` was declared here for a whole
/// milestone with no arm to receive it.
///
/// **Credentials are NOT here, on purpose**: they travel by environment and
/// **never** touch the file, so they have no file-side counterpart to override.
pub const ENV_OVERRIDES: [(&str, &str); 8] = [
    ("MAGI_SMOKE_ENDPOINT", "endpoint"),
    ("MAGI_SMOKE_PROBE_TIMEOUT_SECS", "probe_timeout_secs"),
    ("MAGI_SMOKE_PAYLOAD_TARGET_BYTES", "payload_target_bytes"),
    ("MAGI_SMOKE_RUN_PAYLOAD_BYTES", "run_payload_bytes"),
    ("MAGI_SMOKE_BUDGET_HAPPY_SECS", "budgets.happy_secs"),
    ("MAGI_SMOKE_BUDGET_LARGE_SECS", "budgets.large_payload_secs"),
    ("MAGI_SMOKE_BUDGET_INJECTED_SECS", "budgets.injected_secs"),
    (
        "MAGI_SMOKE_BUDGET_NO_BACKEND_SECS",
        "budgets.no_backend_secs",
    ),
];

/// The four shared runs plus the offline one. **Lives here, in `config.rs`**,
/// because `Config::budget` consumes it: defining it in the runner (Task 9)
/// would create an inverted dependency between modules. Closed on purpose — a
/// run is a unit the harness EXECUTES, and adding one is a deliberate change,
/// not a config value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunId {
    HappySmall,
    Large62k,
    Rotation,
    Degradation,
    NoBackend,
}

impl RunId {
    /// Stable identifier printed alongside every assertion, so five red results
    /// with the same id read as ONE failure with five symptoms.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HappySmall => "happy_small",
            Self::Large62k => "large_62k",
            Self::Rotation => "rotation",
            Self::Degradation => "degradation",
            Self::NoBackend => "no_backend",
        }
    }

    /// Whether this run sends anything to the backend.
    ///
    /// An exhaustive `match` rather than a list of the ones that do: a run added
    /// later stops the compilation here and its answer gets decided, instead of
    /// being inherited from whichever side the author of the list forgot.
    pub fn uses_backend(self) -> bool {
        match self {
            Self::HappySmall | Self::Large62k | Self::Rotation | Self::Degradation => true,
            Self::NoBackend => false,
        }
    }
}

/// Upper bound for `probe_timeout_secs`, in seconds. Ten minutes is generous
/// slack over any sane contention-probe window; above it the "probe" would be
/// long enough to hide a hung backend rather than detect one.
const MAX_PROBE_TIMEOUT_SECS: u64 = 600;

/// Ceiling for any single run's time budget, in seconds: four hours.
///
/// High on purpose. Against a cloud backend a run is minutes, but against a
/// local model on one GPU the three mages are SERIALISED by the hardware, and
/// this harness must not turn a legitimate deployment into a permanent failure.
/// What the ceiling rejects is a value that has stopped being a cap — a run
/// allowed to last a day reports a TIME failure nobody will ever be present for.
const MAX_BUDGET_SECS: u64 = 4 * 60 * 60;

/// Lower bound for `payload_target_bytes`, in bytes. Below this the
/// large-payload scenario cannot reproduce the failure the harness exists to
/// catch — a reasoning model exhausting its output budget on a large
/// payload — and would certify exactly what never fails.
const MIN_PAYLOAD_TARGET_BYTES: usize = 100_000;

/// Lower bound for `run_payload_bytes`, in bytes.
///
/// **One, and deliberately not a rounder-looking number.** The only bound this
/// field has a defensible basis for is "not empty": a zero-byte payload makes
/// every backend run analyse nothing, and a trio that agrees about nothing
/// still produces a report, so the runs would pass over an input that was never
/// there — green by omission, arrived at through configuration. Anything above
/// `1` would be a size someone invented, and there is no upper bound at all
/// because raising this value **is** the documented way to reproduce the
/// large-input failure by hand (see [`Config::run_payload_bytes`]); a ceiling
/// here would forbid the one use the field exists for.
const MIN_RUN_PAYLOAD_BYTES: usize = 1;

/// The two schemes an `endpoint` may carry. A closed list, not a pattern: the
/// harness talks to the backend over `reqwest`, and these are the schemes it
/// speaks to one.
const ENDPOINT_SCHEMES: [&str; 2] = ["http://", "https://"];

/// Characters that end the authority component of a URL, i.e. everything after
/// which the text is a path, a query or a fragment rather than `host[:port]`.
const AUTHORITY_TERMINATORS: [char; 3] = ['/', '?', '#'];

impl Config {
    pub fn from_str(text: &str) -> Result<Self, ConfigError> {
        let cfg: Config = toml::from_str(text).map_err(|e| ConfigError(e.to_string()))?;
        // `validate()` calls `validate_probe_window()`: defining it and never
        // calling it would leave it as documentation with Rust syntax.
        cfg.validate()?;
        Ok(cfg)
    }

    // --- The ONLY accessors the rest of the harness uses. The `_secs` fields are
    // the WIRE shape (toml); everything downstream speaks `Duration`, so the
    // conversion happens exactly here and nowhere else. Tasks 8 and 9 call these.

    /// Contention-probe window. See R27 for what it does NOT cover.
    pub fn probe_timeout(&self) -> Duration {
        Duration::from_secs(self.probe_timeout_secs)
    }

    /// Per-run time cap (R32). **Caps, not predictions**: a run that reaches one
    /// reports a TIME failure, never a verdict about the crate.
    ///
    /// Takes a `RunId`, **not a string**: the set of runs is closed and known at
    /// compile time, so an exhaustive `match` removes the "unknown id" case
    /// entirely — no `panic!`, no `Option`, and no magic strings (§0.1.2).
    pub fn budget(&self, run: RunId) -> Duration {
        let secs = match run {
            RunId::HappySmall => self.budgets.happy_secs,
            RunId::Large62k => self.budgets.large_payload_secs,
            RunId::Rotation | RunId::Degradation => self.budgets.injected_secs,
            RunId::NoBackend => self.budgets.no_backend_secs,
        };
        Duration::from_secs(secs)
    }

    /// The longest budget among the runs that actually talk to the backend —
    /// the bound the spy proxy gives its upstream client.
    ///
    /// # Why the proxy needs it, and why it is derived rather than invented
    ///
    /// A backend that accepts a connection and then never answers would hang
    /// the proxy's forward. The run's own budget does eventually cut it, so
    /// nothing hangs forever — but the proxy would be the one component in the
    /// path with no bound of its own, and a harness component with no bound is
    /// the thing that ends up blamed for a backend fault.
    ///
    /// The value is the LONGEST such budget, not a fresh constant, for one
    /// reason: the proxy must never be what cuts first. Cutting before the run
    /// would turn a slow-but-legal backend into a proxy error, and a proxy
    /// error is a HARNESS fault — reported as "cannot test", never as a
    /// verdict about the crate. Bounding it by the longest budget makes the
    /// run's cap the one that always expires first.
    ///
    /// [`Budgets::no_backend_secs`] is excluded on purpose: the offline run
    /// sends nothing through the proxy, so letting its (short) budget pull the
    /// bound down would cut forwards belonging to runs it has nothing to do
    /// with. The three included here are exactly the budgets of the runs for
    /// which [`RunId::uses_backend`] is true.
    ///
    /// # Returns
    ///
    /// The maximum of the three backend-run budgets, as a [`Duration`]. Never
    /// zero: [`Config::validate`] rejects a zero budget outright.
    pub fn longest_backend_budget(&self) -> Duration {
        let secs = self
            .budgets
            .happy_secs
            .max(self.budgets.large_payload_secs)
            .max(self.budgets.injected_secs);
        Duration::from_secs(secs)
    }

    // --- `impl Config` continues below ---

    /// Every numeric value has a range, and violating it NAMES the field.
    /// A `0` in a timeout is not an aggressive setting: it is one that disables
    /// the check silently.
    fn validate(&self) -> Result<(), ConfigError> {
        // Overrides are applied BEFORE this point (see ENV_OVERRIDES at module
        // scope), so every check below sees the effective value regardless of
        // where it came from — file or env.
        //
        // THE CALL, which was missing: `validate_probe_window` existed and
        // nothing invoked it, i.e. it was documentation with Rust syntax. Three
        // mages flagged it independently.
        self.validate_probe_window()?;
        self.validate_endpoint()?;
        if self.probe_timeout_secs == 0 || self.probe_timeout_secs > MAX_PROBE_TIMEOUT_SECS {
            return Err(ConfigError(format!(
                "probe_timeout_secs must be in 1..={MAX_PROBE_TIMEOUT_SECS}; 0 disables the \
                 contention probe silently"
            )));
        }
        if self.payload_target_bytes < MIN_PAYLOAD_TARGET_BYTES {
            return Err(ConfigError(format!(
                "payload_target_bytes must be >= {MIN_PAYLOAD_TARGET_BYTES}: below that the \
                 large-payload scenario stops being large and certifies exactly what never fails"
            )));
        }
        if self.run_payload_bytes < MIN_RUN_PAYLOAD_BYTES {
            return Err(ConfigError(format!(
                "run_payload_bytes must be >= {MIN_RUN_PAYLOAD_BYTES}: an empty payload makes \
                 every backend run analyse nothing, and a run that analysed nothing still \
                 reports — so the suite would go green over an input that was never sent"
            )));
        }
        for (name, v) in [
            ("happy_secs", self.budgets.happy_secs),
            ("large_payload_secs", self.budgets.large_payload_secs),
            ("injected_secs", self.budgets.injected_secs),
            ("no_backend_secs", self.budgets.no_backend_secs),
        ] {
            if v == 0 || v > MAX_BUDGET_SECS {
                return Err(ConfigError(format!(
                    "budgets.{name} must be in 1..={MAX_BUDGET_SECS}: zero caps a run at \
                     nothing, and a budget beyond the ceiling stops being a cap at all — a \
                     run that can last a day reports a TIME failure nobody will ever see"
                )));
            }
        }
        // The probe guards the runs that USE the backend, so it is bounded by the
        // shortest of THOSE — not by `no_backend_secs`, which belongs to the one
        // run the probe does not guard at all. Comparing against it tied the
        // probe to something unrelated and rejected sane configurations.
        let shortest_guarded = self
            .budgets
            .happy_secs
            .min(self.budgets.large_payload_secs)
            .min(self.budgets.injected_secs);
        if self.probe_timeout_secs > shortest_guarded {
            return Err(ConfigError(format!(
                "probe_timeout_secs ({}) must be <= the shortest backend-using budget ({}), \
                 or the probe outlives the scenario it guards",
                self.probe_timeout_secs, shortest_guarded
            )));
        }
        Ok(())
    }

    /// Rejects any `MAGI_SMOKE_*` that is not in `ENV_OVERRIDES`. The namespace is
    /// fail-closed: a typo'd variable must be an error, never a setting that
    /// silently does nothing.
    pub fn reject_unknown_smoke_vars() -> Result<(), ConfigError> {
        for (k, _) in std::env::vars() {
            if k.starts_with("MAGI_SMOKE_") && !ENV_OVERRIDES.iter().any(|(n, _)| *n == k) {
                return Err(ConfigError(format!(
                    "unknown environment variable {k}: the MAGI_SMOKE_ namespace is \
                     fail-closed. Known: {:?}",
                    ENV_OVERRIDES.iter().map(|(n, _)| *n).collect::<Vec<_>>()
                )));
            }
        }
        Ok(())
    }

    /// Applies ONE environment override on top of `base` and re-validates the
    /// whole config.
    ///
    /// # Parameters
    ///
    /// * `key` — the variable's name, normally one from [`ENV_OVERRIDES`].
    /// * `raw` — its unparsed value, exactly as the environment holds it.
    /// * `base` — the config the override is layered onto (file or built-in).
    ///
    /// # Returns
    ///
    /// The config with the override applied. A key outside the `MAGI_SMOKE_`
    /// namespace is ignored, which is what lets a caller hand this the whole
    /// environment without filtering it first.
    ///
    /// # Errors
    ///
    /// [`ConfigError`] if the value does not parse, if the resulting config
    /// fails [`Config::validate`], or if `key` is in the `MAGI_SMOKE_`
    /// namespace but has no arm here — a typo'd override that was silently
    /// ignored would look exactly like one that was applied, and the operator
    /// would debug a value they believe they changed.
    pub fn apply_env_override(
        key: &str,
        raw: &str,
        mut base: Config,
    ) -> Result<Config, ConfigError> {
        // Declared precedence is env > file > built-in, so the range check must
        // cover the HIGHEST-precedence path too.
        //
        // EVERY key in ENV_OVERRIDES is handled, and an UNKNOWN `MAGI_SMOKE_*`
        // key is an ERROR, not a shrug: a typo'd override that is silently
        // ignored looks exactly like one that was applied, and the operator
        // would debug a value they believe they changed.
        let num = |v: &mut u64| -> Result<(), ConfigError> {
            *v = raw
                .parse()
                .map_err(|_| ConfigError(format!("{key}: not a number")))?;
            Ok(())
        };
        match key {
            "MAGI_SMOKE_ENDPOINT" => base.endpoint = raw.to_string(),
            "MAGI_SMOKE_PROBE_TIMEOUT_SECS" => num(&mut base.probe_timeout_secs)?,
            "MAGI_SMOKE_BUDGET_HAPPY_SECS" => num(&mut base.budgets.happy_secs)?,
            "MAGI_SMOKE_BUDGET_LARGE_SECS" => num(&mut base.budgets.large_payload_secs)?,
            "MAGI_SMOKE_BUDGET_INJECTED_SECS" => num(&mut base.budgets.injected_secs)?,
            "MAGI_SMOKE_BUDGET_NO_BACKEND_SECS" => num(&mut base.budgets.no_backend_secs)?,
            "MAGI_SMOKE_PAYLOAD_TARGET_BYTES" => {
                base.payload_target_bytes = raw
                    .parse()
                    .map_err(|_| ConfigError(format!("{key}: not a number")))?;
            }
            // The arm this list PROMISED and did not have. `ENV_OVERRIDES`
            // declared the variable, so `reject_unknown_smoke_vars` let it
            // through, and it then fell into the catch-all below and was
            // refused as an "unknown override" — a variable the harness
            // advertises and then rejects. The test
            // `every_declared_env_override_is_actually_handled` is what now
            // makes the list and this `match` fail together instead of drifting.
            "MAGI_SMOKE_RUN_PAYLOAD_BYTES" => {
                base.run_payload_bytes = raw
                    .parse()
                    .map_err(|_| ConfigError(format!("{key}: not a number")))?;
            }
            // The fail-closed catch-all: a `MAGI_SMOKE_*` key with no arm of
            // its own lands here and is REFUSED, which is what makes the
            // coverage test above able to tell "handled" from "fell through".
            other if other.starts_with("MAGI_SMOKE_") => {
                return Err(ConfigError(format!(
                    "{other}: unknown override. Known keys: {}",
                    ENV_OVERRIDES
                        .iter()
                        .map(|(k, _)| *k)
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            _ => {}
        }
        // Validation runs AFTER, so an env value goes through the same ranges as
        // a file value — which is the whole point of R30.
        base.validate()?;
        Ok(base)
    }

    /// Returns the config and the sentence that MUST be printed before anything
    /// runs, naming which file was loaded or that defaults were used.
    pub fn load_or_default(path: Option<&Path>) -> Result<(Config, String), ConfigError> {
        let base = match path {
            Some(p) if p.exists() => {
                let text = std::fs::read_to_string(p)
                    .map_err(|e| ConfigError(format!("{}: {e}", p.display())))?;
                Ok((Config::from_str(&text)?, format!("config: {}", p.display())))
            }
            Some(p) => Err(ConfigError(format!(
                "{}: file was given but does not exist — refusing to fall back to defaults",
                p.display()
            ))),
            None => Ok((
                Config::default_config(),
                "config: built-in defaults".to_string(),
            )),
        }?;
        // The declared precedence is `env > file > built-in`, so the overrides
        // are applied HERE, over whichever base was chosen. Defining
        // `apply_env_override` without calling it would leave the
        // highest-precedence path unreachable — and its range validation with
        // it, which is exactly the bypass R30 names
        // (`MAGI_SMOKE_PROBE_TIMEOUT_SECS=0` walking straight in).
        let (mut cfg, origin) = base;
        for (key, _) in ENV_OVERRIDES {
            if let Ok(raw) = std::env::var(key) {
                cfg = Config::apply_env_override(key, &raw, cfg)?;
            }
        }
        // An unknown MAGI_SMOKE_* is rejected too: the namespace is fail-closed,
        // so a typo cannot become a setting that silently does nothing.
        Config::reject_unknown_smoke_vars()?;
        cfg.validate()?;
        Ok((cfg, origin))
    }

    /// The built-in defaults, **seats included**.
    ///
    /// # Why seats have defaults at all
    ///
    /// `R30` says the harness ships pointing at a local Ollama by default, and
    /// the closing criterion is `cargo run` with **no arguments**. A default with
    /// `seats: []` satisfies neither: every backend run needs models, so the
    /// tool would build, start, and then be unable to do the thing it exists
    /// for — while looking configured.
    ///
    /// # Why THESE three, and why the example file repeats them
    ///
    /// They are the trio this repository actually runs
    /// (`.claude/magi-ollama.toml`), so they are known to exist rather than
    /// invented for a default. The mixed trio is not decoration: **exactly one
    /// seat must not honour the reasoning control**, or the property that
    /// replaced "fail loudly" cannot be exercised.
    ///
    /// `magi-smoke.toml.example` carries the same three values on purpose — it
    /// documents the WHY of each key — and
    /// `the_example_file_matches_the_built_in_defaults` keeps the two from
    /// drifting. Two sources of truth that nothing compares is how one of them
    /// becomes wrong in silence.
    ///
    /// # A default is not a promise that the models are reachable
    ///
    /// The preflight verifies availability. An unavailable model makes the
    /// backend runs report **SKIP with its reason**, never PASS (R25).
    ///
    /// # And that IS in tension with "green from day one" — declared, not papered over
    ///
    /// These three are `:cloud` tags: they need `ollama signin` and a `pull` per
    /// tag. On a machine without them, `cargo run` starts, the preflight says
    /// so, and eight of the twelve scenarios SKIP. **"E1 green" therefore means
    /// "green on a machine with the trio available"**, and the README says it
    /// in those words — the alternative was a local default that would be a
    /// guess about someone else's hardware, which this project refuses
    /// elsewhere for the same reason.
    fn default_config() -> Config {
        Config {
            endpoint: default_endpoint(),
            probe_timeout_secs: default_probe_timeout(),
            payload_target_bytes: default_payload_target(),
            run_payload_bytes: default_run_payload(),
            budgets: Budgets::default(),
            seats: default_seats(),
            fallbacks: default_fallbacks(),
        }
    }
}

/// The mixed trio. **`supports_reasoning_control` is NOT all-true**: S-C3 needs
/// one seat that cannot honour it.
/// The default rotation candidates.
///
/// One is enough for the property under test — that a seat whose model fails
/// reaches a DIFFERENT lineage — and each extra one costs a preflight probe.
fn default_fallbacks() -> Vec<Fallback> {
    vec![Fallback {
        model: "deepseek-v4-pro:cloud".into(),
        lineage: "deepseek".into(),
    }]
}

fn default_seats() -> Vec<Seat> {
    vec![
        Seat {
            agent: "melchior".into(),
            model: "qwen3.5:397b-cloud".into(),
            lineage: "alibaba".into(),
            supports_reasoning_control: true,
        },
        Seat {
            agent: "balthasar".into(),
            model: "kimi-k2.6:cloud".into(),
            lineage: "moonshot".into(),
            supports_reasoning_control: false,
        },
        Seat {
            agent: "caspar".into(),
            model: "glm-5.2:cloud".into(),
            lineage: "zhipu".into(),
            supports_reasoning_control: true,
        },
    ]
}

/// The widened second window. **The config validates `window * (1 + FACTOR)`
/// against the shortest run budget, not `window` alone** — validating the bare
/// value would let a probe legally consume more than the run it is protecting,
/// which is the range check agreeing with itself.
pub const PROBE_RETRY_FACTOR: u32 = 3;

impl Config {
    /// The check the constant above PROMISES. The previous validation compared
    /// the bare `probe_timeout_secs` — half of what the probe can actually
    /// consume — and the promise lived only in the docstring.
    ///
    /// # The comparator is the shortest run that USES the backend
    ///
    /// **Not `no_backend_secs`.** That run never touches the endpoint, so the
    /// probe protects nothing there — and comparing against it made the crate's
    /// own defaults FAIL their own validation (`10 * 4 = 40s` against a 30s
    /// budget). A guard that fires on the shipped defaults is one that gets
    /// silenced on day one; this project already rejected that exact shape for
    /// the retry warning.
    /// Checks that `endpoint` is usable as a base URL, and NAMES the field when
    /// it is not — the same contract every other check in [`Config::validate`]
    /// keeps.
    ///
    /// # Why here, and not left to the preflight
    ///
    /// Every other field is range-checked at load time; this one was not, so a
    /// value that is not a URL travelled all the way to the first request and
    /// failed there — one layer and several seconds away from the typo, wearing
    /// a transport error's clothes. A configuration mistake should be reported
    /// as a configuration mistake, at the moment the configuration is read.
    ///
    /// # Scheme and host, and deliberately nothing more
    ///
    /// This is not a URL parser and must not become one — no dependency, and no
    /// hand-rolled percent-decoding, IPv6-literal or IDNA handling, whose bugs
    /// would start REJECTING endpoints that work. What it rules out is the
    /// class of value that cannot possibly work: a missing or unknown scheme,
    /// and an empty or whitespace-carrying host. A syntactically fine endpoint
    /// pointing at nothing is still the preflight's business, and always was.
    ///
    /// # Errors
    ///
    /// [`ConfigError`] naming `endpoint` and what was wrong with it.
    ///
    /// # Complexity
    ///
    /// `O(n)` in the endpoint's length.
    fn validate_endpoint(&self) -> Result<(), ConfigError> {
        let Some(after_scheme) = ENDPOINT_SCHEMES
            .iter()
            .find_map(|s| self.endpoint.strip_prefix(s))
        else {
            return Err(ConfigError(format!(
                "endpoint must start with one of {ENDPOINT_SCHEMES:?}; got {:?} — a value that \
                 is not a URL fails at the first request instead of here, a layer away from \
                 the typo",
                self.endpoint
            )));
        };
        // `next()` on a `split` always yields, so the fallback is unreachable —
        // it is written rather than unwrapped because this is production code.
        let host = after_scheme
            .split(AUTHORITY_TERMINATORS)
            .next()
            .unwrap_or("");
        if host.is_empty() {
            return Err(ConfigError(format!(
                "endpoint has no host: {:?}",
                self.endpoint
            )));
        }
        if host.chars().any(char::is_whitespace) {
            return Err(ConfigError(format!(
                "endpoint host contains whitespace: {:?}",
                self.endpoint
            )));
        }
        Ok(())
    }

    fn validate_probe_window(&self) -> Result<(), ConfigError> {
        let worst = self.probe_timeout_secs * (1 + PROBE_RETRY_FACTOR as u64);
        let shortest_backend_run = self
            .budgets
            .happy_secs
            .min(self.budgets.large_payload_secs)
            .min(self.budgets.injected_secs);
        if worst > shortest_backend_run {
            return Err(ConfigError(format!(
                "probe_timeout_secs={} can consume {}s with its retry, more than the shortest \
                 backend run budget ({}s): the probe would outlast the run it protects",
                self.probe_timeout_secs, worst, shortest_backend_run
            )));
        }
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes the tests that touch the REAL process environment.
    ///
    /// Env vars are process-global, and `cargo test` runs a binary's tests on
    /// multiple threads by default, so two tests on different threads can
    /// otherwise observe each other's `MAGI_SMOKE_*` mutations — the crate
    /// itself pulls in `serial_test` for exactly this reason. The harness does
    /// not take that dependency, so this lock is the same isolation, hand
    /// rolled with only the standard library, and scoped to the handful of
    /// tests below that actually read or write the ambient environment.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// RAII guard that unsets a `MAGI_SMOKE_*` variable on drop. Combined with
    /// [`ENV_LOCK`], a test using this guard cannot leave the variable set for
    /// whichever test runs next on the same thread — even if the assertion
    /// between `set` and drop panics, since unwinding still runs `Drop`.
    struct EnvVarGuard(&'static str);

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            std::env::set_var(key, value);
            Self(key)
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            std::env::remove_var(self.0);
        }
    }

    #[test]
    fn unknown_field_is_a_parse_error_not_a_warning() {
        let toml = r#"
            endpoint = "http://localhost:11434"
            totally_unknown = 3
        "#;
        let err = Config::from_str(toml).unwrap_err();
        assert!(
            format!("{err}").contains("totally_unknown"),
            "the error must NAME the field; a generic parse error sends the reader hunting"
        );
    }

    #[test]
    fn a_budget_beyond_the_ceiling_is_rejected_and_the_field_is_named() {
        // A cap that can never be reached in a session anybody watches is not a
        // cap. The ceiling is deliberately high — a local GPU serialises the
        // three mages — so what it rejects is a number that stopped being one.
        let mut cfg = Config::default();
        cfg.budgets.happy_secs = MAX_BUDGET_SECS + 1;
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("happy_secs"),
            "the message must name the field: {err}"
        );
        cfg.budgets.happy_secs = MAX_BUDGET_SECS;
        assert!(
            cfg.validate().is_ok(),
            "the ceiling itself is a legal value; only beyond it is not"
        );
    }

    #[test]
    fn the_proxys_upstream_bound_is_the_longest_backend_budget_and_ignores_the_offline_one() {
        // The proxy must never be what cuts first, so its bound tracks the
        // LONGEST run it can be serving. Pinning only the default would pass
        // against a function that returned a constant, so each budget is
        // raised past the others in turn.
        let cfg = Config::default();
        assert_eq!(
            cfg.longest_backend_budget(),
            Duration::from_secs(cfg.budgets.large_payload_secs),
            "with the shipped defaults the large-payload run is the longest"
        );
        for raise in [
            |b: &mut Budgets| b.happy_secs = MAX_BUDGET_SECS,
            |b: &mut Budgets| b.large_payload_secs = MAX_BUDGET_SECS,
            |b: &mut Budgets| b.injected_secs = MAX_BUDGET_SECS,
        ] {
            let mut cfg = Config::default();
            raise(&mut cfg.budgets);
            assert_eq!(
                cfg.longest_backend_budget(),
                Duration::from_secs(MAX_BUDGET_SECS),
                "every backend budget must be able to set the bound"
            );
        }
        // The offline run sends nothing through the proxy, so its budget must
        // not pull the bound down — a shorter one there would cut forwards
        // belonging to runs it has nothing to do with.
        let mut offline_is_tiny = Config::default();
        offline_is_tiny.budgets.no_backend_secs = 1;
        assert_eq!(
            offline_is_tiny.longest_backend_budget(),
            Config::default().longest_backend_budget(),
            "no_backend_secs is deliberately outside this maximum"
        );
    }

    #[test]
    fn an_endpoint_that_is_not_a_url_is_rejected_and_the_field_is_named() {
        // Every other field had a range check that NAMES it; this one had
        // none, so a typo reached the preflight and failed there — one layer
        // and several seconds from the mistake, looking like a backend that
        // was not up.
        for bad in [
            "",                        // nothing at all
            "localhost:11434",         // the commonest slip: no scheme
            "ftp://localhost:11434",   // a scheme, but not one we speak
            "http://",                 // scheme and no host
            "http:// localhost:11434", // a host that cannot be one
        ] {
            let cfg = Config {
                endpoint: bad.to_string(),
                ..Config::default()
            };
            let Err(err) = cfg.validate() else {
                panic!("{bad:?} must be rejected, not carried to the first request");
            };
            let err = err.to_string();
            assert!(
                err.contains("endpoint"),
                "the message must NAME the field, or the reader goes hunting: {err}"
            );
        }
        // The other half, and it is what stops the check passing by refusing
        // everything: real endpoints — including one with a path, which is
        // legal input this must not start rejecting — still load.
        for good in [
            "http://localhost:11434",
            "https://example.invalid",
            "http://127.0.0.1:8080/v1",
        ] {
            let cfg = Config {
                endpoint: good.to_string(),
                ..Config::default()
            };
            assert!(
                cfg.validate().is_ok(),
                "{good:?} is a usable endpoint and must not be rejected"
            );
        }
        // And through the environment, which is the HIGHEST-precedence path
        // (R30): a check that only sees the file leaves the override unguarded.
        let via_env =
            Config::apply_env_override("MAGI_SMOKE_ENDPOINT", "localhost:11434", Config::default())
                .expect_err("the env path goes through the same validation as the file path");
        assert!(format!("{via_env}").contains("endpoint"));
    }

    #[test]
    fn a_zero_timeout_is_rejected_and_the_field_is_named() {
        let toml = r#"
            endpoint = "http://localhost:11434"
            probe_timeout_secs = 0
        "#;
        let err = Config::from_str(toml).unwrap_err();
        assert!(format!("{err}").contains("probe_timeout_secs"));
    }

    #[test]
    fn env_values_go_through_the_same_range_validation_as_file_values() {
        // The declared precedence is env > file > built-in, so a range check that
        // only looks at the file leaves the HIGHEST-precedence path unguarded.
        let err =
            Config::apply_env_override("MAGI_SMOKE_PROBE_TIMEOUT_SECS", "0", Config::default())
                .unwrap_err();
        assert!(format!("{err}").contains("probe_timeout_secs"));
    }

    /// A raw value for `key` that is VALID and DIFFERENT from the built-in
    /// default, so applying it must be observable in the resulting config.
    ///
    /// The catch-all does not return a placeholder: it panics, naming the key.
    /// A default arm here would let a newly declared `ENV_OVERRIDES` entry be
    /// "covered" by a value that changes nothing, and the coverage test would
    /// report success while checking that entry not at all — the exact shape of
    /// defect this pair of test and list exists to close.
    fn representative_value_for(key: &str) -> &'static str {
        match key {
            "MAGI_SMOKE_ENDPOINT" => "http://example.invalid:9999",
            "MAGI_SMOKE_PROBE_TIMEOUT_SECS" => "5",
            "MAGI_SMOKE_PAYLOAD_TARGET_BYTES" => "300000",
            "MAGI_SMOKE_RUN_PAYLOAD_BYTES" => "4096",
            "MAGI_SMOKE_BUDGET_HAPPY_SECS" => "121",
            "MAGI_SMOKE_BUDGET_LARGE_SECS" => "301",
            "MAGI_SMOKE_BUDGET_INJECTED_SECS" => "181",
            "MAGI_SMOKE_BUDGET_NO_BACKEND_SECS" => "31",
            unknown => panic!(
                "{unknown} was added to ENV_OVERRIDES without a representative value here; \
                 add one, or the coverage test cannot check that entry at all"
            ),
        }
    }

    #[test]
    fn every_declared_env_override_is_actually_handled() {
        // The guard the `ENV_OVERRIDES` docstring PROMISED and did not have.
        // Without it, `MAGI_SMOKE_RUN_PAYLOAD_BYTES` sat in the list for a whole
        // milestone with no arm in `apply_env_override`, and the comment
        // claiming a test enforced coverage is what kept anyone from looking.
        //
        // # How it tells "handled" from "fell through", mechanically
        //
        // Two independent checks, because either one alone can be satisfied by
        // an arm that does nothing:
        //
        // 1. **No `Err`.** Every key here starts with `MAGI_SMOKE_`, so a key
        //    with no arm of its own reaches the catch-all guard
        //    `other if other.starts_with("MAGI_SMOKE_")` and is refused as an
        //    "unknown override". Falling through is therefore observable from
        //    outside, and it is what this half detects.
        // 2. **The config CHANGED.** `Config` derives `PartialEq`, and every
        //    value from `representative_value_for` differs from the built-in
        //    default, so an arm that parses and then discards the value leaves
        //    the config equal to the base and fails here.
        for (key, field) in ENV_OVERRIDES {
            let raw = representative_value_for(key);
            let applied =
                Config::apply_env_override(key, raw, Config::default()).unwrap_or_else(|e| {
                    panic!(
                        "{key} is declared in ENV_OVERRIDES for `{field}` but \
                         apply_env_override has no arm for it: {e}"
                    )
                });
            assert_ne!(
                applied,
                Config::default(),
                "{key} was accepted but changed nothing, so `{field}` is not really \
                 overridable — an operator would debug a value they believe they set"
            );
        }
    }

    #[test]
    fn the_run_payload_has_a_range_and_violating_it_names_the_field() {
        // Every numeric value has a range and the message NAMES the field; this
        // one had neither, so a zero-byte run payload was accepted and every
        // backend run would have analysed nothing while still reporting.
        let zero = Config {
            run_payload_bytes: 0,
            ..Config::default()
        };
        let err = zero
            .validate()
            .expect_err("a zero-byte run payload must be rejected");
        assert!(
            format!("{err}").contains("run_payload_bytes"),
            "the error must NAME the field: {err}"
        );
        // The boundary itself, from both sides: one byte below is refused and
        // exactly the minimum is accepted. Asserting only the rejection would
        // pass just as well against a check that refuses every value.
        let at_minimum = Config {
            run_payload_bytes: MIN_RUN_PAYLOAD_BYTES,
            ..Config::default()
        };
        assert!(
            at_minimum.validate().is_ok(),
            "the minimum itself must be accepted, or the bound is off by one"
        );
        // And the same range applies through the environment, which is the
        // HIGHEST-precedence path (R30).
        let via_env =
            Config::apply_env_override("MAGI_SMOKE_RUN_PAYLOAD_BYTES", "0", Config::default())
                .expect_err("the env path goes through the same ranges as the file path");
        assert!(format!("{via_env}").contains("run_payload_bytes"));
    }

    #[test]
    fn absent_file_yields_defaults_and_says_so() {
        // Reads the real process environment via `load_or_default`, so it
        // shares ENV_LOCK with the two env-mutating tests below.
        let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let (cfg, announcement) = Config::load_or_default(None).unwrap();
        assert_eq!(cfg.endpoint, "http://localhost:11434");
        assert!(
            announcement.contains("built-in defaults"),
            "a silent default makes 'I pointed where I meant' indistinguishable from \
             'I pointed at the default without knowing'"
        );
    }

    #[test]
    fn an_unmatched_magi_smoke_var_is_rejected_and_named() {
        // This is the Err branch of `reject_unknown_smoke_vars` — until this
        // test, nothing in the suite ever set an unmatched `MAGI_SMOKE_*`
        // variable, so a guard that silently accepted everything would have
        // passed the whole suite. Exercised through `load_or_default`, the
        // real (and only) call site, not the bare function, so the test
        // proves the integration, not just the unit.
        let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        const TYPO: &str = "MAGI_SMOKE_ENDPOINTS"; // trailing 's' — not in ENV_OVERRIDES
        let _guard = EnvVarGuard::set(TYPO, "http://example.invalid");
        let err = Config::load_or_default(None).unwrap_err();
        assert!(
            format!("{err}").contains(TYPO),
            "the error must NAME the unmatched variable, not just say something is wrong"
        );
    }

    #[test]
    fn a_correctly_spelled_override_still_loads() {
        // Companion to the test above: proves `reject_unknown_smoke_vars`
        // rejects the TYPO specifically, not every `MAGI_SMOKE_*` variable —
        // without this, the previous test could pass because the guard
        // rejects everything, which is not the property being verified.
        let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _guard = EnvVarGuard::set("MAGI_SMOKE_ENDPOINT", "http://example.invalid:9999");
        let (cfg, _origin) = Config::load_or_default(None).unwrap();
        assert_eq!(cfg.endpoint, "http://example.invalid:9999");
    }

    #[test]
    // The name is deliberately not shouting `PASS` as the plan wrote it: the
    // project's naming standard is snake_case for functions and clippy enforces
    // it. The emphasis lives here instead — this is the test that keeps the
    // shipped defaults from failing the very validation they ship with.
    fn the_shipped_defaults_pass_their_own_probe_window_validation() {
        // 10 * (1+3) = 40 <= min(120, 300, 180) = 120. The test that keeps the
        // guard honest: without it, a poorly chosen range would make the built-in
        // config invalid and nobody would notice until the first `cargo run`.
        assert!(Config::default().validate_probe_window().is_ok());
    }

    #[test]
    fn the_built_in_defaults_can_actually_run_a_backend_scenario() {
        // Criterion 1 of this milestone is `cargo run` with NO arguments. With
        // zero seats the harness builds, starts, and cannot execute a single
        // backend run — configured-looking and useless. And exactly one seat
        // must NOT support the reasoning control, or the mixed-trio property has
        // nothing to exercise.
        let c = Config::default();
        assert_eq!(c.seats.len(), 3);
        assert_eq!(
            c.seats
                .iter()
                .filter(|s| !s.supports_reasoning_control)
                .count(),
            1
        );
    }

    #[test]
    fn the_example_file_matches_the_built_in_defaults() {
        // Two sources of truth that nothing compares is how one of them silently
        // becomes wrong. The example exists to explain the WHY of each key, not
        // to hold different values.
        let from_file = Config::from_str(include_str!("../magi-smoke.toml.example")).unwrap();
        assert_eq!(from_file, Config::default());
    }
}
