// Author: Julian Bolivar
// Version: 4.0.0
// Date: 2026-08-23

//! Single import point for `magi-core`, whichever source it came from.
//!
//! The two dependency modes are build-time facts, not runtime flags: a compiled
//! binary cannot change which crate it links against. Selecting neither or both
//! is a configuration error, so it fails at compile time rather than producing a
//! binary that cannot decide what it is testing.

#[cfg(all(feature = "tree", feature = "published"))]
compile_error!(
    "features `tree` and `published` are mutually exclusive: they select the SOURCE of \
     magi-core. Pick one. Note that `--all-features` activates both and therefore cannot \
     work in this package."
);

#[cfg(not(any(feature = "tree", feature = "published")))]
compile_error!(
    "one of `tree` or `published` must be enabled: without a source for magi-core there is \
     nothing to smoke-test. `tree` is the default. The `published` mode still RESOLVES but \
     no longer COMPILES: this harness implements LlmProvider, whose complete() returns \
     Completion since 4.0.0, and the pin is 3.2. See the FEATURE_MATRIX note in main.rs."
);

#[cfg(feature = "tree")]
pub use magi_core_tree as magi_core;

// OUT OF SERVICE since 4.1.0. The assertion is the whole of this mode now, and it
// must be the ONLY diagnostic a build emits -- a reader who sees it buried under
// nineteen type errors learns nothing the errors did not already say.
//
// MEASURED (2026-09-05): with `magi_core` still aliased to `magi_core_pub`,
// `cargo check --no-default-features --features published` produced 20 errors --
// this assertion plus 19 from the 3.2 API. `compile_error!` does not abort before
// type checking, so the alias below points at the TREE crate: the rest of the
// harness type-checks and this message stands alone. Nothing reads that alias --
// the build stops here.
#[cfg(feature = "published")]
compile_error!(concat!(
    "the `published` harness mode is out of service since 4.1.0: its pin resolves to ",
    "magi-core 3.2, whose LlmProvider::complete 4.0.0 broke. ",
    "NOTE: `--all-features` activates this feature, so this failure is EXPECTED there ",
    "and is not a defect. Re-pointing needs five changes together: move the pin, ",
    "migrate the body to the 4.x API, remove this assertion, flip --build-matrix back ",
    "to expecting green, and update the FEATURE_MATRIX row. It is maintenance for ",
    "after 4.1.0 publishes."
));

#[cfg(feature = "published")]
pub use magi_core_tree as magi_core;

/// Which source this binary was built against. Printed at startup so a run's
/// output always says what it tested — a mode that has to be inferred from the
/// invocation is a mode that gets misreported.
pub const MODE: &str = if cfg!(feature = "tree") {
    "tree"
} else {
    "published"
};
