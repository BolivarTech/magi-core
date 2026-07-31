// TARGET: orchestrator.rs
// A `match` nested inside an arm is entitled to its own catch-all: it is a different decision, over
// a different subject. Only arms belonging to a watched match may not have one.

fn f() {
    match outcome {
        A => {
            let _n = match limit {
                0 => "zero",
                _ => "some",
            };
            1
        }
        B => 2,
    };
}
