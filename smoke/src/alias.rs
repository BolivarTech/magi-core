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
     nothing to smoke-test. `tree` is the default; use \
     `--no-default-features --features published` for the other mode."
);

#[cfg(feature = "tree")]
pub use magi_core_tree as magi_core;

#[cfg(feature = "published")]
pub use magi_core_pub as magi_core;

/// Which source this binary was built against. Printed at startup so a run's
/// output always says what it tested — a mode that has to be inferred from the
/// invocation is a mode that gets misreported.
pub const MODE: &str = if cfg!(feature = "tree") {
    "tree"
} else {
    "published"
};
