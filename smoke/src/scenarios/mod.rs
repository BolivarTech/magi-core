// Author: Julian Bolivar
// Version: 4.0.0
// Date: 2026-08-23

//! The scenario registry, one submodule per milestone that contributed scenarios.
//!
//! - `e1` — the twelve harness scenarios (`S1, S2, S2b, S4, S5, S6, S7, S14, S15, S16, S20, S21`).
//! - `e2` — the ones `e1` deferred that this stage does implement (`S3`, `S8`-`S13`).
//!   `S17`-`S19` were stage E3's and none of them will arrive. `S17` was REDESIGNED into
//!   `ci/check_packaged_consumer.sh`, a local pre-publish gate that compiles the crate's
//!   examples against the `cargo package` tarball — it has nothing to do with the `published`
//!   mode and waits for nothing. `S18` (did docs.rs build this version) is out of scope
//!   permanently: it exists only after publishing, so it is a gate that cannot stop anything.
//!   `S19` was cancelled with the rest of the post-publish scope.
//! - `e` — the axis-E scenarios of MS3 (`S-E2`, `S-E3`). `S-E1` and `S-E4` are not here on
//!   purpose: one is a grep over the source and the other is the absence of state, and neither
//!   is visible from a run.
//! - `f` — the axis-F scenarios of MS2 (`S-F1`, `S-F2a`, `S-F2b`, `S-F3`, `S-F4`, `S-F5`), three
//!   of which run; the other three are out of scope from here, each with its reason in its own
//!   name so the row still says why.
//! - `r5` — the two endpoint-down scenarios of MS3 (`S-R5a`, `S-R5b`), the only ones that
//!   milestone declares non-waivable: the abort criterion lives in the join loop's interaction
//!   with the abort guard, which a unit test approximates and only a run exercises.
//!
//! Split per milestone rather than living directly here, so each has a sibling to land in
//! without reshaping this file.

mod e;
mod e1;
mod e2;
mod f;
mod r5;

pub use e::e_scenarios;
pub use e1::e1_scenarios;
pub use e2::e2_scenarios;
pub use f::f_scenarios;
pub use r5::r5_scenarios;

#[cfg(test)]
mod tests {
    // Exercises the RE-EXPORT specifically (`scenarios::e1_scenarios`, not
    // `e1::e1_scenarios`'s own internal tests, which call the definition
    // directly via `super::*`). `main.rs` will call it through this same
    // path once wired; until then this is the re-export's only consumer,
    // which is also the reason to have it: a public path with no caller is
    // exactly what this project's own standards forbid.
    use super::e1_scenarios;

    #[test]
    fn the_module_reexports_all_twelve_scenarios() {
        assert_eq!(e1_scenarios().len(), 12);
    }
}
