// TARGET: orchestrator.rs
// EXPECT: catch-all arm
fn f() {
    match outcome {
        A => one(),
        _ => two(),
    }
}
