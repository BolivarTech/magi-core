// TARGET: orchestrator.rs
// EXPECT: catch-all arm
// A classifier nested inside another is STILL a classifier. Its catch-all is flagged, because the
// rule is about the subject being matched, not about how deeply it sits.

fn f() {
    match outcome {
        A => {
            let _n = match err {
                Http => 1,
                _ => 0,
            };
            1
        }
        B => 2,
    };
}
