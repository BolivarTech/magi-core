// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-08-18

//! The twelve E1 scenarios: `S1, S2, S2b, S4, S5, S6, S7, S14, S15, S16, S20, S21`.
//!
//! Split into its own `e1` submodule (rather than living directly here) so a
//! later `e2` submodule — the six scenarios this stage deliberately does not
//! implement (`S3`, `S8`-`S13` minus the ones already covered, `S17`-`S19`) —
//! has a sibling to land in without reshaping this file.

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
