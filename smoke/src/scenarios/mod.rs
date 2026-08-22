// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-18

//! The scenario registry, one submodule per milestone that contributed scenarios.
//!
//! - `e1` — the twelve harness scenarios (`S1, S2, S2b, S4, S5, S6, S7, S14, S15, S16, S20, S21`).
//! - `e2` — the ones `e1` deferred that this stage does implement (`S3`, `S8`-`S13`).
//!   `S17`-`S19` belong to stage E3 — the `published` mode, which cannot run before `4.0.0` is
//!   on crates.io — and are deliberately absent here.
//! - `f` — the axis-F scenarios of MS2 (`S-F1`, `S-F2a`, `S-F2b`, `S-F3`, `S-F4`, `S-F5`), three
//!   of which run; the other three are out of scope from here, each with its reason in its own
//!   name so the row still says why.
//!
//! Split per milestone rather than living directly here, so each has a sibling to land in
//! without reshaping this file.

mod e1;
mod e2;
mod f;

pub use e1::e1_scenarios;
pub use e2::e2_scenarios;
pub use f::f_scenarios;

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
