// TARGET: orchestrator.rs
fn f() {
    match outcome {
        A => one(),
        B => two(),
    }
}
fn later() {
    match something_else {
        _ => three(),
    }
}
