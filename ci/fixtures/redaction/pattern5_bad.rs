// TARGET: orchestrator.rs
// EXPECT: catch-all arm
fn f() {
    match outcome {
        A => 1,
        _ => 2,
    };
}
