// TARGET: orchestrator.rs
// EXPECT: catch-all arm
// The outer match must stay guarded AFTER a nested one closes. Losing its state there was a real
// false negative: every remaining arm went unwatched, this catch-all among them.

fn f() {
    match outcome {
        A => {
            let _n = match limit {
                0 => "zero",
                _ => "some",
            };
            1
        }
        _ => 2,
    };
}
